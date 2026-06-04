//! The explorer page: a dock of independently-navigating panes.
//!
//! [`ExplorerPage`] owns a gpui-component [`DockArea`] whose center holds one or
//! more [`ExplorerPane`]s as draggable tabs (`docs/explorer-essentials.md` §3).
//! The dock provides tab drag-and-drop within and between splits, edge-drop
//! splitting, and a horizontally-scrollable tab bar. `ExplorerPage` adds the
//! explorer-specific layer on top: one shared quick-access sidebar (whose clicks
//! and drags drive the active pane), replaying `[ui]` config onto new panes, and
//! mirroring navigation across panes when `synced_panes` is enabled (§3.2).
//!
//! Panes are tracked in a `panes` registry of weak handles (the dock owns the
//! strong references). Because tab drag-and-drop can destroy a pane at any time,
//! the registry is pruned of dead handles whenever the dock layout changes.

use std::sync::{Arc, Once};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::dock::{DockArea, DockEvent, DockItem, DockPlacement, PanelView, TabPanel};
use gpui_component::resizable::{h_resizable, resizable_panel, ResizableState};
use gpui_component::{Icon, IconName, Placement};
use nohrs_core::config::{Explorer as ExplorerConfig, SplitDirection, Ui};
use nohrs_services::search::SearchService;
use nohrs_ui::theme::theme;

use super::state::ExplorerPane;
use super::types::{ExplorerDrag, PaneEvent};

// Key context the pane shortcuts are bound under, so they only fire while the
// explorer (and not another page) is focused.
const PANES_CONTEXT: &str = "ExplorerPanes";

actions!(
    explorer_panes,
    [
        /// Split the active pane into a left/right split.
        SplitVertical,
        /// Split the active pane into a top/bottom split.
        SplitHorizontal,
        /// Move focus to the first pane.
        FocusPane1,
        /// Move focus to the second pane.
        FocusPane2,
        /// Move focus to the next pane, wrapping around.
        FocusNextPane,
        /// Move focus to the previous pane, wrapping around.
        FocusPrevPane,
        /// Toggle the shared quick-access sidebar.
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
            KeyBinding::new("cmd-b", ToggleSidebar, Some(PANES_CONTEXT)),
            KeyBinding::new("ctrl-b", ToggleSidebar, Some(PANES_CONTEXT)),
        ]);
    });
}

// Which edge of the dock a drag is hovering, deciding where a dropped directory
// opens: as a new tab (center) or a new split on that edge.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl DropZone {
    // Edge zones map to the dock's split placement; `Center` opens a tab instead
    // of splitting, so it has no placement.
    fn placement(self) -> Option<Placement> {
        match self {
            DropZone::Left => Some(Placement::Left),
            DropZone::Right => Some(Placement::Right),
            DropZone::Top => Some(Placement::Top),
            DropZone::Bottom => Some(Placement::Bottom),
            DropZone::Center => None,
        }
    }
}

// Classifies a cursor position within `bounds` into a drop zone: the outer 35%
// band on each side is an edge split, the middle is a new tab. Mirrors the dock's
// own `TabPanel` edge detection so the two feel consistent.
fn drop_zone_for(bounds: Bounds<Pixels>, position: Point<Pixels>) -> DropZone {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width <= 0.0 || height <= 0.0 {
        return DropZone::Center;
    }
    let fx = (f32::from(position.x) - f32::from(bounds.origin.x)) / width;
    let fy = (f32::from(position.y) - f32::from(bounds.origin.y)) / height;
    const EDGE: f32 = 0.35;
    // Prefer the horizontal edges when the cursor is nearer a left/right band than
    // a top/bottom one, so corners resolve to a single, predictable split.
    let left = fx;
    let right = 1.0 - fx;
    let top = fy;
    let bottom = 1.0 - fy;
    let min = left.min(right).min(top).min(bottom);
    if min > EDGE {
        DropZone::Center
    } else if min == left {
        DropZone::Left
    } else if min == right {
        DropZone::Right
    } else if min == top {
        DropZone::Top
    } else {
        DropZone::Bottom
    }
}

// A registered pane: a weak handle (the dock owns the strong reference) paired
// with the navigation subscription kept alive for its lifetime.
struct PaneEntry {
    pane: WeakEntity<ExplorerPane>,
    _subscription: Subscription,
}

/// The explorer page: a dock of independently-navigating panes with a shared
/// quick-access sidebar. With a single pane it renders like an unsplit explorer.
pub struct ExplorerPage {
    // The dock hosting the panes as draggable tabs/splits.
    dock_area: Entity<DockArea>,
    // The tab panel the initial pane lives in; split/new-tab operations target the
    // first tab panel in the dock tree, falling back to this.
    root_tab_panel: Entity<TabPanel>,
    // Weak handles to every live pane, each with its navigation subscription.
    // Pruned on dock layout changes (tab drag-and-drop can destroy panes).
    panes: Vec<PaneEntry>,
    // Forwards dock layout changes so the registry can be pruned.
    _dock_subscription: Subscription,
    /// Whether navigation in one pane mirrors into the others (§3.2).
    synced_panes: bool,
    // Last-applied `[ui]` settings, replayed onto panes opened later so they match
    // config rather than reverting to pane defaults.
    ui: Ui,
    // Whether the shared quick-access sidebar is shown (toggled with `Cmd/Ctrl+B`).
    sidebar_visible: bool,
    // Builds the per-pane search backend; needed because the page (not a group)
    // now constructs panes directly.
    search_service: Option<Arc<SearchService>>,
    // Resizable state for the sidebar/dock divider.
    shell_resizable: Entity<ResizableState>,
    // The edge a directory drag is hovering, for the drop-zone preview.
    drop_zone: Option<DropZone>,
    focus_handle: FocusHandle,
}

impl Focusable for ExplorerPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ExplorerPage {
    /// Builds the explorer with a single pane inside a fresh dock. `shell_resizable`
    /// is supplied by the application (it owns app-level state) and drives the
    /// sidebar/dock divider.
    pub fn new(
        shell_resizable: Entity<ResizableState>,
        search_service: Option<Arc<SearchService>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        bind_pane_keys(cx);

        let dock_area = cx.new(|cx| DockArea::new("explorer-dock", None, window, cx));
        let first_pane = cx.new(|cx| ExplorerPane::build(search_service.clone(), window, cx));

        let root_tab_panel = dock_area.update(cx, |area, cx| {
            let weak = cx.entity().downgrade();
            let item = DockItem::tabs(
                vec![Arc::new(first_pane.clone()) as Arc<dyn PanelView>],
                Some(0),
                &weak,
                window,
                cx,
            );
            let tab_panel = match &item {
                DockItem::Tabs { view, .. } => view.clone(),
                // `DockItem::tabs` always returns a `Tabs` variant.
                _ => unreachable!("DockItem::tabs builds a Tabs item"),
            };
            area.set_center(item, window, cx);
            tab_panel
        });

        let dock_subscription = cx.subscribe(&dock_area, Self::on_dock_event);

        let mut page = Self {
            dock_area,
            root_tab_panel,
            panes: Vec::new(),
            _dock_subscription: dock_subscription,
            synced_panes: false,
            ui: Ui::default(),
            sidebar_visible: true,
            search_service,
            shell_resizable,
            drop_zone: None,
            focus_handle: cx.focus_handle(),
        };
        page.register_pane(&first_pane, cx);
        page
    }

    // Builds a pane (optionally rooted at `cwd`), replaying the active `[ui]`
    // config so it matches the user's sort/hidden/icon settings.
    fn build_pane(
        &self,
        cwd: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ExplorerPane> {
        let search_service = self.search_service.clone();
        let pane = cx.new(|cx| ExplorerPane::build(search_service, window, cx));
        let ui = self.ui.clone();
        pane.update(cx, |pane, cx| {
            pane.apply_config_ui(&ui, cx);
            if let Some(cwd) = cwd {
                pane.cwd = cwd;
                // Force a reload of the new root on the next render.
                pane.loaded = false;
            }
        });
        pane
    }

    // Tracks a freshly-built pane: subscribes to its navigation events and records
    // a weak handle. The dock holds the strong reference.
    fn register_pane(&mut self, pane: &Entity<ExplorerPane>, cx: &mut Context<Self>) {
        let subscription = cx.subscribe(pane, Self::on_pane_event);
        self.panes.push(PaneEntry {
            pane: pane.downgrade(),
            _subscription: subscription,
        });
    }

    // Drops registry entries (and their subscriptions) for panes the dock has
    // destroyed via tab drag-and-drop or close.
    fn prune_dead_panes(&mut self) {
        self.panes.retain(|entry| entry.pane.upgrade().is_some());
    }

    // Every live pane, in registration order.
    fn live_panes(&self) -> Vec<Entity<ExplorerPane>> {
        self.panes
            .iter()
            .filter_map(|entry| entry.pane.upgrade())
            .collect()
    }

    // The pane the shared sidebar acts on: the dock's active panel, falling back
    // to the first live pane.
    fn active_pane(&self, cx: &App) -> Option<Entity<ExplorerPane>> {
        let panes = self.live_panes();
        panes
            .iter()
            .find(|pane| pane.read(cx).active_in_dock)
            .cloned()
            .or_else(|| panes.first().cloned())
    }

    // The tab panel split/new-tab operations target: the first tab panel in the
    // dock tree, falling back to the original root.
    fn primary_tab_panel(&self, cx: &App) -> Entity<TabPanel> {
        fn walk(item: &DockItem) -> Option<Entity<TabPanel>> {
            match item {
                DockItem::Tabs { view, .. } => Some(view.clone()),
                DockItem::Split { items, .. } => items.iter().find_map(walk),
                _ => None,
            }
        }
        walk(self.dock_area.read(cx).items()).unwrap_or_else(|| self.root_tab_panel.clone())
    }

    // Prunes the registry when the dock layout changes (e.g. a tab was dragged
    // away and its pane destroyed).
    fn on_dock_event(
        &mut self,
        _dock_area: Entity<DockArea>,
        event: &DockEvent,
        cx: &mut Context<Self>,
    ) {
        if let DockEvent::LayoutChanged = event {
            let before = self.panes.len();
            self.prune_dead_panes();
            if self.panes.len() != before {
                cx.notify();
            }
        }
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
            .live_panes()
            .into_iter()
            .filter(|pane| pane.entity_id() != source.entity_id())
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

    /// Opens a new pane rooted at the active pane's directory, split off in
    /// `direction` (left/right for vertical, top/bottom for horizontal).
    pub fn split(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cwd = self.active_pane(cx).map(|pane| pane.read(cx).cwd.clone());
        let placement = match direction {
            SplitDirection::Vertical => Placement::Right,
            SplitDirection::Horizontal => Placement::Bottom,
        };
        let pane = self.build_pane(cwd, window, cx);
        self.register_pane(&pane, cx);
        self.split_center(pane, placement, window, cx);
    }

    // Opens a directory dropped onto the dock: as a new tab (center) or a new
    // split on the hovered edge.
    fn open_directory(
        &mut self,
        path: String,
        zone: DropZone,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.build_pane(Some(path), window, cx);
        self.register_pane(&pane, cx);
        match zone.placement() {
            Some(placement) => self.split_center(pane, placement, window, cx),
            None => {
                let target = self.primary_tab_panel(cx);
                target.update(cx, |tab_panel, cx| {
                    tab_panel.add_panel(Arc::new(pane) as Arc<dyn PanelView>, window, cx);
                });
                cx.notify();
            }
        }
    }

    // Splits the whole dock center, placing `pane` (in its own tab panel) on the
    // given edge of the existing layout. Done synchronously by rebuilding the
    // center `DockItem` tree, rather than the dock's deferred `add_panel_at`, so
    // the new pane is attached immediately (and split behaviour is deterministic
    // under test).
    fn split_center(
        &mut self,
        pane: Entity<ExplorerPane>,
        placement: Placement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dock_area.update(cx, |area, cx| {
            let weak = cx.entity().downgrade();
            let existing = area.items().clone();
            let new_item = DockItem::tabs(
                vec![Arc::new(pane) as Arc<dyn PanelView>],
                Some(0),
                &weak,
                window,
                cx,
            );
            let axis = match placement {
                Placement::Left | Placement::Right => Axis::Horizontal,
                Placement::Top | Placement::Bottom => Axis::Vertical,
            };
            let items = match placement {
                Placement::Left | Placement::Top => vec![new_item, existing],
                Placement::Right | Placement::Bottom => vec![existing, new_item],
            };
            let split = DockItem::split(axis, items, &weak, window, cx);
            area.set_center(split, window, cx);
        });
        cx.notify();
    }

    /// Closes the live pane at `index`, keeping at least one open.
    pub fn close_pane(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let live = self.live_panes();
        if live.len() <= 1 {
            return;
        }
        let Some(pane) = live.get(index).cloned() else {
            return;
        };
        let target_id = pane.entity_id();
        self.dock_area.update(cx, |area, cx| {
            area.remove_panel(
                Arc::new(pane) as Arc<dyn PanelView>,
                DockPlacement::Center,
                window,
                cx,
            );
        });
        // Drop the closed pane's registry entry (and any already-dead entries)
        // explicitly: the dock keeps extra `Arc` copies of a panel, so the weak
        // handle may still upgrade for a while and we cannot rely on the pane being
        // released to prune it.
        self.panes.retain(|entry| {
            entry
                .pane
                .upgrade()
                .is_some_and(|pane| pane.entity_id() != target_id)
        });
        cx.notify();
    }

    // Focuses the live pane at `index`, if any.
    pub(crate) fn focus_pane(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let panes = self.live_panes();
        if let Some(pane) = panes.get(index) {
            pane.read(cx).focus_handle(cx).focus(window);
            cx.notify();
        }
    }

    // Index of the active pane within the live registry order.
    fn active_index(&self, cx: &App) -> usize {
        self.live_panes()
            .iter()
            .position(|pane| pane.read(cx).active_in_dock)
            .unwrap_or(0)
    }

    fn focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.live_panes().len();
        if count < 2 {
            return;
        }
        let next = (self.active_index(cx) + 1) % count;
        self.focus_pane(next, window, cx);
    }

    fn focus_prev(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.live_panes().len();
        if count < 2 {
            return;
        }
        let prev = (self.active_index(cx) + count - 1) % count;
        self.focus_pane(prev, window, cx);
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    // Navigates the active pane to `path` (used by the shared sidebar).
    fn navigate_active(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane(cx) {
            pane.update(cx, |pane, cx| pane.change_dir(path, window, cx));
        }
    }

    /// Applies the `[ui]` config section to every pane (§5 of `config.md`).
    pub fn apply_config_ui(&mut self, ui: &Ui, cx: &mut Context<Self>) {
        self.ui = ui.clone();
        for pane in self.live_panes() {
            pane.update(cx, |pane, cx| pane.apply_config_ui(ui, cx));
        }
    }

    /// Applies the `[explorer]` config section: the synced-panes opt-in. Enabling
    /// sync immediately mirrors the active pane's directory into the others. The
    /// configured split orientation no longer applies a layout — splits are driven
    /// by the split shortcuts and drag-and-drop.
    pub fn apply_config_explorer(&mut self, explorer: &ExplorerConfig, cx: &mut Context<Self>) {
        let enabling = explorer.synced_panes && !self.synced_panes;
        self.synced_panes = explorer.synced_panes;
        if enabling {
            if let Some(active) = self.active_pane(cx) {
                let path = active.read(cx).cwd.clone();
                let active_id = active.entity_id();
                for pane in self.live_panes() {
                    if pane.entity_id() != active_id {
                        let path = path.clone();
                        pane.update(cx, |pane, cx| pane.navigate_to_synced(path, cx));
                    }
                }
            }
        }
        cx.notify();
    }

    /// Footer status for the active pane (a config error in `RootView` still takes
    /// precedence over this).
    pub fn status_for_footer(&self, cx: &App) -> Option<(String, bool)> {
        self.active_pane(cx)?.read(cx).status_for_footer()
    }

    // The shared quick-access sidebar: clicking a shortcut navigates the active
    // pane; dragging one onto the dock opens it as a new tab/split.
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut shortcuts = div().flex().flex_col().gap_1().px(px(8.0));
        for (index, (label, path)) in sidebar_shortcuts().into_iter().enumerate() {
            let click_path = path.clone();
            let drag_path = path.clone();
            shortcuts = shortcuts.child(
                div()
                    .id(("sidebar-shortcut", index))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(theme::BG_HOVER)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_active(click_path.clone(), window, cx)
                    }))
                    // Dragging a shortcut onto the dock opens it as a new tab/split.
                    .on_drag(
                        ExplorerDrag {
                            path: drag_path.clone(),
                        },
                        |drag, _, _, cx| cx.new(|_| drag.clone()),
                    )
                    .child(
                        Icon::new(IconName::Folder)
                            .size_4()
                            .text_color(rgb(theme::GRAY_600)),
                    )
                    .child(div().text_sm().text_color(rgb(theme::FG)).child(label)),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_r_1()
            .border_color(rgb(theme::BORDER))
            .bg(rgb(theme::BG))
            .py(px(16.0))
            .child(
                div()
                    .px(px(12.0))
                    .pb(px(8.0))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme::FG_SECONDARY))
                    .child("Locations"),
            )
            .child(shortcuts)
    }

    // The translucent half-pane (or full) band previewing where a dropped
    // directory will land.
    fn render_drop_phantom(&self, zone: DropZone) -> impl IntoElement {
        let band = div()
            .absolute()
            .bg(rgba(0x60a5_fa55))
            .border_2()
            .border_color(rgb(theme::ACCENT));
        match zone {
            DropZone::Center => band.top_0().left_0().size_full(),
            DropZone::Left => band.top_0().left_0().bottom_0().w(relative(0.5)),
            DropZone::Right => band.top_0().right_0().bottom_0().w(relative(0.5)),
            DropZone::Top => band.top_0().left_0().right_0().h(relative(0.5)),
            DropZone::Bottom => band.bottom_0().left_0().right_0().h(relative(0.5)),
        }
    }
}

impl Render for ExplorerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_visible = self.sidebar_visible;
        // Only preview a drop while a directory drag is actually in flight; a stale
        // zone from a drag that ended elsewhere must not keep showing.
        let drop_zone = if cx.has_active_drag() {
            self.drop_zone
        } else {
            None
        };
        let sidebar = sidebar_visible.then(|| self.render_sidebar(cx));
        let phantom = drop_zone.map(|zone| self.render_drop_phantom(zone));

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
                this.focus_pane(0, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPane2, window, cx| {
                this.focus_pane(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusNextPane, window, cx| {
                this.focus_next(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPrevPane, window, cx| {
                this.focus_prev(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| {
                this.toggle_sidebar(cx);
            }))
            .child(
                h_resizable("explorer-shell", self.shell_resizable.clone())
                    .child(
                        // Keep the sidebar panel in the resizable's child list even
                        // when hidden (toggle via `.visible`) so the persisted panel
                        // sizes stay stable; dropping the child would make the dock
                        // inherit the sidebar's slot.
                        resizable_panel()
                            .size(px(220.0))
                            .size_range(px(180.0)..px(360.0))
                            .visible(sidebar_visible)
                            .when_some(sidebar, |panel, sidebar| panel.child(sidebar)),
                    )
                    .child(
                        resizable_panel().child(
                            div()
                                .id("explorer-dock-region")
                                .relative()
                                .size_full()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                // A directory drag (sidebar shortcut or folder row)
                                // carries `ExplorerDrag`; the dock's own tab DnD uses
                                // a different payload, so the handlers coexist.
                                .on_drag_move(cx.listener(
                                    |this, event: &DragMoveEvent<ExplorerDrag>, _window, cx| {
                                        let zone =
                                            drop_zone_for(event.bounds, event.event.position);
                                        if this.drop_zone != Some(zone) {
                                            this.drop_zone = Some(zone);
                                            cx.notify();
                                        }
                                    },
                                ))
                                .on_drop(cx.listener(|this, drag: &ExplorerDrag, window, cx| {
                                    let zone = this.drop_zone.take().unwrap_or(DropZone::Center);
                                    this.open_directory(drag.path.clone(), zone, window, cx);
                                }))
                                .child(self.dock_area.clone())
                                .when_some(phantom, |this, phantom| this.child(phantom)),
                        ),
                    ),
            )
    }
}

impl crate::Page for ExplorerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}

// Quick-access locations for the shared sidebar: $HOME plus the common XDG
// directories that exist.
fn sidebar_shortcuts() -> Vec<(String, String)> {
    let mut shortcuts = Vec::new();
    let home = std::env::var("HOME").ok();
    #[cfg(target_os = "windows")]
    let home = home.or_else(|| std::env::var("USERPROFILE").ok());
    if let Some(home) = home {
        let join = |sub: &str| {
            std::path::Path::new(&home)
                .join(sub)
                .to_string_lossy()
                .to_string()
        };
        shortcuts.push(("Home".to_string(), home.clone()));
        for sub in ["Desktop", "Downloads", "Documents", "Pictures"] {
            let path = join(sub);
            if std::path::Path::new(&path).exists() {
                shortcuts.push((sub.to_string(), path));
            }
        }
    }
    shortcuts
}

#[cfg(test)]
impl ExplorerPage {
    pub(crate) fn pane_count(&self) -> usize {
        self.live_panes().len()
    }

    pub(crate) fn is_synced(&self) -> bool {
        self.synced_panes
    }

    pub(crate) fn sidebar_is_visible(&self) -> bool {
        self.sidebar_visible
    }

    pub(crate) fn pane(&self, index: usize) -> Entity<ExplorerPane> {
        self.live_panes()
            .get(index)
            .cloned()
            .expect("test requested an out-of-range pane")
    }

    pub(crate) fn pane_cwd(&self, index: usize, cx: &App) -> Option<String> {
        self.live_panes()
            .get(index)
            .map(|pane| pane.read(cx).cwd.clone())
    }

    /// Number of live per-pane subscriptions; used to assert the registry is
    /// pruned in step with the panes the dock holds.
    pub(crate) fn subscription_count(&self) -> usize {
        self.panes.len()
    }
}
