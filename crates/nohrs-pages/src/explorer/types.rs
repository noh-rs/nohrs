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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
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
