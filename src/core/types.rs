/// GUI 非依存の型定義
/// pages/explorer/types.rs から移動

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchType {
    Filename,
    Content,
    All,
}

#[derive(Clone, Debug)]
pub struct SearchMatch {
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Clone, Debug)]
pub struct SearchFileResult {
    pub path: String,
    pub folder: String,
    pub filename: String,
    pub matches: Vec<SearchMatch>,
}
