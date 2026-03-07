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

/// 検索オプションを一括で渡すための構造体
#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub query: String,
    pub search_type: SearchType,
    pub match_case: bool,
    pub match_whole_word: bool,
    pub use_regex: bool,
}

impl SearchQuery {
    pub fn new(query: String) -> Self {
        Self {
            query,
            search_type: SearchType::All,
            match_case: false,
            match_whole_word: false,
            use_regex: false,
        }
    }
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
