use gpui::{Pixels, Point};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Grid,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    Filename,
    Content,
    All,
}

// Search Data Structures
#[derive(Clone)]
pub struct SearchMatch {
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Clone)]
pub struct SearchFileResult {
    pub path: String,
    pub folder: String,
    pub filename: String,
    pub matches: Vec<SearchMatch>,
}

#[derive(Clone, Copy)]
pub struct ResizingColumn {
    pub column_index: usize,
    pub start_width: f32,
    pub start_x: Point<Pixels>,
}

pub struct LastClickInfo {
    pub row: usize,
    pub timestamp: Instant,
    pub click_count: usize,
}

/// What a status message is reporting, which is three things and not two.
///
/// A filesystem operation can also finish what the user asked for and fail to
/// clear up after itself — a cross-volume cut whose copy landed and whose
/// removal of the source gave up partway, or an overwrite whose staging copy
/// could not be cleared. The entry is where the user wanted it, and something of
/// the operation's own is still sitting somewhere they would not look.
///
/// That has to be its own level. Counting it as [`Self::Info`] tells them
/// "1 item(s) moved" while part of the source is still there, and counting it as
/// [`Self::Error`] puts the source back on the clipboard and has the retry land
/// beside the copy that already succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    /// Done, with nothing left over.
    Info,
    /// Done, with something left behind. The message says where.
    Warning,
    /// Not done.
    Error,
}

/// A transient message surfaced to the user via the footer status bar, used to
/// report failures (e.g. a directory that could not be read or a failed search)
/// instead of only logging them.
#[derive(Clone)]
pub struct StatusMessage {
    pub text: String,
    pub level: StatusLevel,
}

/// Events a single pane emits to its containing split view, so the container can
/// mirror navigation across panes when `synced_panes` is enabled (§3.2).
#[derive(Clone)]
pub enum PaneEvent {
    /// The pane navigated to a new directory (carries the new absolute path).
    Navigated(String),
}
