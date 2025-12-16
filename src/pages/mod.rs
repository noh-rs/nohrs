use gpui::AnyElement;

pub mod explorer;
pub mod extensions;
pub mod git;
pub mod s3;
// removed search
pub mod settings;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageKind {
    Explorer,

    Git,
    S3,
    Extensions,
    Settings,
}

impl PageKind {
    pub fn label(&self) -> &'static str {
        match self {
            PageKind::Explorer => "エクスプローラ",

            PageKind::Git => "Git",
            PageKind::S3 => "S3",
            PageKind::Extensions => "拡張機能",
            PageKind::Settings => "設定",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            PageKind::Explorer => "icons/folder.svg",

            PageKind::Git => "icons/github.svg",
            PageKind::S3 => "icons/database.svg",
            PageKind::Extensions => "icons/layout-dashboard.svg",
            PageKind::Settings => "icons/settings.svg",
        }
    }

    pub fn all() -> Vec<PageKind> {
        vec![
            PageKind::Explorer,
            PageKind::Git,
            PageKind::S3,
            PageKind::Extensions,
            PageKind::Settings,
        ]
    }
}

/// Trait for page rendering
pub trait Page {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> AnyElement
    where
        Self: Sized;
}
