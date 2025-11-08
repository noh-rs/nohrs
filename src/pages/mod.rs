use gpui::AnyElement;

pub mod explorer;
pub mod search;
pub mod git;
pub mod s3;
pub mod extensions;
pub mod settings;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageKind {
    Explorer,
    Search,
    Git,
    S3,
    Extensions,
    Settings,
}

impl PageKind {
    pub fn label(&self) -> &'static str {
        match self {
            PageKind::Explorer => "エクスプローラ",
            PageKind::Search => "検索",
            PageKind::Git => "Git",
            PageKind::S3 => "S3",
            PageKind::Extensions => "拡張機能",
            PageKind::Settings => "設定",
        }
    }

    pub fn icon_name(&self) -> gpui_component::IconName {
        use gpui_component::IconName;
        match self {
            PageKind::Explorer => IconName::Folder,
            PageKind::Search => IconName::Search,
            PageKind::Git => IconName::File,
            PageKind::S3 => IconName::Folder,
            PageKind::Extensions => IconName::LayoutDashboard,
            PageKind::Settings => IconName::Settings,
        }
    }

    pub fn all() -> Vec<PageKind> {
        vec![
            PageKind::Explorer,
            PageKind::Search,
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
