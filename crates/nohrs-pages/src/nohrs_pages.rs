//! Top-level page views for the nohrs application, including the explorer,
//! git, S3, extensions, and settings pages, along with the root view that
//! hosts them.

use gpui::AnyElement;

/// The file explorer page and its supporting state and views.
pub mod explorer;
/// The extensions management page.
pub mod extensions;
/// The git page.
pub mod git;
/// The application root view that hosts the sidebar and active page.
pub mod root;
/// The S3 page.
pub mod s3;
// removed search
/// The settings page.
pub mod settings;

pub use root::RootView;

/// Identifies which top-level page is currently selected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageKind {
    /// The file explorer page.
    Explorer,

    /// The git page.
    Git,
    /// The S3 page.
    S3,
    /// The extensions page.
    Extensions,
    /// The settings page.
    Settings,
}

impl PageKind {
    /// Returns the human-readable label for this page.
    pub fn label(&self) -> &'static str {
        match self {
            PageKind::Explorer => "Explorer",
            PageKind::Git => "Git",
            PageKind::S3 => "S3",
            PageKind::Extensions => "Extensions",
            PageKind::Settings => "Settings",
        }
    }

    /// Returns the asset path of the icon representing this page.
    pub fn icon_path(&self) -> &'static str {
        match self {
            PageKind::Explorer => "icons/folder.svg",

            PageKind::Git => "icons/github.svg",
            PageKind::S3 => "icons/database.svg",
            PageKind::Extensions => "icons/layout-dashboard.svg",
            PageKind::Settings => "icons/settings.svg",
        }
    }

    /// Returns all page kinds in display order.
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
    /// Renders the page into an element tree.
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> AnyElement
    where
        Self: Sized;
}
