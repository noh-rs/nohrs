//! The daemon: it owns the index writer and the watcher, and nothing else.
//!
//! Searches do not come through here. Readers open the index themselves, so a
//! daemon that is down, busy, or a version behind costs freshness and never an
//! answer ([ADR
//! 0009](../../../../docs/adr/0009-indexd-owns-the-index-writer.md)).

#![cfg(unix)]

use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result};
use nohrs_services::search::indexer::{IndexManager, IndexReport, Refresh};
use nohrs_services::search::watcher::FileWatcher;

use crate::endpoint::{Claim, Endpoint};
use crate::lease::{Lease, Leases};
use crate::protocol::{self, Request, Response, VERSION};

/// How long the watcher gathers changes before handing them over.
///
/// The same debounce the app used when it owned the watcher: long enough that
/// saving a file once is one update rather than three.
const DEBOUNCE: Duration = Duration::from_secs(2);

/// How often progress is republished while a pass runs.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// What the daemon is asked to be when it starts.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// How long to stay up after the last client leaves.
    pub grace: Duration,
    /// The index to write, and the tree it covers. `None` for the defaults,
    /// which is what everything but a test wants.
    pub index: Option<(PathBuf, PathBuf)>,
}

impl Settings {
    /// The daemon a client starts: the default index, the default grace.
    pub fn with_grace(grace: Duration) -> Self {
        Self { grace, index: None }
    }
}

/// Why [`serve`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// This process was the daemon, and has now stopped.
    Served,
    /// Another process is already the daemon, so this one did nothing.
    AlreadyRunning,
}

/// Runs the daemon until its clients have gone and the grace period is out.
///
/// Returns [`Outcome::AlreadyRunning`] rather than failing when another daemon
/// holds the claim: two clients racing to start one is ordinary, and the loser
/// has nothing to report.
pub fn serve(endpoint: &Endpoint, settings: &Settings) -> Result<Outcome> {
    endpoint.prepare()?;
    let Some(claim) = Claim::take(&endpoint.lock)? else {
        return Ok(Outcome::AlreadyRunning);
    };

    // Holding the claim means nothing is listening on that path, so whatever is
    // there was left by a daemon that died. Binding would fail on it.
    remove_if_present(&endpoint.socket)
        .with_context(|| format!("cannot clear {}", endpoint.socket.display()))?;
    let listener = UnixListener::bind(&endpoint.socket)
        .with_context(|| format!("cannot listen on {}", endpoint.socket.display()))?;

    let daemon = Arc::new(Daemon::open(settings)?);
    daemon.start_watching();
    daemon.refresh_in_background(Refresh::Changed);

    let janitor = std::thread::spawn({
        let daemon = Arc::clone(&daemon);
        let endpoint = endpoint.clone();
        move || {
            daemon.leases.wait_until_idle();
            daemon.stop(&endpoint);
        }
    });

    for connection in listener.incoming() {
        if daemon.stopping.load(Ordering::Acquire) {
            break;
        }
        match connection {
            Ok(stream) => {
                let lease = daemon.leases.take();
                let daemon = Arc::clone(&daemon);
                // One thread per client. There are a handful of them — the app,
                // a launcher, the odd command — and a thread that spends its
                // life blocked on a read costs a stack.
                std::thread::spawn(move || {
                    if let Err(error) = daemon.serve_client(stream, lease) {
                        tracing::debug!("client gone: {error:#}");
                    }
                });
            }
            Err(error) => tracing::warn!("cannot accept a client: {error}"),
        }
    }

    drop(listener);
    if let Err(error) = remove_if_present(&endpoint.socket) {
        tracing::warn!("cannot remove {}: {error}", endpoint.socket.display());
    }
    if janitor.join().is_err() {
        tracing::warn!("the shutdown thread panicked");
    }
    drop(claim);
    Ok(Outcome::Served)
}

/// Removes `path`, treating "there was nothing there" as success.
///
/// Which it is, twice over here: the socket left by a daemon that died has to
/// go before this one can bind, and the socket this one bound has to go when it
/// stops. Either may already be gone, and neither is worth a word about it.
fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The daemon's state: the writer, the clients, and the way out.
struct Daemon {
    index: IndexManager,
    leases: Arc<Leases>,
    stopping: AtomicBool,
    watching: AtomicBool,
    /// Where notices go. Keyed so a client that leaves can be taken out
    /// without having to compare senders.
    subscribers: Mutex<HashMap<u64, std::sync::mpsc::Sender<Response>>>,
    next_subscriber: AtomicU64,
    _watcher: Mutex<Option<FileWatcher>>,
}

impl Daemon {
    fn open(settings: &Settings) -> Result<Self> {
        let index = match &settings.index {
            Some((index_path, content_root)) => {
                IndexManager::new_with_path(index_path.clone(), content_root.clone())?
            }
            None => IndexManager::new()?,
        };
        Ok(Self {
            index,
            leases: Leases::new(settings.grace),
            stopping: AtomicBool::new(false),
            watching: AtomicBool::new(false),
            subscribers: Mutex::new(HashMap::new()),
            next_subscriber: AtomicU64::new(0),
            _watcher: Mutex::new(None),
        })
    }

    /// Stops accepting clients and wakes the accept loop.
    ///
    /// The order matters. The flag goes up first, then a connection of our own
    /// unblocks `accept` — which has to happen while the socket is still bound,
    /// or there would be nothing to connect to — and the accept loop unlinks
    /// the socket once it has broken out. A client that got in between sees its
    /// connection close before a `Welcome`, and starts a daemon of its own,
    /// which is the path it takes when there was never one.
    fn stop(&self, endpoint: &Endpoint) {
        self.stopping.store(true, Ordering::Release);
        if let Err(error) = UnixStream::connect(&endpoint.socket) {
            // Nothing to wake: the loop has already left, or the socket is
            // gone because someone else tore it down.
            tracing::debug!("the accept loop was already awake: {error}");
        }
    }

    fn start_watching(self: &Arc<Self>) {
        let (changes_tx, changes_rx) = async_channel::bounded(100);
        let root = self.index.content_root().to_path_buf();
        match FileWatcher::new(root.clone(), changes_tx, DEBOUNCE) {
            Ok(watcher) => {
                self.set_watcher(Some(watcher));
                self.watching.store(true, Ordering::Release);
            }
            Err(error) => {
                // Losing the watcher costs live updates, not the index: the
                // pass at the next start catches up. Say so rather than exit.
                tracing::error!("cannot watch {}: {error}", root.display());
                return;
            }
        }

        let daemon = Arc::clone(self);
        std::thread::spawn(move || {
            // Ends when the watcher is dropped and closes the channel.
            while let Ok(paths) = changes_rx.recv_blocking() {
                match daemon.index.process_changes(&paths) {
                    Ok(()) => daemon.notify(Response::Committed),
                    Err(error) => tracing::warn!("cannot apply changes: {error:#}"),
                }
            }
        });
    }

    fn set_watcher(&self, watcher: Option<FileWatcher>) {
        let mut held = self._watcher.lock().unwrap_or_else(PoisonError::into_inner);
        *held = watcher;
    }

    /// Starts a pass without waiting for it, for the one at startup.
    fn refresh_in_background(self: &Arc<Self>, refresh: Refresh) {
        let daemon = Arc::clone(self);
        std::thread::spawn(move || match daemon.refresh(refresh) {
            Ok(report) => tracing::info!(
                "index up to date: {} written, {} unchanged, {} removed",
                report.indexed,
                report.unchanged,
                report.removed
            ),
            Err(error) => tracing::error!("indexing failed: {error:#}"),
        });
    }

    /// Runs a pass, publishing progress while it goes.
    ///
    /// Two passes at once serialize on the writer, so a client asking during
    /// the startup pass waits for it rather than racing it.
    fn refresh(self: &Arc<Self>, refresh: Refresh) -> Result<IndexReport> {
        let (progress_tx, progress_rx) = postage::watch::channel_with(0.0f32);
        let done = Arc::new(AtomicBool::new(false));
        let ticker = std::thread::spawn({
            let daemon = Arc::clone(self);
            let done = Arc::clone(&done);
            move || {
                while !done.load(Ordering::Acquire) {
                    std::thread::sleep(PROGRESS_INTERVAL);
                    let fraction = *progress_rx.borrow();
                    daemon.notify(Response::Progress { done: fraction });
                }
            }
        });

        let report = self.index.index_home(refresh, Some(progress_tx));
        done.store(true, Ordering::Release);
        if ticker.join().is_err() {
            tracing::warn!("the progress thread panicked");
        }
        if report.is_ok() {
            self.notify(Response::Committed);
        }
        report
    }

    /// Sends a notice to every connected client.
    fn notify(&self, notice: Response) {
        let subscribers = self
            .subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for sender in subscribers.values() {
            // A client whose writer has already gone is about to be taken out
            // of the list by its own thread; nothing to do about it here.
            if let Err(error) = sender.send(notice.clone()) {
                tracing::trace!("a client missed a notice: {error}");
            }
        }
    }

    /// Serves one client until its end of the socket closes.
    ///
    /// `lease` is taken by value and never handed on: this function returning
    /// is what releases it, and this function returns when the client's end
    /// closes — which the kernel does whatever became of the client.
    fn serve_client(self: &Arc<Self>, stream: UnixStream, lease: Lease) -> Result<()> {
        let id = self.next_subscriber.fetch_add(1, Ordering::Relaxed);
        let (outbound, outbox) = std::sync::mpsc::channel::<Response>();
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, outbound);

        // Everything written to this client goes through one thread, so a
        // notice can never land in the middle of an answer.
        let writer = std::thread::spawn({
            let mut stream = stream.try_clone().context("cannot split the socket")?;
            move || {
                for message in outbox {
                    if let Err(error) = protocol::write_frame(&mut stream, &message) {
                        tracing::debug!("cannot write to a client: {error:#}");
                        break;
                    }
                }
                if let Err(error) = stream.flush() {
                    tracing::trace!("cannot flush to a client: {error}");
                }
            }
        });

        let served = self.read_requests(stream, id);

        // Taking the sender out closes the outbox, which ends the writer.
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        if writer.join().is_err() {
            tracing::warn!("a client's writer thread panicked");
        }
        drop(lease);
        served
    }

    fn read_requests(self: &Arc<Self>, stream: UnixStream, id: u64) -> Result<()> {
        let mut reader = BufReader::new(stream);
        while let Some(request) = protocol::read_frame::<Request>(&mut reader)? {
            let answer = self.answer(&request);
            let sent = self
                .subscribers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&id)
                .map(|sender| sender.send(answer));
            match sent {
                Some(Ok(())) => {}
                // The client stopped reading, which is its way of leaving.
                Some(Err(_)) | None => break,
            }
            if matches!(request, Request::Stop) {
                self.leases.release();
                break;
            }
        }
        Ok(())
    }

    fn answer(self: &Arc<Self>, request: &Request) -> Response {
        match request {
            Request::Hello { version } if *version != VERSION => Response::Failed {
                message: format!("this daemon speaks version {VERSION}, not {version}"),
            },
            Request::Hello { .. } => Response::Welcome {
                version: VERSION,
                pid: std::process::id(),
            },
            Request::Status => self.status(),
            Request::Refresh { .. } => match self.refresh(request.refresh()) {
                Ok(report) => report.into(),
                Err(error) => Response::Failed {
                    message: format!("{error:#}"),
                },
            },
            Request::Stop => Response::Welcome {
                version: VERSION,
                pid: std::process::id(),
            },
        }
    }

    fn status(&self) -> Response {
        let (index_path, content_root) = (self.index.index_path(), self.index.content_root());
        let documents = match self.index.document_count() {
            Ok(documents) => documents,
            Err(error) => {
                return Response::Failed {
                    message: format!("{error:#}"),
                };
            }
        };
        Response::Status {
            index_path: index_path.display().to_string(),
            content_root: content_root.display().to_string(),
            documents,
            watching: self.watching.load(Ordering::Acquire),
            clients: self.leases.live(),
        }
    }
}

/// Where the daemon's own binary is, next to the one that wants it.
///
/// Looked up beside the running executable rather than on `PATH`: a client must
/// start the daemon from its own build, or the two speak different protocol
/// versions the moment a release is installed alongside a development one.
pub fn binary_beside_current() -> Result<PathBuf> {
    let current = std::env::current_exe().context("cannot find this executable")?;
    let directory = current
        .parent()
        .context("this executable has no directory")?;
    let candidate = directory.join(crate::BINARY_NAME);
    if candidate.is_file() {
        return Ok(candidate);
    }
    anyhow::bail!("no {} beside {}", crate::BINARY_NAME, current.display())
}
