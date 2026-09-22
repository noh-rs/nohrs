//! What travels over the socket, and how it is framed.
//!
//! Deliberately small. The socket carries requests to *write*, questions about
//! state, and notices that something changed — never a search. Readers open the
//! index themselves ([ADR
//! 0010](../../../../docs/adr/0010-indexd-owns-the-index-writer.md)), so a
//! daemon that is down, busy or a version behind costs freshness and nothing
//! else.
//!
//! One JSON object per line. A line is a frame: it needs no length prefix to
//! parse, survives a partial read, and can be watched with `nc` while
//! debugging — which is worth more here than the bytes a binary encoding would
//! save, because the volume is one message per file change at most.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use nohrs_services::search::indexer::{IndexReport, Refresh};
use serde::{Deserialize, Serialize};

/// The protocol version, refused on sight when it does not match.
///
/// Bumped whenever a message changes shape. A daemon and a client from
/// different builds must not guess at each other: the client stops the daemon
/// it cannot talk to and starts its own, which is only safe because the daemon
/// is short-lived and owns nothing but the writer.
pub const VERSION: u32 = 1;

/// What a client asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "kebab-case")]
pub enum Request {
    /// Sent first on every connection, before anything else is asked.
    Hello {
        /// The client's [`VERSION`].
        version: u32,
    },
    /// Report what the index holds.
    Status,
    /// Bring the index up to date now, and report what the pass did.
    Refresh {
        /// Whether to re-read everything or only what changed.
        full: bool,
    },
    /// Stop the daemon, without waiting for it to go idle.
    Stop,
}

/// What the daemon answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "kebab-case")]
pub enum Response {
    /// Answer to [`Request::Hello`].
    Welcome {
        /// The daemon's [`VERSION`].
        version: u32,
        /// The daemon's process id, for diagnosis.
        pid: u32,
    },
    /// Answer to [`Request::Status`].
    Status {
        /// Where the index is.
        index_path: String,
        /// The tree it covers.
        content_root: String,
        /// How many documents it holds.
        documents: u64,
        /// Whether the daemon is watching that tree right now.
        watching: bool,
        /// How many clients are holding the daemon up, this one included.
        clients: usize,
    },
    /// Answer to [`Request::Refresh`].
    Refreshed {
        /// Documents written.
        indexed: usize,
        /// Files the pass left alone.
        unchanged: usize,
        /// Documents dropped because the file is gone.
        removed: usize,
    },
    /// A pass committed. The cue for a reader to
    /// [`nohrs_services::search::indexer::IndexReader::reload`].
    Committed,
    /// How far along an indexing pass is, in the range `0.0..=1.0`.
    Progress {
        /// The fraction done.
        done: f32,
    },
    /// The request could not be served. Not fatal to the connection.
    Failed {
        /// What went wrong, as a sentence.
        message: String,
    },
}

impl Response {
    /// Whether this arrived unasked-for, rather than answering a request.
    ///
    /// A client waiting for an answer has to read past these rather than
    /// mistake the next notice for its reply.
    pub fn is_notice(&self) -> bool {
        matches!(self, Self::Committed | Self::Progress { .. })
    }
}

impl Request {
    /// The refresh this request asks for.
    pub fn refresh(&self) -> Refresh {
        match self {
            Self::Refresh { full: true } => Refresh::Everything,
            _ => Refresh::Changed,
        }
    }
}

impl From<IndexReport> for Response {
    fn from(report: IndexReport) -> Self {
        Self::Refreshed {
            indexed: report.indexed,
            unchanged: report.unchanged,
            removed: report.removed,
        }
    }
}

/// Writes one message as a line.
pub fn write_frame<T: Serialize>(writer: &mut impl Write, message: &T) -> Result<()> {
    let line = serde_json::to_string(message).context("cannot encode a message")?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

/// Reads one message from a line, or `None` at end of stream.
///
/// End of stream is how this protocol says "the other side is gone", and it is
/// the only way that is reliable: a client killed outright runs no shutdown
/// code, but the kernel still closes its end.
pub fn read_frame<T: serde::de::DeserializeOwned>(reader: &mut impl BufRead) -> Result<Option<T>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let message = serde_json::from_str(line.trim_end())
        .with_context(|| format!("cannot decode `{}`", line.trim_end()))?;
    Ok(Some(message))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_message_survives_the_round_trip() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &Request::Refresh { full: true }).unwrap();
        write_frame(&mut buffer, &Request::Status).unwrap();

        let mut reader = buffer.as_slice();
        assert_eq!(
            read_frame::<Request>(&mut reader).unwrap(),
            Some(Request::Refresh { full: true })
        );
        assert_eq!(
            read_frame::<Request>(&mut reader).unwrap(),
            Some(Request::Status)
        );
        // The end of the stream is not an error; it is the other side leaving.
        assert_eq!(read_frame::<Request>(&mut reader).unwrap(), None);
    }

    #[test]
    fn one_frame_is_one_line() {
        let mut buffer = Vec::new();
        write_frame(
            &mut buffer,
            &Response::Failed {
                message: "a message\nwith a newline in it".to_string(),
            },
        )
        .unwrap();

        assert_eq!(
            buffer.iter().filter(|byte| **byte == b'\n').count(),
            1,
            "a newline inside a message broke the framing"
        );
    }

    #[test]
    fn notices_are_told_apart_from_answers() {
        assert!(Response::Committed.is_notice());
        assert!(Response::Progress { done: 0.5 }.is_notice());
        assert!(!Response::Welcome { version: 1, pid: 7 }.is_notice());
        assert!(
            !Response::Failed {
                message: String::new()
            }
            .is_notice()
        );
    }

    #[test]
    fn a_refresh_says_which_kind_it_is() {
        assert_eq!(Request::Refresh { full: false }.refresh(), Refresh::Changed);
        assert_eq!(
            Request::Refresh { full: true }.refresh(),
            Refresh::Everything
        );
    }
}
