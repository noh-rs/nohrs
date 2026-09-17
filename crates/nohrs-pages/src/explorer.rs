mod entries;
pub(crate) mod file_ops;
mod list_setup;
mod navigation;
/// The split-view container that owns one or more panes (`docs/explorer-essentials.md` §3).
mod page;
mod preview;
mod search;
mod state;
mod types;
/// Rendering of a single explorer pane: header, sidebar, listing, and preview.
pub mod view;

#[cfg(test)]
mod tests;

pub use page::ExplorerPage;
pub use state::ExplorerPane;
// Part of `status_for_footer`'s signature, so it has to travel with it. The rest
// of `types` stays internal.
pub use types::StatusLevel;

use gpui::{Context, IntoElement, Render, Window};

impl Render for ExplorerPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        view::render(self, window, cx)
    }
}
