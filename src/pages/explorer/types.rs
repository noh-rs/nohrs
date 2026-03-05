use gpui::{Pixels, Point};
use std::time::Instant;

// GUI 非依存の型は core から再エクスポート
pub use crate::core::types::{SearchFileResult, SearchMatch, SearchType, SortKey};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewMode {
    List,
    Grid,
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
