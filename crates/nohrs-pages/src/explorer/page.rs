//! The split-view container that owns the explorer's panes.
//!
//! A single [`ExplorerPage`] holds one or two [`ExplorerPane`]s (2-way split is
//! the P2 cap; `docs/explorer-essentials.md` §3.1). Each pane navigates
//! independently and renders its own header / listing / preview. The container
//! arranges the panes (left/right or top/bottom), routes the split / focus /
//! close shortcuts (§3.2, §6), and mirrors navigation across panes when
//! `synced_panes` is enabled.

use std::sync::{Arc, Once};

use gpui::*;
use gpui_component::resizable::ResizableState;
use nohrs_core::config::{Explorer as ExplorerConfig, SplitDirection, Ui};
use nohrs_services::search::SearchService;
use nohrs_ui::theme::theme;

use super::state::ExplorerPane;
use super::types::PaneEvent;
use crate::pane_group::PaneGroup;

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
///
/// The split/tab/resize machinery lives in the content-agnostic [`PaneGroup`];
/// `ExplorerPage` adds the explorer-specific layer on top: replaying `[ui]`
/// config onto new panes and mirroring navigation across panes when
/// `synced_panes` is enabled (§3.2). It keeps a per-pane subscription Vec
/// index-aligned with `group.panes()` by funneling every create/remove through
/// `add_explorer_pane` / `close_pane`.
pub struct ExplorerPage {
    group: PaneGroup<ExplorerPane>,
    // Subscriptions for pane navigation events, index-aligned with `group.panes()`.
    pane_subscriptions: Vec<Subscription>,
    /// Whether navigation in one pane mirrors into the others (§3.2).
    synced_panes: bool,
    // Last-applied `[ui]` settings, replayed onto panes opened by a later split
    // so they match config rather than reverting to pane defaults.
    ui: Ui,
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
        let build_search_service = search_service.clone();
        let (group, first_pane) = PaneGroup::new(
            Box::new(move |window, cx| {
                let search_service = build_search_service.clone();
                cx.new(|cx| ExplorerPane::build(search_service, window, cx))
            }),
            pane_resizable,
            window,
            cx,
        );
        let mut page = Self {
            group,
            pane_subscriptions: Vec::new(),
            synced_panes: false,
            ui: Ui::default(),
            focus_handle: cx.focus_handle(),
        };
        let subscription = cx.subscribe(&first_pane, Self::on_pane_event);
        page.pane_subscriptions.push(subscription);
        // The root pane shows its sidebar; panes opened by a later split default
        // to hidden (issue #164, §2).
        first_pane.update(cx, |pane, _cx| pane.sidebar_visible = true);
        page
    }

    // Creates a pane (optionally rooted at `cwd`) through the group, subscribes to
    // its navigation events keeping `pane_subscriptions` index-aligned, and
    // replays the active `[ui]` config. Returns the new pane's index.
    fn add_explorer_pane(
        &mut self,
        cwd: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let (index, pane) = self.group.add_pane(window, cx)?;
        let subscription = cx.subscribe(&pane, Self::on_pane_event);
        self.pane_subscriptions.push(subscription);
        // Replay the active `[ui]` config so a pane opened by a split inherits the
        // user's sort/hidden/icon settings instead of reverting to pane defaults.
        let ui = self.ui.clone();
        pane.update(cx, |pane, cx| {
            pane.apply_config_ui(&ui, cx);
            // Split-created panes start with the sidebar collapsed (issue #164).
            pane.sidebar_visible = false;
            if let Some(cwd) = cwd {
                pane.cwd = cwd;
                // Force a reload of the new root on the next render.
                pane.loaded = false;
            }
        });
        Some(index)
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
            .group
            .panes()
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
        self.group.set_direction(direction);
        if self.group.pane_count() < 2 {
            let cwd = self.group.active_pane().read(cx).cwd.clone();
            if let Some(index) = self.add_explorer_pane(Some(cwd), window, cx) {
                self.group.set_active(index, window, cx);
            }
        } else {
            self.group.focus_active(window, cx);
        }
        cx.notify();
    }

    /// Closes a pane, keeping at least one open (§3.1). No-op for the last pane.
    pub fn close_pane(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.remove_pane(index, window, cx) {
            // Dropping the subscription deregisters the removed pane's event
            // handler; removing at the same index keeps the Vecs aligned.
            drop(self.pane_subscriptions.remove(index));
            cx.notify();
        }
    }

    /// Makes `index` the active pane and moves keyboard focus to it.
    pub fn set_active(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.set_active(index, window, cx) {
            cx.notify();
        }
    }

    fn focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.focus_next(window, cx) {
            cx.notify();
        }
    }

    fn focus_prev(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.focus_prev(window, cx) {
            cx.notify();
        }
    }

    /// Applies the `[ui]` config section to every pane (§5 of `config.md`).
    pub fn apply_config_ui(&mut self, ui: &Ui, cx: &mut Context<Self>) {
        self.ui = ui.clone();
        for pane in self.group.panes().to_vec() {
            pane.update(cx, |pane, cx| pane.apply_config_ui(ui, cx));
        }
    }

    /// Applies the `[explorer]` config section: the default split orientation and
    /// the synced-panes opt-in. Enabling sync immediately mirrors the active
    /// pane's directory into the others.
    pub fn apply_config_explorer(&mut self, explorer: &ExplorerConfig, cx: &mut Context<Self>) {
        // Only adopt the configured orientation while unsplit, so a config reload
        // does not silently flip a split the user arranged via the shortcuts.
        if self.group.pane_count() < 2 {
            self.group.set_direction(explorer.split_direction);
        }
        let enabling = explorer.synced_panes && !self.synced_panes;
        self.synced_panes = explorer.synced_panes;
        if enabling {
            let path = self.group.active_pane().read(cx).cwd.clone();
            let active_id = self.group.active_pane().entity_id();
            for pane in self.group.panes().to_vec() {
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
        self.group.active_pane().read(cx).status_for_footer()
    }
}

impl Render for ExplorerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.group.render(
            cx,
            |this, index, window, cx| this.set_active(index, window, cx),
            |this, index, window, cx| this.close_pane(index, window, cx),
        );

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
        self.group.pane_count()
    }

    pub(crate) fn active_index(&self) -> usize {
        self.group.active()
    }

    pub(crate) fn direction(&self) -> SplitDirection {
        self.group.direction()
    }

    pub(crate) fn is_synced(&self) -> bool {
        self.synced_panes
    }

    pub(crate) fn pane(&self, index: usize) -> Entity<ExplorerPane> {
        self.group
            .pane(index)
            .expect("test requested an out-of-range pane")
    }

    pub(crate) fn pane_cwd(&self, index: usize, cx: &App) -> Option<String> {
        self.group
            .panes()
            .get(index)
            .map(|pane| pane.read(cx).cwd.clone())
    }

    /// Number of live per-pane subscriptions; used to assert the subscription Vec
    /// stays aligned with the pane Vec across splits/closes.
    pub(crate) fn subscription_count(&self) -> usize {
        self.pane_subscriptions.len()
    }
}
