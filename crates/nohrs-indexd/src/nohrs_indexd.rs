//! `nohrs-indexd`: the process that owns nohrs's search index writer, and the
//! file watcher beside it.
//!
//! tantivy allows one writer across all processes, and the watcher's output is
//! a stream of write requests, so the two belong together: with them in one
//! place, "the daemon is running" and "the index is keeping up with the
//! filesystem" are the same fact, and a client can check it.
//!
//! What this is *not* is a search server. Readers open the index directly, from
//! any number of processes at once, so nothing here is on the path of answering
//! a query. A daemon that is down, busy, or a version behind costs freshness
//! and never an answer — which is what makes it safe for it to be a daemon at
//! all ([ADR
//! 0010](../../../../docs/adr/0010-indexd-owns-the-index-writer.md)).
//!
//! It is not installed, registered or started at login either. The first
//! process that wants it starts it, and it stops itself once its last client
//! has been gone for a grace period — so the only thing a user has to quit is
//! nohrs.

/// Talking to the daemon, and starting one when there is none.
#[cfg(unix)]
pub mod client;
/// Where the daemon listens, and how exactly one of them gets to.
pub mod endpoint;
/// How the daemon knows when to stop.
pub mod lease;
/// What travels over the socket, and how it is framed.
pub mod protocol;
/// The daemon itself.
#[cfg(unix)]
pub mod server;

/// The name of the daemon's executable, looked for beside the one that wants it.
pub const BINARY_NAME: &str = "nohrs-indexd";

#[cfg(unix)]
pub use client::{Client, Notices};
pub use endpoint::Endpoint;
pub use lease::DEFAULT_GRACE;
