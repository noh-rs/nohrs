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
