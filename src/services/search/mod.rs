use std::fs;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

use crate::core::errors::{Error, Result};

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, Query, QueryParser};
use tantivy::schema::{Field, Schema, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, Term};
use walkdir::WalkDir;

/// Represents the logical kind of an indexed entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

impl EntryKind {
    fn as_str(&self) -> &'static str {
        match self {
            EntryKind::File => "file",
            EntryKind::Directory => "dir",
            EntryKind::Symlink => "symlink",
            EntryKind::Other => "other",
        }
    }

    fn from_str(value: &str) -> EntryKind {
        match value {
            "file" => EntryKind::File,
            "dir" => EntryKind::Directory,
            "symlink" => EntryKind::Symlink,
            _ => EntryKind::Other,
        }
    }
}

/// Determines which index to use during search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDomain {
    FileNames,
    FileContents,
}

/// Controls how the query string is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    Exact,
    Fuzzy { distance: u8 },
}

/// Restricts search to either the entire root or a subtree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchScope {
    Root,
    Subtree(PathBuf),
}

/// Defines a search request.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub domain: SearchDomain,
    pub query: String,
    pub mode: QueryMode,
    pub scope: SearchScope,
    pub limit: usize,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            domain: SearchDomain::FileNames,
            query: String::new(),
            mode: QueryMode::Exact,
            scope: SearchScope::Root,
            limit: 10,
        }
    }
}

/// A single search hit enriched with scoring information.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub path: PathBuf,
    pub name: String,
    pub kind: EntryKind,
    pub score: f32,
}

/// Describes the configuration of a search index storage.
#[derive(Debug, Clone)]
pub struct SearchServiceConfig {
    pub base_path: PathBuf,
    pub index_path: PathBuf,
}

impl SearchServiceConfig {
    pub fn new(base_path: impl Into<PathBuf>, index_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            index_path: index_path.into(),
        }
    }
}

struct ScopeFilter {
    root: String,
    prefix: String,
}

impl ScopeFilter {
    fn new(path: &Path) -> ScopeFilter {
        let root = path.to_string_lossy().to_string();
        let mut prefix = root.clone();
        if !prefix.ends_with(MAIN_SEPARATOR) {
            prefix.push(MAIN_SEPARATOR);
        }
        ScopeFilter { root, prefix }
    }

    fn matches(&self, candidate: &Path) -> bool {
        let candidate_str = candidate.to_string_lossy();
        candidate_str == self.root || candidate_str.starts_with(&self.prefix)
    }
}

/// Provides indexing and querying capabilities for filesystem search.
pub struct SearchService {
    base_path: PathBuf,
    file_name_index: SearchIndex,
    file_content_index: SearchIndex,
}

impl SearchService {
    pub fn initialize(config: SearchServiceConfig) -> Result<Self> {
        let base_path = fs::canonicalize(&config.base_path)?;
        if !base_path.is_dir() {
            return Err(Error::Other(format!(
                "base path '{}' is not a directory",
                base_path.display()
            )));
        }

        fs::create_dir_all(&config.index_path)?;

        let file_name_index =
            SearchIndex::create(config.index_path.join("filenames"), IndexKind::FileNames)?;
        let file_content_index =
            SearchIndex::create(config.index_path.join("contents"), IndexKind::FileContents)?;

        Ok(Self {
            base_path,
            file_name_index,
            file_content_index,
        })
    }

    /// Rebuilds both file name and file content indexes from scratch.
    pub fn rebuild(&self) -> Result<()> {
        let records = self.collect_records()?;
        self.file_name_index.rebuild(records.iter())?;
        self.file_content_index.rebuild(records.iter())?;
        Ok(())
    }

    /// Executes a search request and returns ranked hits.
    pub fn search(&self, request: &SearchRequest) -> Result<Vec<SearchHit>> {
        if request.limit == 0 {
            return Ok(Vec::new());
        }

        let scope_filter = self.scope_filter(&request.scope)?;
        let index = match request.domain {
            SearchDomain::FileNames => &self.file_name_index,
            SearchDomain::FileContents => &self.file_content_index,
        };
        index.search(
            &request.query,
            request.mode,
            scope_filter.as_ref(),
            request.limit,
        )
    }

    fn scope_filter(&self, scope: &SearchScope) -> Result<Option<ScopeFilter>> {
        match scope {
            SearchScope::Root => Ok(None),
            SearchScope::Subtree(path) => {
                let full = fs::canonicalize(path)?;
                if !full.starts_with(&self.base_path) {
                    return Err(Error::InvalidScope(full));
                }
                Ok(Some(ScopeFilter::new(&full)))
            }
        }
    }

    fn collect_records(&self) -> Result<Vec<FileRecord>> {
        let mut records = Vec::new();
        for entry in WalkDir::new(&self.base_path)
            .into_iter()
            .filter_map(|res| res.ok())
        {
            let path = entry.into_path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(md) => md,
                Err(_) => continue,
            };

            let kind = if metadata.is_dir() {
                EntryKind::Directory
            } else if metadata.is_file() {
                EntryKind::File
            } else if metadata.file_type().is_symlink() {
                EntryKind::Symlink
            } else {
                EntryKind::Other
            };

            if kind == EntryKind::Directory && path == self.base_path {
                continue;
            }

            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());

            let content = if kind == EntryKind::File {
                fs::read_to_string(&path).ok()
            } else {
                None
            };

            records.push(FileRecord {
                path,
                name,
                kind,
                content,
            });
        }

        Ok(records)
    }
}

struct FileRecord {
    path: PathBuf,
    name: String,
    kind: EntryKind,
    content: Option<String>,
}

#[derive(Clone, Copy)]
enum IndexKind {
    FileNames,
    FileContents,
}

impl IndexKind {
    fn accepts(&self, record: &FileRecord) -> bool {
        match self {
            IndexKind::FileNames => true,
            IndexKind::FileContents => record.kind == EntryKind::File && record.content.is_some(),
        }
    }
}

struct SearchIndex {
    kind: IndexKind,
    index: Index,
    reader: IndexReader,
    path_field: Field,
    name_field: Field,
    kind_field: Field,
    content_field: Option<Field>,
}

impl SearchIndex {
    fn create(path: PathBuf, kind: IndexKind) -> Result<Self> {
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(&path)?;

        let mut builder = Schema::builder();
        let path_field = builder.add_text_field("path", STRING | STORED);
        let name_field = builder.add_text_field("name", TEXT | STORED);
        let kind_field = builder.add_text_field("kind", STRING | STORED);
        let content_field = match kind {
            IndexKind::FileNames => None,
            IndexKind::FileContents => Some(builder.add_text_field("content", TEXT)),
        };
        let schema = builder.build();
        let index = Index::create_in_dir(&path, schema)?;
        let reader = index.reader()?;
        Ok(Self {
            kind,
            index,
            reader,
            path_field,
            name_field,
            kind_field,
            content_field,
        })
    }

    fn rebuild<'a>(&self, records: impl Iterator<Item = &'a FileRecord>) -> Result<()> {
        let mut writer = self.index.writer(50_000_000)?;
        writer.delete_all_documents()?;
        for record in records {
            if !self.kind.accepts(record) {
                continue;
            }
            let mut document = doc! {
                self.path_field => record.path.to_string_lossy().to_string(),
                self.name_field => record.name.clone(),
                self.kind_field => record.kind.as_str(),
            };

            if let Some(content_field) = self.content_field {
                if let Some(content) = &record.content {
                    document.add_text(content_field, content.clone());
                } else {
                    continue;
                }
            }

            writer.add_document(document)?;
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    fn search(
        &self,
        query: &str,
        mode: QueryMode,
        scope: Option<&ScopeFilter>,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let query = self.build_query(query, mode)?;
        let searcher = self.reader.searcher();
        let doc_count = searcher.num_docs() as usize;
        let fetch_limit = if doc_count == 0 {
            limit.max(1)
        } else {
            let desired = limit.saturating_mul(10).max(50);
            desired.min(doc_count).max(limit.min(doc_count))
        };
        let top_docs = searcher.search(&query, &TopDocs::with_limit(fetch_limit))?;
        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let retrieved = searcher.doc(doc_address)?;
            let path = retrieved
                .get_first(self.path_field)
                .and_then(|v| v.as_text())
                .unwrap_or("")
                .to_string();
            let name = retrieved
                .get_first(self.name_field)
                .and_then(|v| v.as_text())
                .unwrap_or("")
                .to_string();
            let kind = retrieved
                .get_first(self.kind_field)
                .and_then(|v| v.as_text())
                .map(EntryKind::from_str)
                .unwrap_or(EntryKind::Other);

            results.push(SearchHit {
                path: PathBuf::from(path),
                name,
                kind,
                score,
            });
        }

        if let Some(filter) = scope {
            results.retain(|hit| filter.matches(&hit.path));
        }

        if results.len() > limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    fn build_query(&self, query_str: &str, mode: QueryMode) -> Result<Box<dyn Query>> {
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let main_query: Box<dyn Query> = match mode {
            QueryMode::Exact => {
                let parser = QueryParser::for_index(&self.index, vec![self.primary_field()]);
                parser
                    .parse_query(query_str)
                    .map_err(|e| Error::Other(format!("failed to parse query: {e}")))?
            }
            QueryMode::Fuzzy { distance } => {
                let term = Term::from_field_text(self.primary_field(), query_str);
                Box::new(FuzzyTermQuery::new(term, distance, true))
            }
        };
        clauses.push((Occur::Must, main_query));
        if clauses.len() == 1 {
            Ok(clauses.pop().unwrap().1)
        } else {
            Ok(Box::new(BooleanQuery::new(clauses)))
        }
    }

    fn primary_field(&self) -> Field {
        match self.kind {
            IndexKind::FileNames => self.name_field,
            IndexKind::FileContents => self.content_field.unwrap_or(self.name_field),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_service() -> Result<(SearchService, TempDir, TempDir)> {
        let base_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();

        fs::write(base_dir.path().join("root.txt"), "hello root")?;
        fs::create_dir_all(base_dir.path().join("src"))?;
        fs::write(
            base_dir.path().join("src").join("lib.rs"),
            "fn main() {}\nhello world",
        )?;
        fs::create_dir_all(base_dir.path().join("docs"))?;
        fs::write(
            base_dir.path().join("docs").join("guide.md"),
            "Getting started guide",
        )?;
        fs::create_dir_all(base_dir.path().join("notes"))?;
        fs::write(
            base_dir.path().join("notes").join("todo.txt"),
            "remember the milk",
        )?;

        let service =
            SearchService::initialize(SearchServiceConfig::new(base_dir.path(), index_dir.path()))?;
        service.rebuild()?;

        Ok((service, base_dir, index_dir))
    }

    #[test]
    fn search_file_names_in_root_scope() -> Result<()> {
        let (service, _base, _index) = create_service()?;

        let request = SearchRequest {
            domain: SearchDomain::FileNames,
            query: "guide".to_string(),
            mode: QueryMode::Exact,
            scope: SearchScope::Root,
            limit: 5,
        };

        let hits = service.search(&request)?;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("guide.md"));
        assert_eq!(hits[0].kind, EntryKind::File);
        Ok(())
    }

    #[test]
    fn search_file_names_in_subtree_scope() -> Result<()> {
        let (service, base, _index) = create_service()?;

        let request = SearchRequest {
            domain: SearchDomain::FileNames,
            query: "lib".to_string(),
            mode: QueryMode::Exact,
            scope: SearchScope::Subtree(base.path().join("src")),
            limit: 5,
        };

        let hits = service.search(&request)?;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("lib.rs"));
        Ok(())
    }

    #[test]
    fn fuzzy_search_file_name() -> Result<()> {
        let (service, _base, _index) = create_service()?;

        let request = SearchRequest {
            domain: SearchDomain::FileNames,
            query: "gaide".to_string(),
            mode: QueryMode::Fuzzy { distance: 1 },
            scope: SearchScope::Root,
            limit: 5,
        };

        let hits = service.search(&request)?;
        assert!(hits.iter().any(|hit| hit.path.ends_with("guide.md")));
        Ok(())
    }

    #[test]
    fn search_file_contents() -> Result<()> {
        let (service, base, _index) = create_service()?;

        let request = SearchRequest {
            domain: SearchDomain::FileContents,
            query: "remember".to_string(),
            mode: QueryMode::Exact,
            scope: SearchScope::Subtree(base.path().join("notes")),
            limit: 5,
        };

        let hits = service.search(&request)?;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("todo.txt"));
        Ok(())
    }

    #[test]
    fn search_with_zero_limit_returns_no_hits() -> Result<()> {
        let (service, _base, _index) = create_service()?;

        let request = SearchRequest {
            domain: SearchDomain::FileNames,
            query: "root".to_string(),
            mode: QueryMode::Exact,
            scope: SearchScope::Root,
            limit: 0,
        };

        let hits = service.search(&request)?;
        assert!(hits.is_empty());
        Ok(())
    }

    #[test]
    fn search_rejects_scope_outside_root() -> Result<()> {
        let (service, base, _index) = create_service()?;

        let outside_scope = base
            .path()
            .parent()
            .expect("tempdir should have a parent")
            .to_path_buf();

        let request = SearchRequest {
            domain: SearchDomain::FileNames,
            query: "guide".to_string(),
            mode: QueryMode::Exact,
            scope: SearchScope::Subtree(outside_scope),
            limit: 5,
        };

        let result = service.search(&request);
        assert!(matches!(result, Err(Error::InvalidScope(_))));
        Ok(())
    }
}
