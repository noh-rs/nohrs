//! A reusable 2-way split/tab container, decoupled from the explorer.
//!
//! [`PaneGroup<T>`] owns the split/tab/active/direction/resize/render machinery
//! that used to live inside `ExplorerPage`, parameterized over any content type
//! `T` that implements [`PaneItem`]. The explorer adopts it today; a future page
//! (or plugin page) implements [`PaneItem`] and gets the same split behaviour for
//! free.
//!
//! The group is plain data owned by an embedding view entity, not an entity
//! itself: its mutators take `&mut App`/`&mut Window` and the embedder calls
//! `cx.notify()` after mutating it. Content-specific concerns the group must stay
//! ignorant of (navigation sync, config replay, per-pane subscriptions) live in
//! the embedder, which drives create/remove through the group's primitives.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable, ResizableState};
use gpui_component::{Icon, IconName};
use nohrs_core::config::SplitDirection;
use nohrs_ui::theme::theme;

/// Content that a [`PaneGroup`] can host as a pane/tab. Implemented by the views
/// shown inside a pane (today only `ExplorerPane`).
pub trait PaneItem: Render + Focusable + 'static {
    /// Short label shown on the pane's tab (e.g. the current directory name).
    fn tab_title(&self, cx: &App) -> String;

    /// Optional icon shown before the tab title. Defaults to none.
    fn tab_icon(&self, _cx: &App) -> Option<IconName> {
        None
    }
}

/// Builds a fresh pane entity. Separate from [`PaneItem`] because construction
/// needs embedder-owned services (e.g. the explorer's search service) that the
/// trait should not know about.
type BuildPane<T> = Box<dyn Fn(&mut Window, &mut App) -> Entity<T>>;

/// Maximum panes a group holds. The split is 2-way today (P2 cap); full N-way
/// tabs are tracked in #62.
const MAX_PANES: usize = 2;

/// A 2-way split container over independently-rendered panes of type `T`.
pub struct PaneGroup<T: PaneItem> {
    // Invariant: always non-empty and at most two entries (2-way cap).
    panes: Vec<Entity<T>>,
    /// Index of the pane keyboard input and shortcuts act on.
    active: usize,
    /// Orientation a split uses; toggled by the split shortcuts.
    direction: SplitDirection,
    /// Resizable state for the divider between the two panes.
    pane_resizable: Entity<ResizableState>,
    build_pane: BuildPane<T>,
}

impl<T: PaneItem> PaneGroup<T> {
    /// Builds a group with a single pane. Returns the first pane so the embedder
    /// can wire it up (subscribe, replay config, set initial state).
    pub fn new(
        build_pane: BuildPane<T>,
        pane_resizable: Entity<ResizableState>,
        window: &mut Window,
        cx: &mut App,
    ) -> (Self, Entity<T>) {
        let first = build_pane(window, cx);
        let group = Self {
            panes: vec![first.clone()],
            active: 0,
            direction: SplitDirection::default(),
            pane_resizable,
            build_pane,
        };
        (group, first)
    }

    /// Builds a pane and appends it, enforcing the 2-way cap. Returns its index
    /// and entity (or `None` when already at [`MAX_PANES`]) so the embedder can
    /// subscribe to it and apply per-pane configuration. Does not change the
    /// active pane or notify; that is the embedder's responsibility.
    pub fn add_pane(&mut self, window: &mut Window, cx: &mut App) -> Option<(usize, Entity<T>)> {
        if self.panes.len() >= MAX_PANES {
            return None;
        }
        let pane = (self.build_pane)(window, cx);
        self.panes.push(pane.clone());
        Some((self.panes.len() - 1, pane))
    }

    /// Removes the pane at `index`, keeping at least one open. Returns whether a
    /// pane was actually removed so the embedder can drop its aligned per-pane
    /// state (e.g. a subscription) at the same index. Rebinds the active pane and
    /// moves focus to it.
    pub fn remove_pane(&mut self, index: usize, window: &mut Window, cx: &mut App) -> bool {
        if self.panes.len() <= 1 || index >= self.panes.len() {
            return false;
        }
        self.panes.remove(index);
        if self.active >= self.panes.len() {
            self.active = self.panes.len() - 1;
        }
        self.focus_active(window, cx);
        true
    }

    /// Sets the orientation a split renders with.
    pub fn set_direction(&mut self, direction: SplitDirection) {
        self.direction = direction;
    }

    /// Makes `index` the active pane and focuses it. Returns whether the active
    /// pane changed (out-of-range or unchanged indices are ignored).
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

    /// Focuses the active pane. Falls back to the first pane rather than
    /// panicking should the `active` invariant ever be violated.
    pub fn focus_active(&self, window: &mut Window, cx: &mut App) {
        let handle = self.active_pane().read(cx).focus_handle(cx);
        handle.focus(window);
    }

    /// The currently active pane, falling back to the first to avoid panicking.
    pub fn active_pane(&self) -> &Entity<T> {
        self.panes.get(self.active).unwrap_or(&self.panes[0])
    }

    /// All panes, in display order.
    pub fn panes(&self) -> &[Entity<T>] {
        &self.panes
    }

    /// Number of open panes (1 or 2).
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Index of the active pane.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Current split orientation.
    pub fn direction(&self) -> SplitDirection {
        self.direction
    }

    /// The pane at `index`, if it exists.
    pub fn pane(&self, index: usize) -> Option<Entity<T>> {
        self.panes.get(index).cloned()
    }

    /// Renders the split. `on_activate(index)` fires when a pane is clicked;
    /// `on_close(index)` fires when its close button is clicked. The hooks run
    /// against the embedding view `V` so it can keep its own state in sync.
    pub fn render<V, FA, FC>(
        &self,
        cx: &mut Context<V>,
        on_activate: FA,
        on_close: FC,
    ) -> AnyElement
    where
        V: 'static,
        FA: Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
        FC: Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    {
        if self.panes.len() > 1 {
            let first = self.render_pane(0, cx, on_activate.clone(), on_close.clone());
            let second = self.render_pane(1, cx, on_activate, on_close);
            let group = match self.direction {
                SplitDirection::Vertical => h_resizable("pane-group", self.pane_resizable.clone()),
                SplitDirection::Horizontal => {
                    v_resizable("pane-group", self.pane_resizable.clone())
                }
            };
            group
                .child(resizable_panel().child(first))
                .child(resizable_panel().child(second))
                .into_any_element()
        } else {
            self.render_pane(0, cx, on_activate, on_close)
        }
    }

    fn render_pane<V, FA, FC>(
        &self,
        index: usize,
        cx: &mut Context<V>,
        on_activate: FA,
        on_close: FC,
    ) -> AnyElement
    where
        V: 'static,
        FA: Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
        FC: Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    {
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
                cx.listener(move |this, _event, window, cx| on_activate(this, index, window, cx)),
            )
            .child(self.render_tab_bar(index, split, cx, on_close))
            .child(div().flex_1().min_h(px(0.0)).overflow_hidden().child(pane))
            .into_any_element()
    }

    // The pane-local tab bar: shows the content's tab title as a single tab (full
    // multi-tab support is tracked in #62) plus the pane-close button, which only
    // appears once a split exists.
    fn render_tab_bar<V, FC>(
        &self,
        index: usize,
        split: bool,
        cx: &mut Context<V>,
        on_close: FC,
    ) -> impl IntoElement
    where
        V: 'static,
        FC: Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    {
        let (title, icon) = {
            let item = self.panes[index].read(cx);
            (item.tab_title(cx), item.tab_icon(cx))
        };
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
                    .gap(px(6.0))
                    .h(px(24.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .text_sm()
                    .when(is_active, |this| {
                        this.bg(rgb(theme::TOOLBAR_ACTIVE_BG))
                            .text_color(rgb(theme::TOOLBAR_ACTIVE_TEXT))
                    })
                    .when(!is_active, |this| this.text_color(rgb(theme::TOOLBAR_TEXT)))
                    .when_some(icon, |this, icon| this.child(Icon::new(icon).size_4()))
                    .child(title),
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
                        // Swallow the press so the pane-wide `on_mouse_down`
                        // doesn't activate the pane we're about to close.
                        .on_mouse_down(MouseButton::Left, |_, _window, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            on_close(this, index, window, cx)
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
