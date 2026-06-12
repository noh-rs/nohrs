//! The split-view container that owns the explorer's panes and their tabs.
//!
//! A single [`ExplorerPage`] holds one or two panes (2-way split is the P2 cap;
//! `docs/explorer-essentials.md` §3.1), each with its own list of tabs (§4). Each
//! tab navigates independently and renders its own header / listing / preview.
//! The container arranges the panes (left/right or top/bottom), routes the split
//! / focus / tab shortcuts (§3.2, §4, §6), and mirrors navigation across panes
//! when `synced_panes` is enabled.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Once};
use std::time::Duration;

use gpui::*;
use gpui_component::resizable::ResizableState;
use nohrs_core::config::{Explorer as ExplorerConfig, SplitDirection, Ui};
use nohrs_core::telemetry::LogErr;
use nohrs_services::search::SearchService;
use nohrs_store::KvStore;
use nohrs_ui::theme::theme;
use serde::{Deserialize, Serialize};

use super::state::ExplorerPane;
use super::types::PaneEvent;
use crate::pane_group::{PaneGroup, PaneGroupCallbacks};

// Key context the pane shortcuts are bound under, so they only fire while the
// explorer (and not another page) is focused.
const PANES_CONTEXT: &str = "ExplorerPanes";

// Host-KV key under which the tab session is persisted (`docs/persistence.md`
// §3: tab/session restore lives in redb `state.redb`).
const SESSION_KEY: &str = "session.explorer_tabs";

// How long to coalesce rapid mutations (navigation, tab open/close) before
// writing the session snapshot, so high-frequency changes don't hammer redb's
// fsync-on-commit (`docs/persistence.md` §3).
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// A persisted snapshot of one pane: its tabs (each a directory) and which is
/// active.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PaneSnapshot {
    tabs: Vec<String>,
    active_tab: usize,
}

/// A persisted snapshot of the whole split for session restore (§4).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionSnapshot {
    panes: Vec<PaneSnapshot>,
    active_pane: usize,
    direction: SplitDirection,
}

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
        /// Open a new tab in the active pane, rooted at home.
        NewTab,
        /// Close the active tab (closing the pane if it was its last tab).
        CloseTab,
        /// Toggle the active tab's left quick-access sidebar.
        ToggleSidebar,
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
            KeyBinding::new("cmd-t", NewTab, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-t", NewTab, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-w", CloseTab, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-w", CloseTab, Some(PANES_CONTEXT)),
            KeyBinding::new("cmd-b", ToggleSidebar, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-b", ToggleSidebar, Some(PANES_CONTEXT)),
        ]);
    });
}

/// Home directory used as the root of a freshly opened tab (§4). Mirrors the
/// sidebar's `HOME` lookup; falls back to the current directory.
fn home_dir() -> String {
    std::env::var("HOME")
        .ok()
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| ".".into())
}

/// Clamp `index` into `0..len` (or `0` when `len` is zero), for restoring a
/// possibly-stale active index from a snapshot.
fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

/// The explorer page: a 2-way split container over panes, each holding its own
/// tabs. With a single pane and a single tab it renders exactly like an unsplit
/// explorer.
///
/// The split/tab/resize machinery lives in the content-agnostic [`PaneGroup`];
/// `ExplorerPage` adds the explorer-specific layer on top: replaying `[ui]`
/// config onto new tabs and mirroring navigation across panes when
/// `synced_panes` is enabled (§3.2). It keeps a per-tab subscription map keyed by
/// the tab's `EntityId` by funneling every tab create/remove through the helpers
/// here, so the map stays consistent across the 2-D pane/tab structure.
pub struct ExplorerPage {
    group: PaneGroup<ExplorerPane>,
    // Navigation-event subscriptions, one per live tab, keyed by tab entity id.
    tab_subscriptions: HashMap<EntityId, Subscription>,
    /// Whether navigation in one pane mirrors into the others (§3.2).
    synced_panes: bool,
    // Last-applied `[ui]` settings, replayed onto tabs opened later so they match
    // config rather than reverting to pane defaults.
    ui: Ui,
    // Host KV store for session persistence (§4). `None` disables save/restore
    // (e.g. when the store failed to open, or in tests).
    store: Option<Arc<dyn KvStore>>,
    // The pending debounced session save, dropped (cancelling it) whenever a new
    // mutation reschedules.
    save_task: Option<Task<()>>,
    focus_handle: FocusHandle,
}

impl Focusable for ExplorerPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ExplorerPage {
    /// Builds the explorer with a single pane holding a single tab. The pane
    /// resizable is supplied by the application (it owns the app-level state);
    /// each tab creates its own listing/preview resizable and search input.
    pub fn new(
        pane_resizable: Entity<ResizableState>,
        search_service: Option<Arc<SearchService>>,
        store: Option<Arc<dyn KvStore>>,
        restore_tabs: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        bind_pane_keys(cx);
        let build_search_service = search_service.clone();
        let (group, first_tab) = PaneGroup::new(
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
            tab_subscriptions: HashMap::new(),
            synced_panes: false,
            ui: Ui::default(),
            store,
            save_task: None,
            focus_handle: cx.focus_handle(),
        };
        page.subscribe_tab(&first_tab, cx);
        // The root pane's tab shows its sidebar; tabs opened later default to
        // hidden (issue #164, §2).
        first_tab.update(cx, |pane, _cx| pane.sidebar_visible = true);

        // Restore the previous session's tabs (§4), unless disabled by config.
        // A one-time synchronous read at startup is acceptable; ongoing writes go
        // through a background task (see `schedule_save`).
        if restore_tabs {
            if let Some(snapshot) = page.load_session() {
                page.restore_session(snapshot, &first_tab, window, cx);
            }
        }
        page
    }

    // Reads and deserializes the persisted session snapshot, if any.
    fn load_session(&self) -> Option<SessionSnapshot> {
        let bytes = self.store.as_ref()?.get(SESSION_KEY).log_err()??;
        serde_json::from_slice(&bytes).log_err()
    }

    // Rebuilds panes and tabs from a restored snapshot. `first_tab` is the single
    // tab the group was constructed with; it is reused as pane 0's first tab.
    fn restore_session(
        &mut self,
        snapshot: SessionSnapshot,
        first_tab: &Entity<ExplorerPane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(first_pane) = snapshot.panes.first() else {
            return;
        };
        self.group.set_direction(snapshot.direction);

        // Pane 0 reuses the existing first tab for its first directory, then adds
        // the rest. The root pane keeps its sidebar visible (issue #164).
        if let Some(cwd) = first_pane.tabs.first() {
            self.configure_tab(first_tab, Some(cwd.clone()), true, cx);
        }
        for cwd in first_pane.tabs.iter().skip(1) {
            if let Some(tab) = self.group.add_tab(0, window, cx) {
                self.subscribe_tab(&tab, cx);
                self.configure_tab(&tab, Some(cwd.clone()), true, cx);
            }
        }
        let active = clamp_index(first_pane.active_tab, self.group.tab_count(0));
        self.group.set_active_tab(0, active, window, cx);

        // Pane 1, if the snapshot had a split. `add_explorer_pane` collapses its
        // sidebar (split-created), matching a freshly split second pane.
        if let Some(second_pane) = snapshot.panes.get(1) {
            if let Some(index) =
                self.add_explorer_pane(second_pane.tabs.first().cloned(), window, cx)
            {
                for cwd in second_pane.tabs.iter().skip(1) {
                    if let Some(tab) = self.group.add_tab(index, window, cx) {
                        self.subscribe_tab(&tab, cx);
                        self.configure_tab(&tab, Some(cwd.clone()), false, cx);
                    }
                }
                let active = clamp_index(second_pane.active_tab, self.group.tab_count(index));
                self.group.set_active_tab(index, active, window, cx);
            }
        }

        let active_pane = clamp_index(snapshot.active_pane, self.group.pane_count());
        self.group.set_active(active_pane, window, cx);
    }

    // Subscribes to a tab's navigation events, keyed by its entity id so the map
    // stays consistent regardless of pane/tab position.
    fn subscribe_tab(&mut self, tab: &Entity<ExplorerPane>, cx: &mut Context<Self>) {
        let subscription = cx.subscribe(tab, Self::on_pane_event);
        self.tab_subscriptions.insert(tab.entity_id(), subscription);
    }

    // Replays the active `[ui]` config onto a freshly built tab and, when given,
    // roots it at `cwd` (forcing a reload). Shared by split, new-tab, and restore.
    fn configure_tab(
        &self,
        tab: &Entity<ExplorerPane>,
        cwd: Option<String>,
        sidebar_visible: bool,
        cx: &mut Context<Self>,
    ) {
        let ui = self.ui.clone();
        tab.update(cx, |pane, cx| {
            pane.apply_config_ui(&ui, cx);
            pane.sidebar_visible = sidebar_visible;
            if let Some(cwd) = cwd {
                pane.cwd = cwd;
                // Force a reload of the new root on the next render.
                pane.loaded = false;
            }
        });
    }

    // Creates a pane (with one tab) through the group, subscribes to the tab, and
    // replays the active `[ui]` config. Returns the new pane's index.
    fn add_explorer_pane(
        &mut self,
        cwd: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let (index, tab) = self.group.add_pane(window, cx)?;
        self.subscribe_tab(&tab, cx);
        // Split-created panes start with the sidebar collapsed (issue #164).
        self.configure_tab(&tab, cwd, false, cx);
        Some(index)
    }

    /// Opens a new tab in `pane_index`, rooted at home (§4), and activates it.
    fn new_tab_in_pane(&mut self, pane_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // Inherit the sidebar visibility of the pane's currently active tab so the
        // new tab looks continuous with what the user was just seeing.
        let sidebar_visible = self
            .group
            .tab(pane_index, self.group.active_tab(pane_index))
            .map(|tab| tab.read(cx).sidebar_visible)
            .unwrap_or(false);
        let Some(tab) = self.group.add_tab(pane_index, window, cx) else {
            return;
        };
        self.subscribe_tab(&tab, cx);
        self.configure_tab(&tab, Some(home_dir()), sidebar_visible, cx);
        // Make the pane active and focus its new (now active) tab.
        self.group.set_active(pane_index, window, cx);
        self.group.focus_active(window, cx);
        self.schedule_save(cx);
        cx.notify();
    }

    /// Closes the tab at `(pane_index, tab_index)`. When it is the pane's last
    /// tab, the whole pane closes (§3.1, §4).
    fn close_tab_in_pane(
        &mut self,
        pane_index: usize,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.group.close_tab(pane_index, tab_index, window, cx);
        if outcome.removed.is_empty() {
            return;
        }
        // Dropping each removed tab's subscription deregisters its event handler.
        for id in &outcome.removed {
            self.tab_subscriptions.remove(id);
        }
        self.schedule_save(cx);
        cx.notify();
    }

    /// Activates `(pane_index, tab_index)`, making its pane active too.
    fn activate_tab(
        &mut self,
        pane_index: usize,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group.set_active_tab(pane_index, tab_index, window, cx) {
            self.schedule_save(cx);
            cx.notify();
        }
    }

    /// Moves a tab within its pane (drag reorder, §4).
    fn reorder_tab(&mut self, pane_index: usize, from: usize, to: usize, cx: &mut Context<Self>) {
        if self.group.reorder_tab(pane_index, from, to) {
            self.schedule_save(cx);
            cx.notify();
        }
    }

    // Mirrors a tab's navigation into the other panes while syncing is enabled.
    fn on_pane_event(
        &mut self,
        source: Entity<ExplorerPane>,
        event: &PaneEvent,
        cx: &mut Context<Self>,
    ) {
        let PaneEvent::Navigated(path) = event;
        // A tab changed directory: persist the new session (a tab's cwd is part
        // of the snapshot), regardless of whether syncing is on.
        self.schedule_save(cx);
        if !self.synced_panes {
            return;
        }
        // Only mirror navigation from a visible (active) tab; a background tab
        // (e.g. one restored on startup) must not move the other panes.
        if !self.group.is_active_tab(source.entity_id()) {
            return;
        }
        for tab in self.group.other_active_tabs(source.entity_id()) {
            let path = path.clone();
            // `navigate_to_synced` (not `change_dir`) is deliberate: it does not
            // re-emit `PaneEvent::Navigated`, which would mirror back to the
            // source and loop indefinitely. Don't replace it with a regular
            // navigation method.
            tab.update(cx, |pane, cx| pane.navigate_to_synced(path, cx));
        }
    }

    /// Ensures a split exists and uses `direction`. With one pane it opens a
    /// second rooted at the active tab's directory; with two it just re-orients
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
        self.schedule_save(cx);
        cx.notify();
    }

    /// Closes a whole pane (its × button), keeping at least one open (§3.1).
    /// No-op for the last pane.
    pub fn close_pane(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let removed = self.group.remove_pane(index, window, cx);
        if removed.is_empty() {
            return;
        }
        for id in &removed {
            self.tab_subscriptions.remove(id);
        }
        self.schedule_save(cx);
        cx.notify();
    }

    /// Makes `index` the active pane and moves keyboard focus to it.
    pub fn set_active(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.set_active(index, window, cx) {
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.focus_next(window, cx) {
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn focus_prev(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.focus_prev(window, cx) {
            self.schedule_save(cx);
            cx.notify();
        }
    }

    /// Applies the `[ui]` config section to every tab (§5 of `config.md`).
    pub fn apply_config_ui(&mut self, ui: &Ui, cx: &mut Context<Self>) {
        self.ui = ui.clone();
        for tab in self.group.all_tabs() {
            tab.update(cx, |pane, cx| pane.apply_config_ui(ui, cx));
        }
    }

    /// Applies the `[explorer]` config section: the default split orientation and
    /// the synced-panes opt-in. Enabling sync immediately mirrors the active
    /// tab's directory into the other panes.
    pub fn apply_config_explorer(&mut self, explorer: &ExplorerConfig, cx: &mut Context<Self>) {
        // Only adopt the configured orientation while unsplit, so a config reload
        // does not silently flip a split the user arranged via the shortcuts.
        if self.group.pane_count() < 2 {
            self.group.set_direction(explorer.split_direction);
        }
        let enabling = explorer.synced_panes && !self.synced_panes;
        self.synced_panes = explorer.synced_panes;
        if enabling {
            let active_id = self.group.active_pane().entity_id();
            let path = self.group.active_pane().read(cx).cwd.clone();
            for tab in self.group.other_active_tabs(active_id) {
                let path = path.clone();
                tab.update(cx, |pane, cx| pane.navigate_to_synced(path, cx));
            }
            // Mirroring changed the other tabs' directories, but
            // `navigate_to_synced` deliberately emits no `Navigated` event, so
            // nothing else schedules a save. Persist the new session here.
            self.schedule_save(cx);
        }
        cx.notify();
    }

    /// Footer status for the active tab (a config error in `RootView` still
    /// takes precedence over this).
    pub fn status_for_footer(&self, cx: &App) -> Option<(String, bool)> {
        self.group.active_pane().read(cx).status_for_footer()
    }

    // Captures the current panes/tabs into a persistable snapshot (§4).
    fn session_snapshot(&self, cx: &App) -> SessionSnapshot {
        let panes = (0..self.group.pane_count())
            .map(|pane_index| {
                let tabs = (0..self.group.tab_count(pane_index))
                    .filter_map(|tab_index| {
                        self.group
                            .tab(pane_index, tab_index)
                            .map(|tab| tab.read(cx).cwd.clone())
                    })
                    .collect();
                PaneSnapshot {
                    tabs,
                    active_tab: self.group.active_tab(pane_index),
                }
            })
            .collect();
        SessionSnapshot {
            panes,
            active_pane: self.group.active(),
            direction: self.group.direction(),
        }
    }

    // Debounced session save: every mutation reschedules this, so a burst of
    // changes results in a single redb write after `SAVE_DEBOUNCE` of quiet. The
    // blocking `put` runs on the background executor, off the UI thread.
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.store.is_none() {
            return;
        }
        self.save_task = Some(cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                cx.background_executor().timer(SAVE_DEBOUNCE).await;
                let prepared = this
                    .read_with(&cx, |this, cx| {
                        let store = this.store.clone()?;
                        let bytes = serde_json::to_vec(&this.session_snapshot(cx)).log_err()?;
                        Some((store, bytes))
                    })
                    .ok()
                    .flatten();
                let Some((store, bytes)) = prepared else {
                    return;
                };
                cx.background_spawn(async move {
                    store.put(SESSION_KEY, &bytes).log_err();
                })
                .await;
            }
        }));
    }

    // Builds the render callbacks routing every tab/pane interaction back here.
    fn pane_callbacks() -> PaneGroupCallbacks<Self> {
        PaneGroupCallbacks {
            on_activate_pane: Rc::new(|this: &mut Self, pane, window, cx| {
                this.set_active(pane, window, cx)
            }),
            on_close_pane: Rc::new(|this: &mut Self, pane, window, cx| {
                this.close_pane(pane, window, cx)
            }),
            on_new_tab: Rc::new(|this: &mut Self, pane, window, cx| {
                this.new_tab_in_pane(pane, window, cx)
            }),
            on_activate_tab: Rc::new(|this: &mut Self, pane, tab, window, cx| {
                this.activate_tab(pane, tab, window, cx)
            }),
            on_close_tab: Rc::new(|this: &mut Self, pane, tab, window, cx| {
                this.close_tab_in_pane(pane, tab, window, cx)
            }),
            on_reorder_tab: Rc::new(|this: &mut Self, pane, from, to, _window, cx| {
                this.reorder_tab(pane, from, to, cx)
            }),
        }
    }
}

impl Render for ExplorerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let callbacks = Self::pane_callbacks();
        let body = self.group.render(cx, &callbacks);

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
            .on_action(cx.listener(|this, _: &NewTab, window, cx| {
                let pane = this.group.active();
                this.new_tab_in_pane(pane, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let pane = this.group.active();
                let tab = this.group.active_tab(pane);
                this.close_tab_in_pane(pane, tab, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| {
                let pane = this.group.active();
                let tab_index = this.group.active_tab(pane);
                if let Some(tab) = this.group.tab(pane, tab_index) {
                    tab.update(cx, |tab, cx| tab.toggle_sidebar(cx));
                }
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

    /// Number of tabs in `pane_index`.
    pub(crate) fn tab_count(&self, pane_index: usize) -> usize {
        self.group.tab_count(pane_index)
    }

    /// Active tab index of `pane_index`.
    pub(crate) fn active_tab(&self, pane_index: usize) -> usize {
        self.group.active_tab(pane_index)
    }

    /// The active tab of `pane_index` (the visible `ExplorerPane`).
    pub(crate) fn pane(&self, index: usize) -> Entity<ExplorerPane> {
        self.group
            .tab(index, self.group.active_tab(index))
            .expect("test requested an out-of-range pane")
    }

    /// The `cwd` of the tab at `(pane_index, tab_index)`.
    pub(crate) fn tab_cwd(&self, pane_index: usize, tab_index: usize, cx: &App) -> Option<String> {
        self.group
            .tab(pane_index, tab_index)
            .map(|tab| tab.read(cx).cwd.clone())
    }

    /// The active tab's `cwd` for `pane_index`.
    pub(crate) fn pane_cwd(&self, index: usize, cx: &App) -> Option<String> {
        self.tab_cwd(index, self.group.active_tab(index), cx)
    }

    /// Test entry points for the tab actions exercised without keybindings.
    pub(crate) fn test_new_tab(
        &mut self,
        pane_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_tab_in_pane(pane_index, window, cx);
    }

    pub(crate) fn test_close_tab(
        &mut self,
        pane_index: usize,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tab_in_pane(pane_index, tab_index, window, cx);
    }

    pub(crate) fn test_activate_tab(
        &mut self,
        pane_index: usize,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_tab(pane_index, tab_index, window, cx);
    }

    pub(crate) fn test_reorder_tab(
        &mut self,
        pane_index: usize,
        from: usize,
        to: usize,
        cx: &mut Context<Self>,
    ) {
        self.reorder_tab(pane_index, from, to, cx);
    }

    /// Number of live per-tab subscriptions; used to assert the subscription map
    /// stays consistent across splits/closes/new tabs.
    pub(crate) fn subscription_count(&self) -> usize {
        self.tab_subscriptions.len()
    }
}
