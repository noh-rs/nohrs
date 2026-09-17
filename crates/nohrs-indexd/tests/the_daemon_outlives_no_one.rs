//! The daemon against a real socket, a real index and a real process.
//!
//! The unit tests cover the counting ([`nohrs_indexd::lease`]) and the claim
//! ([`nohrs_indexd::endpoint`]) on their own. This is the part neither can
//! reach: that the count is really the kernel's — that a client which is
//! *killed* rather than dropped still releases its hold — and that the daemon
//! goes away on its own afterwards.

#![cfg(unix)]
// `clippy.toml` bans the synchronous `std::fs` helpers so blocking IO does not
// reach the GPUI foreground thread. A test binary has no UI thread, and staging
// a real tree on disk is the point here.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use nohrs_indexd::client::Client;
use nohrs_indexd::protocol::Response;
use nohrs_indexd::{Endpoint, Notices};
use nohrs_services::search::control::IndexControl;
use nohrs_services::search::indexer::Refresh;

/// Short enough to keep the test quick, long enough that starting a process
/// does not read as the grace period expiring.
const GRACE: Duration = Duration::from_secs(2);

/// How long to give the daemon to do something before calling it stuck.
const PATIENCE: Duration = Duration::from_secs(30);

/// How long the daemon has to say nothing before it counts as having nothing
/// left to say. Longer than the watcher's debounce, so that the pass a pass
/// provokes is inside the window rather than after it.
const SILENCE: Duration = Duration::from_secs(4);

struct Fixture {
    _home: tempfile::TempDir,
    endpoint: Endpoint,
    daemon: Child,
}

impl Fixture {
    /// A daemon over its own index, its own tree and its own socket.
    fn start() -> Self {
        let home = tempfile::tempdir().unwrap();
        let content = home.path().join("content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("notes.txt"), "a needle in here\n").unwrap();

        let endpoint = Endpoint::under(home.path().join("run"));
        let daemon = Command::new(env!("CARGO_BIN_EXE_nohrs-indexd"))
            .arg("--endpoint-dir")
            .arg(endpoint.socket.parent().unwrap())
            .arg("--index-dir")
            .arg(home.path().join("index"))
            .arg("--content-root")
            .arg(&content)
            .arg("--grace")
            .arg(GRACE.as_secs().to_string())
            // The stderr sink honours `RUST_LOG`; an inherited one would change
            // what the child does and has nothing to do with what is asserted.
            .env_remove("RUST_LOG")
            .spawn()
            .unwrap();

        Self {
            _home: home,
            endpoint,
            daemon,
        }
    }

    fn connect(&self) -> Client {
        let deadline = Instant::now() + PATIENCE;
        loop {
            match Client::connect(&self.endpoint) {
                Ok(client) => return client,
                Err(error) if Instant::now() >= deadline => {
                    panic!("the daemon never came up: {error:#}")
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Whatever the test did, nothing is left running. Reported rather than
        // asserted: this runs while a failing test is unwinding, and panicking
        // here would bury that test's own message. A daemon left behind holds a
        // socket the next run would trip over.
        if let Ok(None) = self.daemon.try_wait()
            && let Err(error) = self.daemon.kill()
        {
            eprintln!("could not stop the daemon under test: {error}");
        }
        if let Err(error) = self.daemon.wait() {
            eprintln!("could not reap the daemon under test: {error}");
        }
    }
}

/// Waits for `condition`, or gives up and says what it was waiting for.
fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("gave up waiting for {what}");
}

fn gone(path: &Path) -> bool {
    !path.exists()
}

/// Waits for the daemon to stop announcing commits and says how many it made
/// getting there, or gives up on one that will not stop.
///
/// Giving up rather than waiting is the point: a daemon whose passes provoke
/// the next one is not slow, it is never going to be quiet, and a test that
/// waits for it to be hangs the job instead of reporting what it found.
fn falls_quiet(heard: &std::sync::mpsc::Receiver<()>) -> usize {
    let quiet_by = Instant::now() + PATIENCE;
    let mut commits = 0;
    while heard.recv_timeout(SILENCE).is_ok() {
        commits += 1;
        assert!(
            Instant::now() < quiet_by,
            "the daemon committed {commits} times without going quiet, \
             with nothing touching its tree: each pass is provoking the next"
        );
    }
    commits
}

#[test]
fn the_daemon_serves_its_index_and_then_sees_itself_out() {
    let fixture = Fixture::start();
    let client = fixture.connect();

    let report = client.refresh(Refresh::Changed).unwrap();
    assert!(
        report.indexed > 0 || report.unchanged > 0,
        "the daemon indexed nothing at all: {report:?}"
    );

    let status = client.status().unwrap();
    assert!(status.documents.unwrap_or_default() > 0, "{status:?}");
    assert!(status.watching, "the daemon is not watching its tree");

    // The last client leaving starts the countdown, and nothing else does.
    let started = Instant::now();
    drop(client);
    until("the daemon to stop", || gone(&fixture.endpoint.socket));
    assert!(
        started.elapsed() >= GRACE,
        "the daemon left before the grace period was out"
    );
}

#[test]
fn a_client_that_is_killed_releases_its_hold_like_any_other() {
    let fixture = Fixture::start();
    // Held until the other client is counted, so that the daemon cannot
    // decide it has no clients in between.
    let probe = fixture.connect();

    // A client in a process of its own, so that it can be killed outright: no
    // destructor runs, no goodbye is sent, and the daemon has nothing but the
    // kernel closing the socket to go on.
    let mut held = Command::new(env!("CARGO_BIN_EXE_holds_a_connection"))
        .arg(&fixture.endpoint.socket)
        .spawn()
        .unwrap();
    until("the killable client to be counted", || {
        probe.status().unwrap().clients == Some(2)
    });

    // From here the killable client is the only thing holding the daemon up.
    drop(probe);
    std::thread::sleep(GRACE * 2);
    assert!(
        !gone(&fixture.endpoint.socket),
        "the daemon stopped while a client still held it"
    );

    let started = Instant::now();
    held.kill().unwrap();
    held.wait().unwrap();
    until("the daemon to stop", || gone(&fixture.endpoint.socket));

    assert!(
        started.elapsed() >= GRACE,
        "a killed client was not given the same grace period as any other"
    );
}

#[test]
fn a_second_daemon_stands_down_rather_than_fighting_over_the_index() {
    let fixture = Fixture::start();
    let client = fixture.connect();

    // Exactly what a client racing to start one does.
    let second = Command::new(env!("CARGO_BIN_EXE_nohrs-indexd"))
        .arg("--endpoint-dir")
        .arg(fixture.endpoint.socket.parent().unwrap())
        .arg("--grace")
        .arg("1")
        .env_remove("RUST_LOG")
        .status()
        .unwrap();

    assert!(second.success(), "the second daemon failed: {second:?}");
    // The first one is still the daemon, and still answering.
    assert!(client.status().is_ok(), "the first daemon was displaced");
}

#[test]
fn a_client_that_finds_a_daemon_running_uses_it_rather_than_starting_another() {
    let fixture = Fixture::start();
    // Held so the daemon is certainly up before the next client looks for one.
    let first = fixture.connect();

    let second = Client::connect_or_start(&fixture.endpoint).unwrap();

    // Both are talking to the same daemon: it counts them both.
    assert_eq!(second.status().unwrap().clients, Some(2));
    assert_eq!(
        first.status().unwrap().index_path,
        second.status().unwrap().index_path
    );
}

/// The window's reader answers from the segments that existed when it opened,
/// so without this notice a file saved a moment ago is simply not found: the
/// index would be current and the window would not know (`nohrs/src/app.rs`).
#[test]
fn a_subscriber_is_told_when_a_pass_commits() {
    let fixture = Fixture::start();
    // Held so the daemon does not go idle between subscribing and asking.
    let client = fixture.connect();

    // Subscribing has registered this connection by the time it returns: the
    // daemon adds a client to `subscribers` before it reads that client's
    // first request, and the `Welcome` that `subscribe` waits for is itself
    // sent through that subscription. So the refresh below cannot commit
    // before there is anyone to tell.
    //
    // Read on a thread all the same, but collected with a deadline: were that
    // ordering ever to change, a notice that never comes should fail this test
    // rather than hang it, because a hung job on CI says nothing about what
    // broke.
    let notices = Notices::subscribe(&fixture.endpoint).unwrap();
    let (heard_tx, heard) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for notice in notices {
            if notice == Response::Committed && heard_tx.send(()).is_err() {
                eprintln!("a notice arrived after the test had given up waiting");
                return;
            }
        }
    });

    // The daemon commits a pass of its own at startup, and reading a tree
    // updates the access times in it, which the watcher is told about and
    // answers with a pass of its own. Neither is this test's, and the second
    // arrives a debounce interval late, so the daemon is first let fall silent:
    // a notice collected below is then one this test asked for. Without this,
    // the assertion is satisfied by a commit that was already on its way, and a
    // daemon that has stopped announcing passes altogether still passes.
    falls_quiet(&heard);

    client.refresh(Refresh::Everything).unwrap();
    heard
        .recv_timeout(PATIENCE)
        .expect("a pass committed and nothing was told about it");
}

#[test]
fn asking_it_to_stop_does_not_wait_for_the_grace_period() {
    let fixture = Fixture::start();
    let client = fixture.connect();

    let started = Instant::now();
    client.stop().unwrap();
    until("the daemon to stop", || gone(&fixture.endpoint.socket));

    assert!(
        started.elapsed() < GRACE,
        "a stop waited for the grace period anyway"
    );
}

/// The version handshake has to decide something. A connection told its version
/// is not this daemon's could go straight on to `Stop` and be obeyed, which
/// makes the greeting a formality — and `Stop` the one request where being
/// obeyed by a caller you have just disagreed with is not recoverable.
#[cfg(unix)]
#[test]
fn a_caller_that_has_not_agreed_on_a_version_is_not_obeyed() {
    use std::io::{BufRead, BufReader, ErrorKind};
    use std::os::unix::net::UnixStream;

    use nohrs_indexd::protocol::{Request, VERSION, read_frame, write_frame};

    /// What a read on the stranger's connection came back with, once the
    /// notices every connection receives have been passed over.
    enum Heard {
        Answer(Response),
        Ended,
    }

    /// Reads until the daemon answers or lets go, and fails the test if it does
    /// neither within `PATIENCE`: a handshake or close path that regresses
    /// should say so rather than hang the job, because a hung job on CI says
    /// nothing about what broke.
    fn next_answer(reading: &mut impl BufRead) -> Heard {
        loop {
            match read_frame::<Response>(reading) {
                Ok(Some(frame)) if frame.is_notice() => continue,
                Ok(Some(frame)) => return Heard::Answer(frame),
                Ok(None) => return Heard::Ended,
                Err(error) => match error
                    .downcast_ref::<std::io::Error>()
                    .map(std::io::Error::kind)
                {
                    // Writing to a connection the daemon has already dropped
                    // can leave this end reset rather than at a clean end of
                    // stream. Both are the daemon having let go. A timeout is
                    // not: that is the read deadline expiring on a daemon which
                    // neither answered nor closed.
                    Some(ErrorKind::ConnectionReset | ErrorKind::BrokenPipe) => {
                        return Heard::Ended;
                    }
                    _ => panic!("the stranger's connection could not be read: {error:#}"),
                },
            }
        }
    }

    let fixture = Fixture::start();
    // Waits for the daemon and holds a lease, so that what the test observes
    // afterwards is the daemon refusing rather than the grace period passing.
    let client = fixture.connect();

    let mut stranger = UnixStream::connect(&fixture.endpoint.socket).unwrap();
    stranger.set_read_timeout(Some(PATIENCE)).unwrap();
    write_frame(
        &mut stranger,
        &Request::Hello {
            version: VERSION + 1,
        },
    )
    .unwrap();
    let mut reading = BufReader::new(stranger.try_clone().unwrap());
    // Notices go to every connection, so one from the pass the daemon runs at
    // startup can land ahead of the answer. The real client skips them the same
    // way (`Client::request_watching`); reading the first frame and calling it
    // the answer would make this test fail on timing rather than on behaviour.
    let refusal = match next_answer(&mut reading) {
        Heard::Answer(frame) => frame,
        Heard::Ended => panic!("the daemon closed on a greeting instead of refusing it"),
    };
    assert!(
        matches!(refusal, Response::Failed { .. }),
        "a mismatched version was welcomed: {refusal:?}"
    );

    // The connection is over, so this either fails to write or is never read.
    // Both are the daemon declining to act, which is what is being asserted.
    write_frame(&mut stranger, &Request::Stop).ok();

    // Notices queued before the daemon dropped this connection may still be
    // flushed to it, so what is asserted is that nothing but a notice arrives
    // before the stream ends — not that the very next frame is the end.
    if let Heard::Answer(frame) = next_answer(&mut reading) {
        panic!("a caller that never agreed on a version was answered anyway: {frame:?}");
    }
    // The daemon is still here and still serving the client that did greet it.
    assert!(
        fixture.endpoint.socket.exists(),
        "a caller that never agreed on a version stopped the daemon"
    );
    client.status().unwrap();
}

/// Indexing a file opens it, and `notify` reports an open as an event like any
/// other — so a pass that re-indexes whatever the watcher reports is feeding the
/// watcher its own reads. Left that way the daemon re-indexes the tree every
/// debounce interval for as long as it runs, over a tree nobody is touching, on
/// a machine doing nothing.
#[test]
fn a_pass_does_not_feed_the_watcher_its_own_reads() {
    let fixture = Fixture::start();
    let client = fixture.connect();

    let notices = Notices::subscribe(&fixture.endpoint).unwrap();
    let (heard_tx, heard) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for notice in notices {
            if notice == Response::Committed && heard_tx.send(()).is_err() {
                return;
            }
        }
    });
    // The daemon's own pass at startup, and whatever that provoked, first.
    let after_starting = falls_quiet(&heard);
    assert!(after_starting > 0, "the daemon never indexed anything");

    client.refresh(Refresh::Everything).unwrap();

    // The pass itself commits, and the events its own reads raised commit again
    // a debounce later — those passes finding nothing changed, and so reading
    // nothing, which is where it has to stop. What is asserted is that it does
    // stop: a daemon whose passes provoke the next one never goes quiet, so the
    // count is not what separates the two, termination is.
    falls_quiet(&heard);
}
