//! A reusable 2-way split / per-pane tab container, decoupled from the explorer.
//!
//! [`PaneGroup<T>`] owns the split/tab/active/direction/resize/render machinery
//! that used to live inside `ExplorerPage`, parameterized over any content type
//! `T` that implements [`PaneItem`]. The explorer adopts it today; a future page
//! (or plugin page) implements [`PaneItem`] and gets the same behaviour for free.
//!
//! Each pane owns an independent list of tabs (`docs/explorer-essentials.md` §4),
//! of which one is active and rendered; the group holds at most two panes
//! (2-way split is the P2 cap, §3.1). The group is plain data owned by an
//! embedding view entity, not an entity itself: its mutators take
//! `&mut App`/`&mut Window` and the embedder calls `cx.notify()` after mutating
//! it. Content-specific concerns the group must stay ignorant of (navigation
//! sync, config replay, per-tab subscriptions) live in the embedder, which drives
//! create/remove through the group's primitives.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel, v_resizable};
use gpui_component::{Icon, IconName};
use nohrs_core::config::SplitDirection;
use nohrs_ui::theme::theme;

/// Content that a [`PaneGroup`] can host as a tab. Implemented by the views shown
/// inside a tab (today only `ExplorerPane`).
pub trait PaneItem: Render + Focusable + 'static {
    /// Short label shown on the tab (e.g. the current directory name).
    fn tab_title(&self, cx: &App) -> String;

    /// Optional icon shown before the tab title. Defaults to none.
    fn tab_icon(&self, _cx: &App) -> Option<IconName> {
        None
    }
}

/// Builds a fresh tab entity. Separate from [`PaneItem`] because construction
/// needs embedder-owned services (e.g. the explorer's search service) that the
/// trait should not know about.
type BuildPane<T> = Box<dyn Fn(&mut Window, &mut App) -> Entity<T>>;

/// Maximum panes a group holds. The split is 2-way today (P2 cap, §3.1); 3+ way
/// splits are P3+. (Tabs within a pane are unbounded — see [`Pane`].)
const MAX_PANES: usize = 2;

/// One pane: an ordered, non-empty list of tabs with one active.
struct Pane<T: PaneItem> {
    // Invariant: always non-empty.
    tabs: Vec<Entity<T>>,
    /// Index of the tab rendered and acted on within this pane.
    active_tab: usize,
}

impl<T: PaneItem> Pane<T> {
    fn with_tab(tab: Entity<T>) -> Self {
        Self {
            tabs: vec![tab],
            active_tab: 0,
        }
    }

    /// The active tab, falling back to the first to avoid panicking should the
    /// `active_tab` invariant ever be violated.
    fn active(&self) -> &Entity<T> {
        self.tabs.get(self.active_tab).unwrap_or(&self.tabs[0])
    }
}

/// The outcome of [`PaneGroup::close_tab`]: which tab entities were removed (so
/// the embedder can drop their per-tab state) and whether the whole pane closed
/// because the last tab was the one closed.
#[derive(Debug, Default)]
pub struct TabCloseOutcome {
    /// Entity ids of every tab removed by the close.
    pub removed: Vec<EntityId>,
    /// Whether closing the tab also closed its pane (it was the pane's last tab).
    pub pane_closed: bool,
}

/// A pane-indexed render hook: `(view, pane, window, cx)`.
type PaneHook<V> = Rc<dyn Fn(&mut V, usize, &mut Window, &mut Context<V>)>;
/// A tab-indexed render hook: `(view, pane, tab, window, cx)`.
type TabHook<V> = Rc<dyn Fn(&mut V, usize, usize, &mut Window, &mut Context<V>)>;
/// A tab-reorder render hook: `(view, pane, from, to, window, cx)`.
type ReorderHook<V> = Rc<dyn Fn(&mut V, usize, usize, usize, &mut Window, &mut Context<V>)>;

/// The view-state hooks a [`PaneGroup`] needs to render its interactive chrome.
/// Bundled into one struct because the tab bar wires up six distinct actions;
/// each closure runs against the embedding view `V`. Closures are `Rc`-wrapped so
/// the struct can be cheaply cloned across the two panes and their tabs.
pub struct PaneGroupCallbacks<V: 'static> {
    /// A pane was clicked: `(pane)`.
    pub on_activate_pane: PaneHook<V>,
    /// A pane's close button was clicked: `(pane)`.
    pub on_close_pane: PaneHook<V>,
    /// The new-tab (`+`) button was clicked: `(pane)`.
    pub on_new_tab: PaneHook<V>,
    /// A tab was clicked: `(pane, tab)`.
    pub on_activate_tab: TabHook<V>,
    /// A tab's close button was clicked: `(pane, tab)`.
    pub on_close_tab: TabHook<V>,
    /// A tab was dragged onto another within the same pane: `(pane, from, to)`.
    pub on_reorder_tab: ReorderHook<V>,
}

// Manual `Clone` (derive would wrongly demand `V: Clone`; `Rc` clones regardless).
impl<V: 'static> Clone for PaneGroupCallbacks<V> {
    fn clone(&self) -> Self {
        Self {
            on_activate_pane: self.on_activate_pane.clone(),
            on_close_pane: self.on_close_pane.clone(),
            on_new_tab: self.on_new_tab.clone(),
            on_activate_tab: self.on_activate_tab.clone(),
            on_close_tab: self.on_close_tab.clone(),
            on_reorder_tab: self.on_reorder_tab.clone(),
        }
    }
}

/// The drag payload for a tab reorder: identifies the tab being dragged by its
/// `(pane, tab)` position. Matched by type at the drop site.
#[derive(Clone)]
struct TabDrag {
    pane: usize,
    tab: usize,
}

/// The view rendered under the cursor while a tab is dragged.
struct TabDragPreview {
    title: SharedString,
}

impl Render for TabDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(rgb(theme::TOOLBAR_ACTIVE_BG))
            .text_color(rgb(theme::TOOLBAR_ACTIVE_TEXT))
            .text_sm()
            .child(self.title.clone())
    }
}

/// A 2-way split container over panes, each with its own independently-navigating
/// tabs of type `T`.
pub struct PaneGroup<T: PaneItem> {
    // Invariant: always non-empty and at most two entries (2-way cap).
    panes: Vec<Pane<T>>,
    /// Index of the pane keyboard input and shortcuts act on.
    active: usize,
    /// Orientation a split uses; toggled by the split shortcuts.
    direction: SplitDirection,
    /// Resizable state for the divider between the two panes.
    pane_resizable: Entity<ResizableState>,
    build_pane: BuildPane<T>,
}

impl<T: PaneItem> PaneGroup<T> {
    /// Builds a group with a single pane holding a single tab. Returns that tab
    /// so the embedder can wire it up (subscribe, replay config, set initial
    /// state).
    pub fn new(
        build_pane: BuildPane<T>,
        pane_resizable: Entity<ResizableState>,
        window: &mut Window,
        cx: &mut App,
    ) -> (Self, Entity<T>) {
        let first = build_pane(window, cx);
        let group = Self {
            panes: vec![Pane::with_tab(first.clone())],
            active: 0,
            direction: SplitDirection::default(),
            pane_resizable,
            build_pane,
        };
        (group, first)
    }

    /// Builds a pane (with one tab) and appends it, enforcing the 2-way cap.
    /// Returns the new pane's index and its first tab (or `None` when already at
    /// [`MAX_PANES`]) so the embedder can subscribe to the tab and apply per-pane
    /// configuration. Does not change the active pane or notify; that is the
    /// embedder's responsibility.
    pub fn add_pane(&mut self, window: &mut Window, cx: &mut App) -> Option<(usize, Entity<T>)> {
        if self.panes.len() >= MAX_PANES {
            return None;
        }
        let tab = (self.build_pane)(window, cx);
        self.panes.push(Pane::with_tab(tab.clone()));
        Some((self.panes.len() - 1, tab))
    }

    /// Removes the pane at `index`, keeping at least one open. Returns the entity
    /// ids of every tab removed (empty if nothing was removed) so the embedder
    /// can drop the matching per-tab state. Rebinds the active pane and moves
    /// focus to it.
    pub fn remove_pane(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<EntityId> {
        if self.panes.len() <= 1 || index >= self.panes.len() {
            return Vec::new();
        }
        let removed = self.panes.remove(index);
        if self.active >= self.panes.len() {
            self.active = self.panes.len() - 1;
        }
        self.focus_active(window, cx);
        removed.tabs.iter().map(|tab| tab.entity_id()).collect()
    }

    /// Builds a tab, appends it to the pane at `pane_index`, and makes it that
    /// pane's active tab. Returns the new tab (or `None` for an out-of-range
    /// pane) so the embedder can subscribe and configure it. Does not change the
    /// active pane or notify.
    pub fn add_tab(
        &mut self,
        pane_index: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<T>> {
        if pane_index >= self.panes.len() {
            return None;
        }
        let tab = (self.build_pane)(window, cx);
        let pane = &mut self.panes[pane_index];
        pane.tabs.push(tab.clone());
        pane.active_tab = pane.tabs.len() - 1;
        Some(tab)
    }

    /// Closes the tab at `(pane_index, tab_index)`. When it is the pane's last
    /// tab, the whole pane closes (subject to the min-one-pane rule, §3.1).
    /// Returns which tab entities were removed and whether the pane closed, so
    /// the embedder can drop the matching per-tab state.
    pub fn close_tab(
        &mut self,
        pane_index: usize,
        tab_index: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> TabCloseOutcome {
        let tab_len = match self.panes.get(pane_index) {
            Some(pane) if tab_index < pane.tabs.len() => pane.tabs.len(),
            _ => return TabCloseOutcome::default(),
        };

        if tab_len > 1 {
            let removed_id = {
                let pane = &mut self.panes[pane_index];
                let removed = pane.tabs.remove(tab_index);
                if pane.active_tab >= pane.tabs.len() {
                    pane.active_tab = pane.tabs.len() - 1;
                } else if tab_index < pane.active_tab {
                    pane.active_tab -= 1;
                }
                removed.entity_id()
            };
            if pane_index == self.active {
                self.focus_active(window, cx);
            }
            return TabCloseOutcome {
                removed: vec![removed_id],
                pane_closed: false,
            };
        }

        // The pane's last tab: closing it closes the pane (a no-op for the last
        // pane, which `remove_pane` guards).
        let removed = self.remove_pane(pane_index, window, cx);
        let pane_closed = !removed.is_empty();
        TabCloseOutcome {
            removed,
            pane_closed,
        }
    }

    /// Makes `tab_index` the active tab of `pane_index` and that pane the active
    /// pane, moving focus to it. Returns whether anything changed.
    pub fn set_active_tab(
        &mut self,
        pane_index: usize,
        tab_index: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        match self.panes.get(pane_index) {
            Some(pane) if tab_index < pane.tabs.len() => {}
            _ => return false,
        }
        let changed = self.active != pane_index || self.panes[pane_index].active_tab != tab_index;
        self.panes[pane_index].active_tab = tab_index;
        self.active = pane_index;
        self.focus_active(window, cx);
        changed
    }

    /// Moves the tab at `from` to `to` within `pane_index`, preserving which tab
    /// is active. Returns whether the order changed.
    pub fn reorder_tab(&mut self, pane_index: usize, from: usize, to: usize) -> bool {
        let Some(pane) = self.panes.get_mut(pane_index) else {
            return false;
        };
        if from >= pane.tabs.len() || to >= pane.tabs.len() || from == to {
            return false;
        }
        let active_id = pane.tabs[pane.active_tab].entity_id();
        let tab = pane.tabs.remove(from);
        pane.tabs.insert(to, tab);
        if let Some(position) = pane
            .tabs
            .iter()
            .position(|tab| tab.entity_id() == active_id)
        {
            pane.active_tab = position;
        }
        true
    }

    /// Sets the orientation a split renders with.
    pub fn set_direction(&mut self, direction: SplitDirection) {
        self.direction = direction;
    }

    /// Makes `index` the active pane and focuses its active tab. Returns whether
    /// the active pane changed (out-of-range or unchanged indices are ignored).
    pub fn set_active(&mut self, index: usize, window: &mut Window, cx: &mut App) -> bool {
        if index >= self.panes.len() || index == self.active {
            return false;
        }
        self.active = index;
        self.focus_active(window, cx);
        true
    }

    /// Moves focus to the next pane, wrapping around. Returns whether it moved.
    pub fn focus_next(&mut self, window: &mut Window, cx: &mut App) -> bool {
        if self.panes.len() < 2 {
            return false;
        }
        self.set_active((self.active + 1) % self.panes.len(), window, cx)
    }

    /// Moves focus to the previous pane, wrapping around. Returns whether it moved.
    pub fn focus_prev(&mut self, window: &mut Window, cx: &mut App) -> bool {
        if self.panes.len() < 2 {
            return false;
        }
        let count = self.panes.len();
        self.set_active((self.active + count - 1) % count, window, cx)
    }

    /// Focuses the active pane's active tab.
    pub fn focus_active(&self, window: &mut Window, cx: &mut App) {
        let handle = self.active_pane().read(cx).focus_handle(cx);
        handle.focus(window);
    }

    /// The active tab of the active pane, falling back to the first to avoid
    /// panicking.
    pub fn active_pane(&self) -> &Entity<T> {
        self.panes
            .get(self.active)
            .unwrap_or(&self.panes[0])
            .active()
    }

    /// The active tab of every pane, in pane order.
    pub fn active_tabs(&self) -> Vec<Entity<T>> {
        self.panes
            .iter()
            .map(|pane| pane.active().clone())
            .collect()
    }

    /// Every tab across every pane, in display order.
    pub fn all_tabs(&self) -> Vec<Entity<T>> {
        self.panes
            .iter()
            .flat_map(|pane| pane.tabs.iter().cloned())
            .collect()
    }

    /// The active tab of every pane except the one containing the tab `id`. Used
    /// by the embedder to mirror navigation across panes.
    pub fn other_active_tabs(&self, id: EntityId) -> Vec<Entity<T>> {
        let source_pane = self.locate_tab(id).map(|(pane_index, _)| pane_index);
        self.panes
            .iter()
            .enumerate()
            .filter(|(pane_index, _)| Some(*pane_index) != source_pane)
            .map(|(_, pane)| pane.active().clone())
            .collect()
    }

    /// Whether the tab `id` is the active tab of its pane.
    pub fn is_active_tab(&self, id: EntityId) -> bool {
        match self.locate_tab(id) {
            Some((pane_index, tab_index)) => self.panes[pane_index].active_tab == tab_index,
            None => false,
        }
    }

    /// Locate a tab by entity id, returning `(pane_index, tab_index)`.
    pub fn locate_tab(&self, id: EntityId) -> Option<(usize, usize)> {
        for (pane_index, pane) in self.panes.iter().enumerate() {
            for (tab_index, tab) in pane.tabs.iter().enumerate() {
                if tab.entity_id() == id {
                    return Some((pane_index, tab_index));
                }
            }
        }
        None
    }

    /// Number of open panes (1 or 2).
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Index of the active pane.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Number of tabs in `pane_index` (0 for an out-of-range pane).
    pub fn tab_count(&self, pane_index: usize) -> usize {
        self.panes
            .get(pane_index)
            .map(|pane| pane.tabs.len())
            .unwrap_or(0)
    }

    /// Active tab index of `pane_index` (0 for an out-of-range pane).
    pub fn active_tab(&self, pane_index: usize) -> usize {
        self.panes
            .get(pane_index)
            .map(|pane| pane.active_tab)
            .unwrap_or(0)
    }

    /// The tab at `(pane_index, tab_index)`, if it exists.
    pub fn tab(&self, pane_index: usize, tab_index: usize) -> Option<Entity<T>> {
        self.panes
            .get(pane_index)
            .and_then(|pane| pane.tabs.get(tab_index).cloned())
    }

    /// Current split orientation.
    pub fn direction(&self) -> SplitDirection {
        self.direction
    }

    /// Renders the split. The `callbacks` route every interaction (pane/tab
    /// activate, close, new tab, reorder) back to the embedding view `V` so it
    /// can keep its own state in sync.
    pub fn render<V>(&self, cx: &mut Context<V>, callbacks: &PaneGroupCallbacks<V>) -> AnyElement
    where
        V: 'static,
    {
        if self.panes.len() > 1 {
            let first = self.render_pane(0, cx, callbacks);
            let second = self.render_pane(1, cx, callbacks);
            let group = match self.direction {
                SplitDirection::Vertical => {
                    h_resizable("pane-group").with_state(&self.pane_resizable)
                }
                SplitDirection::Horizontal => {
                    v_resizable("pane-group").with_state(&self.pane_resizable)
                }
            };
            group
                .child(resizable_panel().child(first))
                .child(resizable_panel().child(second))
                .into_any_element()
        } else {
            self.render_pane(0, cx, callbacks)
        }
    }

    fn render_pane<V>(
        &self,
        pane_index: usize,
        cx: &mut Context<V>,
        callbacks: &PaneGroupCallbacks<V>,
    ) -> AnyElement
    where
        V: 'static,
    {
        let content = self.panes[pane_index].active().clone();
        let split = self.panes.len() > 1;
        let is_active = split && pane_index == self.active;
        let on_activate_pane = callbacks.on_activate_pane.clone();
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
                cx.listener(move |this, _event, window, cx| {
                    on_activate_pane(this, pane_index, window, cx)
                }),
            )
            .child(self.render_tab_bar(pane_index, split, cx, callbacks))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(content),
            )
            .into_any_element()
    }

    // The pane-local tab bar: one chip per tab (active highlight, per-tab close
    // when more than one tab, drag-to-reorder), the new-tab `+` button, and the
    // pane-close button which only appears once a split exists.
    fn render_tab_bar<V>(
        &self,
        pane_index: usize,
        split: bool,
        cx: &mut Context<V>,
        callbacks: &PaneGroupCallbacks<V>,
    ) -> impl IntoElement
    where
        V: 'static,
    {
        let pane = &self.panes[pane_index];
        let tab_count = pane.tabs.len();
        let active_tab = pane.active_tab;
        let tabs: Vec<AnyElement> = pane
            .tabs
            .iter()
            .enumerate()
            .map(|(tab_index, tab)| {
                let (title, icon) = {
                    let item = tab.read(cx);
                    (item.tab_title(cx), item.tab_icon(cx))
                };
                self.render_tab(
                    pane_index,
                    tab_index,
                    title,
                    icon,
                    tab_index == active_tab,
                    tab_count > 1,
                    cx,
                    callbacks,
                )
            })
            .collect();

        let on_new_tab = callbacks.on_new_tab.clone();
        let on_close_pane = callbacks.on_close_pane.clone();

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
            .children(tabs)
            // New-tab affordance (`Cmd/Ctrl+T`, §4 / §6).
            .child(
                div()
                    .id(SharedString::from(format!("new-tab-{pane_index}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme::TOOLBAR_HOVER)))
                    // Swallow the press so the pane-wide `on_mouse_down` does not
                    // also fire (it would still activate the pane, which is fine,
                    // but keeps the interaction crisp).
                    .on_mouse_down(MouseButton::Left, |_, _window, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        on_new_tab(this, pane_index, window, cx)
                    }))
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
                        .id(("close-pane", pane_index))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(22.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme::TOOLBAR_HOVER)))
                        // Swallow the press so the pane-wide `on_mouse_down`
                        // doesn't activate the pane we're about to close.
                        .on_mouse_down(MouseButton::Left, |_, _window, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            on_close_pane(this, pane_index, window, cx)
                        }))
                        .child(
                            Icon::new(IconName::Close)
                                .size_4()
                                .text_color(rgb(theme::TOOLBAR_TEXT)),
                        ),
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn render_tab<V>(
        &self,
        pane_index: usize,
        tab_index: usize,
        title: String,
        icon: Option<IconName>,
        is_active: bool,
        show_close: bool,
        cx: &mut Context<V>,
        callbacks: &PaneGroupCallbacks<V>,
    ) -> AnyElement
    where
        V: 'static,
    {
        let on_activate_tab = callbacks.on_activate_tab.clone();
        let on_close_tab = callbacks.on_close_tab.clone();
        let on_reorder_tab = callbacks.on_reorder_tab.clone();
        let drag_title = SharedString::from(title.clone());

        div()
            .id(SharedString::from(format!("tab-{pane_index}-{tab_index}")))
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .text_sm()
            .cursor_pointer()
            .when(is_active, |this| {
                this.bg(rgb(theme::TOOLBAR_ACTIVE_BG))
                    .text_color(rgb(theme::TOOLBAR_ACTIVE_TEXT))
            })
            .when(!is_active, |this| {
                this.text_color(rgb(theme::TOOLBAR_TEXT))
                    .hover(|style| style.bg(rgb(theme::TOOLBAR_HOVER)))
            })
            .on_click(cx.listener(move |this, _event, window, cx| {
                on_activate_tab(this, pane_index, tab_index, window, cx)
            }))
            .on_drag(
                TabDrag {
                    pane: pane_index,
                    tab: tab_index,
                },
                move |_drag, _offset, _window, cx| {
                    cx.new(|_cx| TabDragPreview {
                        title: drag_title.clone(),
                    })
                },
            )
            // Only same-pane reorders are supported (§4 — reorder within the tab
            // bar); a drop from another pane is ignored.
            .on_drop(cx.listener(move |this, drag: &TabDrag, window, cx| {
                if drag.pane == pane_index {
                    on_reorder_tab(this, pane_index, drag.tab, tab_index, window, cx);
                }
            }))
            .when_some(icon, |this, icon| this.child(Icon::new(icon).size_4()))
            .child(title)
            .when(show_close, |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!(
                            "close-tab-{pane_index}-{tab_index}"
                        )))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(16.0))
                        .rounded(px(4.0))
                        .hover(|style| style.bg(rgb(theme::TOOLBAR_HOVER)))
                        // Swallow the press so closing a tab doesn't also activate
                        // it (or start a drag).
                        .on_mouse_down(MouseButton::Left, |_, _window, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            on_close_tab(this, pane_index, tab_index, window, cx)
                        }))
                        .child(Icon::new(IconName::Close).size_4()),
                )
            })
            .into_any_element()
    }
}
