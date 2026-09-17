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
        // Whatever the test did, nothing is left running.
        if let Ok(None) = self.daemon.try_wait() {
            let _ = self.daemon.kill();
        }
        let _ = self.daemon.wait();
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
    let mut notices = Notices::subscribe(&fixture.endpoint).unwrap();

    let told =
        std::thread::spawn(move || notices.find(|notice| matches!(notice, Response::Committed)));
    client.refresh(Refresh::Everything).unwrap();

    assert_eq!(
        told.join().unwrap(),
        Some(Response::Committed),
        "a pass committed and nothing was told about it"
    );
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
