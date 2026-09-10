//! The launcher window's root view: a search field over a ranked result list.
//!
//! The layout follows docs/launcher.md §3 and §5 — a single field, nothing
//! below it until something is typed, then a sectioned list of rows carrying an
//! icon, the name, where it lives, and what kind of thing it is.
//!
//! Colours come from the active `gpui-component` theme rather than a palette of
//! this crate's own, so the launcher follows the user's configured light/dark
//! mode and matches the text the `Input` draws for itself.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, HighlightStyle, Hsla, KeyDownEvent,
    ScrollStrategy, SharedString, StyledText, Subscription, Task, UniformListScrollHandle,
    WeakEntity, Window, div, px, uniform_list,
};
use gpui_component::{ActiveTheme, Icon, IconName};
use nohrs_core::telemetry::LogErr;
use nohrs_services::search::file_index::FileNameIndex;

use crate::field::{FieldChanged, FieldStyle, SearchField};
use crate::ranking::{self, LauncherItem};

/// Placeholder shown in the empty field. It doubles as the only onboarding the
/// launcher gets (docs/launcher.md §3).
const PLACEHOLDER: &str = "Search files and folders…";

/// How many rows the list holds. Beyond this the ranking is noise, and every
/// extra row costs a match-index computation.
const MAX_RESULTS: usize = 50;

/// Keystroke-to-results debounce (docs/launcher.md §12 budgets 50ms).
const DEBOUNCE: Duration = Duration::from_millis(50);

/// Rows moved by `PageUp` / `PageDown`.
const PAGE_JUMP: usize = 8;

/// Height of the search field.
pub const SEARCH_BAR_HEIGHT: f32 = 58.0;
/// Height of one result row.
pub const ROW_HEIGHT: f32 = 42.0;
/// Height of the action bar along the bottom.
pub const FOOTER_HEIGHT: f32 = 38.0;
/// Corner radius of the window itself.
pub const WINDOW_RADIUS: f32 = 12.0;
/// Inset of the row list from the window edge, which is what makes a selected
/// row read as a floating pill rather than a full-width band.
const LIST_INSET: f32 = 8.0;

/// Opacity of the foreground tint behind the selected row, and behind a hovered
/// one. Two clearly separated steps: the pointer must never be mistakable for
/// the keyboard selection, which is what `Enter` acts on.
const SELECTED_TINT: f32 = 0.10;
/// Opacity of the tint behind a hovered row.
const HOVER_TINT: f32 = 0.045;

/// Glyph for the platform's command modifier, as it appears on that platform's
/// keyboards.
const SECONDARY_MODIFIER: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl "
};

/// Colours for the search field, taken from the active theme.
fn field_style(cx: &App) -> FieldStyle {
    FieldStyle {
        text: cx.theme().foreground,
        placeholder: cx.theme().muted_foreground,
        cursor: cx.theme().caret,
        selection: cx.theme().selection,
    }
}

/// The launcher's root view.
pub struct LauncherView {
    focus_handle: FocusHandle,
    query_input: Entity<SearchField>,
    query: SharedString,
    // `Arc` rather than `Vec` because the list closure clones this on every
    // frame; an atomic bump is cheaper than copying up to `MAX_RESULTS` rows.
    items: Arc<[LauncherItem]>,
    selected: usize,
    index: Arc<FileNameIndex>,
    home: Option<PathBuf>,
    scroll_handle: UniformListScrollHandle,
    // Dropping this cancels the search it owns. Assigning a new one on every
    // keystroke is therefore both the debounce and the guarantee that a slow
    // search can never land on top of a newer one.
    search_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl LauncherView {
    /// Builds the view over a name index, focusing the search field.
    ///
    /// `home` is the directory the index was built from: it anchors the depth
    /// boost and is the prefix subtitles abbreviate to `~`.
    pub fn new(
        index: Arc<FileNameIndex>,
        home: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query_input = cx.new(|cx| SearchField::new(PLACEHOLDER, field_style(cx), cx));

        let subscription = cx.subscribe(&query_input, |this, _field, _event: &FieldChanged, cx| {
            this.on_query_changed(cx);
        });

        window.focus(&query_input.read(cx).focus_handle(cx));

        Self {
            focus_handle: cx.focus_handle(),
            query_input,
            query: SharedString::default(),
            items: Vec::new().into(),
            selected: 0,
            index,
            home,
            scroll_handle: UniformListScrollHandle::new(),
            search_task: None,
            _subscriptions: vec![subscription],
        }
    }

    /// The rows currently on screen, in rank order.
    pub fn items(&self) -> &[LauncherItem] {
        &self.items
    }

    /// Index of the highlighted row, or `None` when the list is empty.
    pub fn selected_index(&self) -> Option<usize> {
        (!self.items.is_empty()).then_some(self.selected)
    }

    /// The highlighted row, if there is one.
    pub fn selected_item(&self) -> Option<&LauncherItem> {
        self.items.get(self.selected)
    }

    fn on_query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.query_input.read(cx).text().clone();
        if query == self.query {
            return;
        }
        self.query = query;
        self.schedule_search(cx);
    }

    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        let query = self.query.clone();
        if query.trim().is_empty() {
            // Assigning `None` drops any in-flight search, so an earlier query's
            // results cannot repopulate a field the user has just cleared.
            self.search_task = None;
            self.set_items(Vec::new(), cx);
            return;
        }

        let entries = self.index.snapshot();
        let home = self.home.clone();
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let items = cx
                .background_spawn(async move {
                    ranking::rank(&entries, &query, MAX_RESULTS, home.as_deref())
                })
                .await;
            this.update(cx, |this, cx| this.set_items(items, cx))
                .log_err();
        }));
    }

    fn set_items(&mut self, items: Vec<LauncherItem>, cx: &mut Context<Self>) {
        self.items = items.into();
        self.selected = 0;
        if !self.items.is_empty() {
            self.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn set_selected(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        self.selected = index.min(self.items.len().saturating_sub(1));
        self.scroll_handle
            .scroll_to_item(self.selected, ScrollStrategy::Top);
        cx.notify();
    }

    /// Moves to the next row, wrapping to the first past the end.
    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        let Some(last) = self.items.len().checked_sub(1) else {
            return;
        };
        let next = if self.selected >= last {
            0
        } else {
            self.selected + 1
        };
        self.set_selected(next, cx);
    }

    /// Moves to the previous row, wrapping to the last before the start.
    pub fn select_previous(&mut self, cx: &mut Context<Self>) {
        let Some(last) = self.items.len().checked_sub(1) else {
            return;
        };
        let previous = self.selected.checked_sub(1).unwrap_or(last);
        self.set_selected(previous, cx);
    }

    /// Jumps `PAGE_JUMP` rows, clamping at the ends rather than wrapping —
    /// wrapping a page jump loses the reader's place.
    pub fn page_down(&mut self, cx: &mut Context<Self>) {
        self.set_selected(self.selected.saturating_add(PAGE_JUMP), cx);
    }

    /// Jumps `PAGE_JUMP` rows back, clamping at the top.
    pub fn page_up(&mut self, cx: &mut Context<Self>) {
        self.set_selected(self.selected.saturating_sub(PAGE_JUMP), cx);
    }

    /// Opens the selected row with the system handler and closes the launcher.
    pub fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected) else {
            return;
        };
        cx.open_with_system(&item.path);
        self.close(window, cx);
    }

    /// Reveals the selected row in the system file manager and closes.
    pub fn reveal_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected) else {
            return;
        };
        cx.reveal_path(&item.path);
        self.close(window, cx);
    }

    /// Dismisses the launcher window.
    pub fn close(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        // `secondary` is Cmd on macOS and Ctrl elsewhere, which is what the
        // Cmd-prefixed bindings in docs/launcher.md mean on each platform.
        let secondary = modifiers.secondary();
        let handled = match event.keystroke.key.as_str() {
            "escape" => {
                self.close(window, cx);
                true
            }
            "down" if secondary => {
                self.set_selected(self.items.len().saturating_sub(1), cx);
                true
            }
            "up" if secondary => {
                self.set_selected(0, cx);
                true
            }
            "down" => {
                self.select_next(cx);
                true
            }
            "up" => {
                self.select_previous(cx);
                true
            }
            // Emacs-style navigation, which the terminal-minded expect to work
            // wherever there is a list.
            "n" if modifiers.control => {
                self.select_next(cx);
                true
            }
            "p" if modifiers.control => {
                self.select_previous(cx);
                true
            }
            "pagedown" => {
                self.page_down(cx);
                true
            }
            "pageup" => {
                self.page_up(cx);
                true
            }
            "enter" if secondary => {
                self.reveal_selected(window, cx);
                true
            }
            "enter" => {
                self.open_selected(window, cx);
                true
            }
            _ => false,
        };

        if handled {
            // Stop the field from also acting on the key: without this, an arrow
            // press would move the text cursor as well as the selection.
            cx.stop_propagation();
        }
    }
}

impl Focusable for LauncherView {
    /// Focus belongs to the search field: everything is typed, so handing focus
    /// to the launcher must put the caret in the query.
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query_input.read(cx).focus_handle(cx)
    }
}

impl Render for LauncherView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_body = !self.query.trim().is_empty();

        div()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .rounded(px(WINDOW_RADIUS))
            .border_1()
            .border_color(cx.theme().border)
            .overflow_hidden()
            .child(self.render_search_bar(has_body, cx))
            .child(self.render_body(cx))
            .child(self.render_footer(cx))
    }
}

impl LauncherView {
    fn render_search_bar(&self, divided: bool, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(12.0))
            .h(px(SEARCH_BAR_HEIGHT))
            .px(px(18.0))
            // The rule only appears once there is something below it to separate.
            .when(divided, |this| {
                this.border_b_1().border_color(cx.theme().border)
            })
            .child(
                Icon::new(IconName::Search)
                    .size_5()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(18.0))
                    .line_height(px(26.0))
                    .child(self.query_input.clone()),
            )
    }

    fn render_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let container = div().flex_1().min_h(px(0.0)).flex().flex_col();

        // Nothing typed: stay empty. The launcher opens silent, which is the
        // whole point of the "super minimal" home screen.
        if self.query.trim().is_empty() {
            return container;
        }

        if self.items.is_empty() {
            let message = if self.index.is_ready() {
                format!("No results for “{}”", self.query.trim())
            } else {
                "Indexing your files…".to_string()
            };
            return container.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(cx.theme().muted_foreground)
                    .child(message),
            );
        }

        container
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(LIST_INSET + 10.0))
                    .pt(px(10.0))
                    .pb(px(4.0))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("Files"),
            )
            .child(self.render_list(cx))
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.items.clone();
        let selected = self.selected;
        let view = cx.entity().downgrade();

        uniform_list("launcher-results", items.len(), {
            move |range: Range<usize>, _window: &mut Window, cx: &mut App| {
                let mut rows = Vec::new();
                for position in range {
                    if let Some(item) = items.get(position) {
                        rows.push(render_row(item, position, position == selected, &view, cx));
                    }
                }
                rows
            }
        })
        .flex_1()
        .px(px(LIST_INSET))
        .pb(px(LIST_INSET))
        .track_scroll(self.scroll_handle.clone())
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let status: SharedString = if self.index.is_ready() {
            "nohrs".into()
        } else {
            "Indexing…".into()
        };

        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .h(px(FOOTER_HEIGHT))
            .px(px(14.0))
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary.opacity(0.4))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().size(px(14.0)).rounded(px(4.0)).bg(cx.theme().primary))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    ),
            )
            .when(self.selected_item().is_some(), |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.0))
                        .child(action_hint("Open", "↵".to_string(), cx))
                        .child(div().w(px(1.0)).h(px(14.0)).bg(cx.theme().border))
                        .child(action_hint("Reveal", format!("{SECONDARY_MODIFIER}↵"), cx)),
                )
            })
    }
}

fn render_row(
    item: &LauncherItem,
    position: usize,
    selected: bool,
    view: &WeakEntity<LauncherView>,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;

    div()
        .id(("launcher-item", position))
        .flex()
        .w_full()
        .items_center()
        .gap(px(10.0))
        .h(px(ROW_HEIGHT))
        .px(px(10.0))
        .rounded(px(8.0))
        .cursor_pointer()
        // Derived from the foreground rather than taken from a palette entry so
        // the two states stay a visible step apart in both light and dark, where
        // a fixed grey would wash out in one of them.
        .when(selected, |this| this.bg(foreground.opacity(SELECTED_TINT)))
        .when(!selected, |this| {
            this.hover(|style| style.bg(foreground.opacity(HOVER_TINT)))
        })
        .on_click({
            let view = view.clone();
            move |_event, window, cx| {
                view.update(cx, |this, cx| {
                    this.set_selected(position, cx);
                    this.open_selected(window, cx);
                })
                .log_err();
            }
        })
        .child(
            Icon::new(if item.kind == crate::ranking::ItemKind::Folder {
                IconName::Folder
            } else {
                IconName::File
            })
            .size_4()
            .text_color(if selected {
                cx.theme().foreground
            } else {
                muted
            }),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .items_baseline()
                .gap(px(8.0))
                .min_w(px(0.0))
                .overflow_hidden()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(13.5))
                        .child(render_title(item, foreground, muted)),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(11.5))
                        .text_color(muted)
                        .child(item.subtitle.clone()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .pl(px(8.0))
                .text_size(px(11.0))
                .text_color(muted)
                .child(item.kind.badge()),
        )
}

/// Draws the name with the matched characters picked out.
///
/// The matched run keeps the full foreground and gains weight while everything
/// else drops to the muted tone, so the reason a row matched is legible in both
/// light and dark without depending on a palette entry that a custom theme may
/// not define. A row matched on its path alone has nothing to pick out, so its
/// name is drawn plainly rather than uniformly dimmed.
fn render_title(item: &LauncherItem, foreground: Hsla, muted: Hsla) -> impl IntoElement + use<> {
    if item.title_matches.is_empty() {
        return div().text_color(foreground).child(item.title.clone());
    }

    let highlights = highlight_ranges(&item.title, &item.title_matches)
        .into_iter()
        .map(|range| {
            (
                range,
                HighlightStyle {
                    color: Some(foreground),
                    font_weight: Some(FontWeight::BOLD),
                    ..Default::default()
                },
            )
        });

    div()
        .text_color(muted)
        .child(StyledText::new(item.title.clone()).with_highlights(highlights))
}

fn action_hint(label: &str, key: String, cx: &App) -> impl IntoElement + use<> {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .min_w(px(20.0))
                .px(px(5.0))
                .py(px(1.0))
                .rounded(px(4.0))
                .bg(cx.theme().muted)
                .text_size(px(11.0))
                .text_color(cx.theme().muted_foreground)
                .child(key),
        )
}

/// Converts matched character positions into the byte ranges `StyledText` wants,
/// merging runs of adjacent characters so each highlighted stretch is one range.
///
/// Character positions are what the matcher reports; byte offsets are what the
/// text layout needs, and for anything non-ASCII the two differ.
fn highlight_ranges(text: &str, matches: &[u32]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (position, (offset, character)) in text.char_indices().enumerate() {
        let position: u32 = match position.try_into() {
            Ok(position) => position,
            // A title longer than 4 billion characters cannot be highlighted,
            // but it can still be displayed.
            Err(_) => break,
        };
        if matches.binary_search(&position).is_err() {
            continue;
        }
        let end = offset + character.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == offset => last.end = end,
            _ => ranges.push(offset..end),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gpui::{TestAppContext, WindowHandle};
    use nohrs_services::search::file_index::IndexedEntry;

    use super::*;

    /// Home the fixture paths hang from, so subtitles abbreviate to `~`.
    const TEST_HOME: &str = "/home/u";

    /// A launcher in a test window. The view is the window's root, exactly as
    /// in the real launcher — nothing sits between it and the window.
    struct Launcher {
        window: WindowHandle<LauncherView>,
    }

    impl Launcher {
        /// Build a launcher over a pre-filled index. The index is populated
        /// directly rather than scanned, so the tests need no filesystem, and
        /// the view is built inside `add_window` because its search field is
        /// window-bound (the pattern in docs/testing.md).
        fn new(cx: &mut TestAppContext, paths: &[&str]) -> Self {
            // The launcher reads its colours from gpui-component's `Theme`
            // global, which this installs; it does not need anything else of it.
            cx.update(gpui_component::init);
            cx.update(crate::field::init);
            let index = Arc::new(FileNameIndex::new());
            index.replace(
                paths
                    .iter()
                    .filter_map(|path| IndexedEntry::new(PathBuf::from(path), false))
                    .collect(),
            );

            let window = cx.add_window(|window, cx| {
                LauncherView::new(index, Some(PathBuf::from(TEST_HOME)), window, cx)
            });
            Self { window }
        }

        /// Put text in the search field without waiting for the search: the
        /// change reaches the view, but its debounce timer is still pending.
        fn enter_text(&self, cx: &mut TestAppContext, query: &str) {
            let query = query.to_string();
            self.window
                .update(cx, move |view, _window, cx| {
                    view.query_input
                        .update(cx, |field, cx| field.set_text(query, cx));
                })
                .expect("the launcher window should be open");
        }

        /// Type into the search field and let the search land: the field emits a
        /// change, the debounce timer has to expire, and the ranking runs on the
        /// background executor before results reach the view.
        async fn type_query(&self, cx: &mut TestAppContext, query: &str) {
            self.enter_text(cx, query);
            cx.background_executor.timer(DEBOUNCE * 2).await;
            cx.run_until_parked();
        }

        fn titles(&self, cx: &mut TestAppContext) -> Vec<String> {
            self.window
                .read_with(cx, |view, _cx| {
                    view.items()
                        .iter()
                        .map(|item| item.title.to_string())
                        .collect()
                })
                .expect("the launcher window should be open")
        }

        fn selected(&self, cx: &mut TestAppContext) -> Option<String> {
            self.window
                .read_with(cx, |view, _cx| {
                    view.selected_item().map(|item| item.title.to_string())
                })
                .expect("the launcher window should be open")
        }

        fn selected_index(&self, cx: &mut TestAppContext) -> Option<usize> {
            self.window
                .read_with(cx, |view, _cx| view.selected_index())
                .expect("the launcher window should be open")
        }

        /// Run a navigation method against the view.
        fn navigate(
            &self,
            cx: &mut TestAppContext,
            act: impl FnOnce(&mut LauncherView, &mut Context<LauncherView>),
        ) {
            self.window
                .update(cx, |view, _window, cx| act(view, cx))
                .expect("the launcher window should be open");
        }
    }

    #[gpui::test]
    async fn typing_a_query_fills_the_result_list(cx: &mut TestAppContext) {
        let launcher = Launcher::new(
            cx,
            &[
                "/home/u/Cargo.toml",
                "/home/u/README.md",
                "/home/u/src/main.rs",
            ],
        );

        // Nothing is shown before anything is typed (docs/launcher.md §3).
        assert!(launcher.titles(cx).is_empty());
        assert_eq!(launcher.selected(cx), None);

        launcher.type_query(cx, "cargo").await;

        assert_eq!(launcher.titles(cx), vec!["Cargo.toml"]);
        assert_eq!(launcher.selected(cx), Some("Cargo.toml".to_string()));
    }

    #[gpui::test]
    async fn clearing_the_query_empties_the_list(cx: &mut TestAppContext) {
        let launcher = Launcher::new(cx, &["/home/u/Cargo.toml"]);
        launcher.type_query(cx, "cargo").await;
        assert_eq!(launcher.titles(cx).len(), 1);

        launcher.type_query(cx, "").await;

        assert!(launcher.titles(cx).is_empty());
        assert_eq!(launcher.selected(cx), None);
    }

    #[gpui::test]
    async fn a_newer_query_supersedes_an_in_flight_one(cx: &mut TestAppContext) {
        let launcher = Launcher::new(cx, &["/home/u/alpha.txt", "/home/u/beta.txt"]);

        // Let the first query register a search, but not run: the test clock only
        // moves when a timer is awaited, so its debounce is still pending here.
        launcher.enter_text(cx, "alpha");
        cx.run_until_parked();
        assert!(launcher.titles(cx).is_empty());

        // Retyping drops that pending search, so only the second query's results
        // are ever shown — a slower search can never land on top of a newer one.
        launcher.type_query(cx, "beta").await;

        assert_eq!(launcher.titles(cx), vec!["beta.txt"]);
    }

    #[gpui::test]
    async fn arrow_navigation_wraps_at_both_ends(cx: &mut TestAppContext) {
        let launcher = Launcher::new(
            cx,
            &[
                "/home/u/note-a.md",
                "/home/u/note-b.md",
                "/home/u/note-c.md",
            ],
        );
        launcher.type_query(cx, "note").await;
        let ordered = launcher.titles(cx);
        assert_eq!(ordered.len(), 3);

        launcher.navigate(cx, |view, cx| {
            view.select_next(cx);
            view.select_next(cx);
        });
        assert_eq!(launcher.selected(cx), Some(ordered[2].clone()));

        // Past the last row it wraps to the first, and back off the front to the
        // last: a launcher list is a ring, not a bounded scroll.
        launcher.navigate(cx, |view, cx| view.select_next(cx));
        assert_eq!(launcher.selected(cx), Some(ordered[0].clone()));

        launcher.navigate(cx, |view, cx| view.select_previous(cx));
        assert_eq!(launcher.selected(cx), Some(ordered[2].clone()));
    }

    #[gpui::test]
    async fn page_jumps_clamp_instead_of_wrapping(cx: &mut TestAppContext) {
        let paths: Vec<String> = (0..20)
            .map(|number| format!("/home/u/note-{number:02}.md"))
            .collect();
        let borrowed: Vec<&str> = paths.iter().map(String::as_str).collect();
        let launcher = Launcher::new(cx, &borrowed);
        launcher.type_query(cx, "note").await;
        let ordered = launcher.titles(cx);

        launcher.navigate(cx, |view, cx| view.page_up(cx));
        assert_eq!(launcher.selected(cx), ordered.first().cloned());

        launcher.navigate(cx, |view, cx| {
            view.page_down(cx);
            view.page_down(cx);
            view.page_down(cx);
        });
        assert_eq!(launcher.selected(cx), ordered.last().cloned());
    }

    #[gpui::test]
    async fn navigating_an_empty_list_is_a_no_op(cx: &mut TestAppContext) {
        let launcher = Launcher::new(cx, &["/home/u/Cargo.toml"]);
        launcher.type_query(cx, "nothing-matches-this").await;

        launcher.navigate(cx, |view, cx| {
            view.select_next(cx);
            view.select_previous(cx);
            view.page_down(cx);
        });

        assert_eq!(launcher.selected(cx), None);
        assert_eq!(launcher.selected_index(cx), None);
    }

    #[test]
    fn adjacent_matches_merge_into_one_range() {
        assert_eq!(highlight_ranges("main.rs", &[0, 1, 2]), vec![0..3]);
        assert_eq!(highlight_ranges("main.rs", &[0, 5]), vec![0..1, 5..6]);
        assert_eq!(highlight_ranges("main.rs", &[]), Vec::<Range<usize>>::new());
    }

    #[test]
    fn ranges_are_byte_offsets_and_land_on_char_boundaries() {
        // "日本語.txt": each kanji is three bytes, so character 1 is bytes 3..6.
        let text = "日本語.txt";
        assert_eq!(highlight_ranges(text, &[1]), vec![3..6]);
        assert_eq!(highlight_ranges(text, &[0, 1, 2]), vec![0..9]);
        for range in highlight_ranges(text, &[0, 2, 4]) {
            assert!(text.is_char_boundary(range.start));
            assert!(text.is_char_boundary(range.end));
        }
    }

    #[test]
    fn positions_past_the_end_are_ignored() {
        // A stale match index must not panic or produce an out-of-bounds range.
        assert_eq!(highlight_ranges("ab", &[0, 99]), vec![0..1]);
    }
}
