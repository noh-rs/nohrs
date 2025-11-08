#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub enum FileKind {
    File,
    Dir,
    Link,
}
