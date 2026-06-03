//! The split-view container that owns the explorer's panes.
//!
//! A single [`ExplorerPage`] holds one or two [`ExplorerPane`]s (2-way split is
//! the P2 cap; `docs/explorer-essentials.md` §3.1). Each pane navigates
//! independently and renders its own header / listing / preview. The container
//! arranges the panes (left/right or top/bottom), routes the split / focus /
//! close shortcuts (§3.2, §6), and mirrors navigation across panes when
//! `synced_panes` is enabled.

use std::sync::{Arc, Once};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable, ResizableState};
use gpui_component::{Icon, IconName};
use nohrs_core::config::{Explorer as ExplorerConfig, SplitDirection, Ui};
use nohrs_services::search::SearchService;
use nohrs_ui::theme::theme;

use super::state::ExplorerPane;
use super::types::PaneEvent;

// Key context the pane shortcuts are bound under, so they only fire while the
// explorer (and not another page) is focused.
const PANES_CONTEXT: &str = "ExplorerPanes";

actions!(
    explorer_panes,
    [
        /// Split the explorer into left/right panes (or flip an existing split).
        SplitVertical,
        /// Split the explorer into top/bottom panes (or flip an existing split).
        SplitHorizontal,
        /// Move focus to the first pane.
        FocusPane1,
        /// Move focus to the second pane.
        FocusPane2,
        /// Move focus to the next pane, wrapping around.
        FocusNextPane,
        /// Move focus to the previous pane, wrapping around.
        FocusPrevPane,
    ]
);

static BIND_PANE_KEYS: Once = Once::new();

fn bind_pane_keys(cx: &mut App) {
    BIND_PANE_KEYS.call_once(|| {
        cx.bind_keys(vec![
            KeyBinding::new("cmd-\\", SplitVertical, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-\\", SplitVertical, Some(PANES_CONTEXT)),
            // `Shift+\` resolves to the `|` keysym; gpui's Linux layer then drops
            // the shift modifier for symbols, so the shortcut arrives as `cmd-|`
            // / `ctrl-|` rather than `*-shift-\`. Bind every spelling so the
            // documented `Cmd/Ctrl+Shift+\` works across platforms and layouts.
            KeyBinding::new("cmd-shift-\\", SplitHorizontal, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-shift-\\", SplitHorizontal, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-|", SplitHorizontal, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-|", SplitHorizontal, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-shift-|", SplitHorizontal, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-shift-|", SplitHorizontal, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-1", FocusPane1, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-1", FocusPane1, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-2", FocusPane2, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-2", FocusPane2, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-]", FocusNextPane, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-]", FocusNextPane, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-[", FocusPrevPane, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-[", FocusPrevPane, Some(PANES_CONTEXT)),
        ]);
    });
}

/// The explorer page: a 2-way split container over independently-navigating
/// panes. With a single pane it renders exactly like an unsplit explorer.
pub struct ExplorerPage {
    // Invariant: always non-empty and at most two entries (2-way cap, §3.1).
    panes: Vec<Entity<ExplorerPane>>,
    // Subscriptions for pane navigation events, index-aligned with `panes`.
    pane_subscriptions: Vec<Subscription>,
    /// Index of the pane keyboard input and shortcuts act on.
    active: usize,
    /// Orientation a split uses; seeded from config, toggled by the shortcuts.
    direction: SplitDirection,
    /// Whether navigation in one pane mirrors into the others (§3.2).
    synced_panes: bool,
    // Last-applied `[ui]` settings, replayed onto panes opened by a later split
    // so they match config rather than reverting to pane defaults.
    ui: Ui,
    // Resizable state for the divider between the two panes.
    pane_resizable: Entity<ResizableState>,
    search_service: Option<Arc<SearchService>>,
    focus_handle: FocusHandle,
}

impl Focusable for ExplorerPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ExplorerPage {
    /// Builds the explorer with a single pane. The pane resizable is supplied by
    /// the application (it owns the app-level state); each pane creates its own
    /// listing/preview resizable and search input.
    pub fn new(
        pane_resizable: Entity<ResizableState>,
        search_service: Option<Arc<SearchService>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        bind_pane_keys(cx);
        let mut page = Self {
            panes: Vec::new(),
            pane_subscriptions: Vec::new(),
            active: 0,
            direction: SplitDirection::default(),
            synced_panes: false,
            ui: Ui::default(),
            pane_resizable,
            search_service,
            focus_handle: cx.focus_handle(),
        };
        page.add_pane(None, window, cx);
        page
    }

    // Creates a pane (optionally rooted at `cwd`), subscribes to its navigation
    // events, and appends it. Returns the new pane's index.
    fn add_pane(
        &mut self,
        cwd: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let search_service = self.search_service.clone();
        let pane = cx.new(|cx| ExplorerPane::build(search_service, window, cx));
        // Replay the active `[ui]` config so a pane opened by a split inherits the
        // user's sort/hidden/icon settings instead of reverting to pane defaults.
        let ui = self.ui.clone();
        pane.update(cx, |pane, cx| {
            pane.apply_config_ui(&ui, cx);
            if let Some(cwd) = cwd {
                pane.cwd = cwd;
                // Force a reload of the new root on the next render.
                pane.loaded = false;
            }
        });
        let subscription = cx.subscribe(&pane, Self::on_pane_event);
        self.panes.push(pane);
        self.pane_subscriptions.push(subscription);
        self.panes.len() - 1
    }

    // Mirrors a pane's navigation into its siblings while syncing is enabled.
    fn on_pane_event(
        &mut self,
        source: Entity<ExplorerPane>,
        event: &PaneEvent,
        cx: &mut Context<Self>,
    ) {
        if !self.synced_panes {
            return;
        }
        let PaneEvent::Navigated(path) = event;
        let targets: Vec<Entity<ExplorerPane>> = self
            .panes
            .iter()
            .filter(|pane| pane.entity_id() != source.entity_id())
            .cloned()
            .collect();
        for pane in targets {
            let path = path.clone();
            // `navigate_to_synced` (not `change_dir`) is deliberate: it does not
            // re-emit `PaneEvent::Navigated`, which would mirror back to the
            // source pane and loop indefinitely. Don't replace it with a regular
            // navigation method.
            pane.update(cx, |pane, cx| pane.navigate_to_synced(path, cx));
        }
    }

    /// Ensures a split exists and uses `direction`. With one pane it opens a
    /// second rooted at the active pane's directory; with two it just re-orients
    /// (so the other split shortcut flips horizontal/vertical). Always keeps the
    /// 2-way cap (§3.1).
    pub fn split(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.direction = direction;
        if self.panes.len() < 2 {
            let cwd = self.active_pane().read(cx).cwd.clone();
            let index = self.add_pane(Some(cwd), window, cx);
            self.active = index;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Closes a pane, keeping at least one open (§3.1). No-op for the last pane.
    pub fn close_pane(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() <= 1 || index >= self.panes.len() {
            return;
        }
        self.panes.remove(index);
        // Dropping the subscription deregisters the removed pane's event handler.
        drop(self.pane_subscriptions.remove(index));
        if self.active >= self.panes.len() {
            self.active = self.panes.len() - 1;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Makes `index` the active pane and moves keyboard focus to it.
    pub fn set_active(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.panes.len() || index == self.active {
            return;
        }
        self.active = index;
        self.focus_active(window, cx);
        cx.notify();
    }

    fn focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() < 2 {
            return;
        }
        self.set_active((self.active + 1) % self.panes.len(), window, cx);
    }

    fn focus_prev(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() < 2 {
            return;
        }
        let count = self.panes.len();
        self.set_active((self.active + count - 1) % count, window, cx);
    }

    fn active_pane(&self) -> &Entity<ExplorerPane> {
        // `active` is kept in bounds by every mutator; fall back to the first
        // pane rather than panicking should that invariant ever be violated.
        self.panes.get(self.active).unwrap_or(&self.panes[0])
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.active_pane().read(cx).focus_handle.clone();
        handle.focus(window);
    }

    /// Applies the `[ui]` config section to every pane (§5 of `config.md`).
    pub fn apply_config_ui(&mut self, ui: &Ui, cx: &mut Context<Self>) {
        self.ui = ui.clone();
        for pane in self.panes.clone() {
            pane.update(cx, |pane, cx| pane.apply_config_ui(ui, cx));
        }
    }

    /// Applies the `[explorer]` config section: the default split orientation and
    /// the synced-panes opt-in. Enabling sync immediately mirrors the active
    /// pane's directory into the others.
    pub fn apply_config_explorer(&mut self, explorer: &ExplorerConfig, cx: &mut Context<Self>) {
        // Only adopt the configured orientation while unsplit, so a config reload
        // does not silently flip a split the user arranged via the shortcuts.
        if self.panes.len() < 2 {
            self.direction = explorer.split_direction;
        }
        let enabling = explorer.synced_panes && !self.synced_panes;
        self.synced_panes = explorer.synced_panes;
        if enabling {
            let path = self.active_pane().read(cx).cwd.clone();
            let active_id = self.active_pane().entity_id();
            for pane in self.panes.clone() {
                if pane.entity_id() != active_id {
                    let path = path.clone();
                    pane.update(cx, |pane, cx| pane.navigate_to_synced(path, cx));
                }
            }
        }
        cx.notify();
    }

    /// Footer status for the active pane (a config error in `RootView` still
    /// takes precedence over this).
    pub fn status_for_footer(&self, cx: &App) -> Option<(String, bool)> {
        self.active_pane().read(cx).status_for_footer()
    }

    fn render_pane(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let pane = self.panes[index].clone();
        let split = self.panes.len() > 1;
        let is_active = split && index == self.active;
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .when(is_active, |this| {
                this.border_t_2().border_color(rgb(theme::ACCENT))
            })
            .when(split && !is_active, |this| {
                this.border_t_2().border_color(rgb(theme::BG))
            })
            // Clicking anywhere in a pane makes it the active one.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| this.set_active(index, window, cx)),
            )
            .child(self.render_tab_bar(index, split, cx))
            .child(div().flex_1().min_h(px(0.0)).overflow_hidden().child(pane))
            .into_any_element()
    }

    // The pane-local tab bar: foundation for per-pane tabs (§3.3, full tabs in
    // #62). For now it shows the current directory as a single tab plus the
    // pane-close button, which only appears once a split exists.
    fn render_tab_bar(
        &self,
        index: usize,
        split: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cwd = self.panes[index].read(cx).cwd.clone();
        let label = std::path::Path::new(&cwd)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or(cwd);
        let is_active = split && index == self.active;

        div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(32.0))
            .w_full()
            .px(px(6.0))
            .gap(px(4.0))
            .bg(rgb(theme::TOOLBAR_BG))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(24.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .text_sm()
                    .when(is_active, |this| {
                        this.bg(rgb(theme::TOOLBAR_ACTIVE_BG))
                            .text_color(rgb(theme::TOOLBAR_ACTIVE_TEXT))
                    })
                    .when(!is_active, |this| this.text_color(rgb(theme::TOOLBAR_TEXT)))
                    .child(label),
            )
            // Placeholder for the new-tab affordance fleshed out in #62.
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.0))
                    .rounded(px(4.0))
                    .child(
                        Icon::new(IconName::Plus)
                            .size_4()
                            .text_color(rgb(theme::MUTED)),
                    ),
            )
            .child(div().flex_grow())
            .when(split, |this| {
                this.child(
                    div()
                        .id(("close-pane", index))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(22.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme::TOOLBAR_HOVER)))
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.close_pane(index, window, cx);
                        }))
                        .child(
                            Icon::new(IconName::Close)
                                .size_4()
                                .text_color(rgb(theme::TOOLBAR_TEXT)),
                        ),
                )
            })
    }
}

impl Render for ExplorerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.panes.len() > 1 {
            let first = self.render_pane(0, cx);
            let second = self.render_pane(1, cx);
            let group = match self.direction {
                SplitDirection::Vertical => {
                    h_resizable("explorer-panes", self.pane_resizable.clone())
                }
                SplitDirection::Horizontal => {
                    v_resizable("explorer-panes", self.pane_resizable.clone())
                }
            };
            group
                .child(resizable_panel().child(first))
                .child(resizable_panel().child(second))
                .into_any_element()
        } else {
            self.render_pane(0, cx)
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .key_context(PANES_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SplitVertical, window, cx| {
                this.split(SplitDirection::Vertical, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitHorizontal, window, cx| {
                this.split(SplitDirection::Horizontal, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPane1, window, cx| {
                this.set_active(0, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPane2, window, cx| {
                this.set_active(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusNextPane, window, cx| {
                this.focus_next(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPrevPane, window, cx| {
                this.focus_prev(window, cx);
            }))
            .child(body)
    }
}

impl crate::Page for ExplorerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}

#[cfg(test)]
impl ExplorerPage {
    pub(crate) fn pane_count(&self) -> usize {
        self.panes.len()
    }

    pub(crate) fn active_index(&self) -> usize {
        self.active
    }

    pub(crate) fn direction(&self) -> SplitDirection {
        self.direction
    }

    pub(crate) fn is_synced(&self) -> bool {
        self.synced_panes
    }

    pub(crate) fn pane(&self, index: usize) -> Entity<ExplorerPane> {
        self.panes[index].clone()
    }

    pub(crate) fn pane_cwd(&self, index: usize, cx: &App) -> Option<String> {
        self.panes.get(index).map(|pane| pane.read(cx).cwd.clone())
    }
}
