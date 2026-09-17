//! Talking to the daemon, and starting one when there is none.

#![cfg(unix)]

use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use nohrs_services::search::control::{IndexControl, IndexStatus};
use nohrs_services::search::indexer::{IndexReport, Refresh};

use crate::endpoint::{Claim, Endpoint};
use crate::protocol::{self, Request, Response, VERSION};
use crate::server;

/// How long a client waits for a daemon it just started to come up.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the connection may say nothing at all before it is given up on.
///
/// A pass over a large tree takes minutes, but it publishes progress every
/// 100ms while it does, so silence this long means the daemon is wedged rather
/// than busy.
const SILENCE_TIMEOUT: Duration = Duration::from_secs(30);

/// A connection to the daemon.
///
/// Holding one is what keeps the daemon up: it counts its clients by their open
/// connections, and dropping this closes ours. If this process is killed
/// outright the kernel closes it instead, which is the same thing as far as the
/// daemon is concerned.
pub struct Client {
    connection: Mutex<Connection>,
}

struct Connection {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Client {
    /// Connects to a running daemon, or starts one and connects to that.
    ///
    /// Fails only when no daemon can be reached *and* none can be started;
    /// callers fall back to indexing in-process rather than giving up.
    pub fn connect_or_start(endpoint: &Endpoint) -> Result<Self> {
        match Self::connect(endpoint) {
            Ok(client) => return Ok(client),
            Err(error) => tracing::debug!("no daemon to talk to yet: {error:#}"),
        }
        start(endpoint)?;

        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let mut wait = Duration::from_millis(5);
        loop {
            match Self::connect(endpoint) {
                Ok(client) => return Ok(client),
                Err(error) if Instant::now() >= deadline => {
                    return Err(error).context("the index daemon did not come up");
                }
                Err(_) => {
                    std::thread::sleep(wait);
                    wait = (wait * 2).min(Duration::from_millis(200));
                }
            }
        }
    }

    /// Connects to a daemon that is already running.
    pub fn connect(endpoint: &Endpoint) -> Result<Self> {
        let stream = UnixStream::connect(&endpoint.socket)
            .with_context(|| format!("cannot reach {}", endpoint.socket.display()))?;
        stream
            .set_read_timeout(Some(SILENCE_TIMEOUT))
            .context("cannot set a timeout on the connection")?;
        let reader = BufReader::new(stream.try_clone().context("cannot split the socket")?);
        let client = Self {
            connection: Mutex::new(Connection { stream, reader }),
        };

        match client.request(&Request::Hello { version: VERSION })? {
            Response::Welcome { version, .. } if version == VERSION => Ok(client),
            Response::Welcome { version, .. } => {
                bail!("the running daemon speaks version {version}, not {VERSION}")
            }
            Response::Failed { message } => bail!("{message}"),
            other => bail!("the daemon answered a greeting with {other:?}"),
        }
    }

    /// Asks the daemon to stop, without waiting for its clients to leave.
    pub fn stop(&self) -> Result<()> {
        self.request(&Request::Stop)?;
        Ok(())
    }

    /// Sends one request and reads its answer, stepping over any notices that
    /// arrive in between.
    fn request(&self, request: &Request) -> Result<Response> {
        self.request_watching(request, |_| {})
    }

    /// Sends one request and reads its answer, handing every notice that
    /// arrives while waiting to `notice`.
    ///
    /// The notices are the reason a slow request is not a silent one: a pass
    /// over a cold tree takes minutes and reports its progress throughout.
    fn request_watching(
        &self,
        request: &Request,
        mut notice: impl FnMut(&Response),
    ) -> Result<Response> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let connection = &mut *connection;
        protocol::write_frame(&mut connection.stream, request)?;
        loop {
            let Some(response) = protocol::read_frame::<Response>(&mut connection.reader)? else {
                bail!("the index daemon closed the connection");
            };
            if !response.is_notice() {
                return Ok(response);
            }
            notice(&response);
        }
    }
}

impl IndexControl for Client {
    fn status(&self) -> Result<IndexStatus> {
        match self.request(&Request::Status)? {
            Response::Status {
                index_path,
                content_root,
                documents,
                watching,
                clients,
            } => Ok(IndexStatus {
                index_path: index_path.into(),
                content_root: content_root.into(),
                documents: Some(documents),
                watching,
                clients: Some(clients),
            }),
            Response::Failed { message } => bail!("{message}"),
            other => bail!("the daemon answered a status request with {other:?}"),
        }
    }

    fn refresh_reporting(
        &self,
        refresh: Refresh,
        progress: Option<postage::watch::Sender<f32>>,
    ) -> Result<IndexReport> {
        let full = matches!(refresh, Refresh::Everything);
        let mut progress = progress;
        let answer = self.request_watching(&Request::Refresh { full }, |notice| {
            if let (Response::Progress { done }, Some(sender)) = (notice, progress.as_mut()) {
                *sender.borrow_mut() = *done;
            }
        })?;
        match answer {
            Response::Refreshed {
                indexed,
                unchanged,
                removed,
            } => Ok(IndexReport {
                indexed,
                unchanged,
                removed,
            }),
            Response::Failed { message } => bail!("{message}"),
            other => bail!("the daemon answered a refresh with {other:?}"),
        }
    }
}

/// Starts a daemon, unless one is already starting.
///
/// The claim decides: whoever takes it is the daemon, so a client that cannot
/// take it knows one is coming up and only has to wait. Without this, every
/// client that arrived at once would start a process, and all but one would
/// exit again having done nothing but cost a fork.
fn start(endpoint: &Endpoint) -> Result<()> {
    endpoint.prepare()?;
    match Claim::take(&endpoint.lock)? {
        // Someone else's daemon has the claim, or is about to bind its socket.
        None => Ok(()),
        Some(claim) => {
            // Released before the child starts, so it can take the claim
            // itself; the child is the one that must hold it for its life.
            drop(claim);
            let binary = server::binary_beside_current()?;
            Command::new(&binary)
                .arg("--endpoint-dir")
                .arg(
                    endpoint
                        .socket
                        .parent()
                        .context("the endpoint has no directory")?,
                )
                // Detached from whatever started it: a one-shot command must be
                // able to exit without taking the daemon with it, and the
                // daemon's own log goes to the rolling file the app writes.
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .with_context(|| format!("cannot start {}", binary.display()))?;
            Ok(())
        }
    }
}

/// A stream of the daemon's notices, on a connection of its own.
///
/// Separate from [`Client`] so that reading notices never has to be untangled
/// from waiting for an answer: this connection only ever reads. It is a client
/// like any other, so holding one keeps the daemon up — which is what a running
/// app wants.
pub struct Notices {
    reader: BufReader<UnixStream>,
}

impl Notices {
    /// Subscribes to a running daemon, starting one if there is none.
    pub fn subscribe(endpoint: &Endpoint) -> Result<Self> {
        // Held until this connection is made: it is what proves the daemon is
        // up and speaking the right version, and dropping it first would leave
        // a gap in which the daemon could decide it had no clients left.
        let client = Client::connect_or_start(endpoint)?;

        let mut stream = UnixStream::connect(&endpoint.socket)
            .with_context(|| format!("cannot reach {}", endpoint.socket.display()))?;
        let reader = BufReader::new(stream.try_clone().context("cannot split the socket")?);
        protocol::write_frame(&mut stream, &Request::Hello { version: VERSION })?;
        let mut notices = Self { reader };
        match notices.next() {
            Some(Response::Welcome { .. }) => {}
            other => bail!("the daemon answered a greeting with {other:?}"),
        }
        drop(client);
        Ok(notices)
    }
}

impl Iterator for Notices {
    type Item = Response;

    /// Blocks until the daemon says something, and ends when it goes away.
    fn next(&mut self) -> Option<Self::Item> {
        match protocol::read_frame::<Response>(&mut self.reader) {
            Ok(response) => response,
            // The daemon going away is the ordinary end of this stream, not a
            // thing to report: it stops when its last client does.
            Err(error) => {
                tracing::debug!("the daemon's notices stopped: {error:#}");
                None
            }
        }
    }
}
