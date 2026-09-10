//! The launcher window: how it is sized, where it appears, and how it toggles.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gpui::{
    App, AppContext, Bounds, Global, Pixels, Point, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, point, px, size,
};
use nohrs_core::telemetry::LogErr;
use nohrs_services::search::file_index::FileNameIndex;

use crate::view::LauncherView;

/// Window width (docs/launcher.md §1). Fixed rather than resizable: a search
/// field gains nothing from being dragged wider, and a known width keeps the
/// row layout predictable.
pub const LAUNCHER_WIDTH: f32 = 750.0;

/// Window height.
pub const LAUNCHER_HEIGHT: f32 = 500.0;

/// How far down the display the window's top edge sits. Centring vertically
/// puts a mostly-empty panel below the eye's resting line; a quarter of the way
/// down is where Spotlight and Raycast both land.
const TOP_FRACTION: f32 = 0.25;

/// Tracks the open launcher window so that summoning it again toggles it closed
/// (docs/launcher.md §1) rather than stacking a second window.
#[derive(Default)]
struct OpenLauncher(Option<WindowHandle<LauncherView>>);

impl Global for OpenLauncher {}

/// Where the launcher window opens: horizontally centred on the primary
/// display, a quarter of the way down it.
pub fn launcher_bounds(cx: &mut App) -> Bounds<Pixels> {
    let window_size = size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT));
    let Some(display) = cx.primary_display() else {
        // Without a display to measure, centring is the best available guess.
        return Bounds::centered(None, window_size, cx);
    };

    let screen = display.bounds();
    let origin: Point<Pixels> = point(
        screen.origin.x + (screen.size.width - window_size.width) / 2.0,
        screen.origin.y + screen.size.height * TOP_FRACTION,
    );
    Bounds {
        origin,
        size: window_size,
    }
}

/// Window options for the launcher: borderless, floating, and fixed-size.
///
/// The background is transparent so the panel can be a rounded rectangle rather
/// than fill a square window (docs/launcher.md §1). Nothing between the window
/// and [`LauncherView`] paints, which is why the launcher owns its search field
/// instead of taking one that requires `gpui_component::Root` at the first layer
/// — `Root` paints an opaque background over the whole window, corners included.
///
/// Where there is no compositor to blend against, a transparent window falls
/// back to whatever is behind it; the panel itself is opaque either way, so the
/// worst case is square corners rather than an unreadable window.
pub fn launcher_window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        // No titlebar: the search field is the window's only chrome.
        titlebar: None,
        focus: true,
        show: true,
        // Above ordinary windows — it is summoned over whatever is in front.
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        display_id: None,
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        app_id: None,
        window_min_size: None,
        window_decorations: None,
        tabbing_identifier: None,
    }
}

/// Opens a launcher window over `index`.
///
/// `home` is the directory the index was built from; see
/// [`crate::view::LauncherView::new`].
pub fn open_launcher(
    index: Arc<FileNameIndex>,
    home: Option<PathBuf>,
    cx: &mut App,
) -> Result<WindowHandle<LauncherView>> {
    let bounds = launcher_bounds(cx);
    let handle = cx.open_window(launcher_window_options(bounds), move |window, cx| {
        cx.new(|cx| LauncherView::new(index, home, window, cx))
    })?;
    Ok(handle)
}

/// Opens the launcher, or closes it if it is already open.
///
/// This is what a hotkey binds to: pressing it twice should leave the screen as
/// it was found rather than leaving a window behind.
pub fn toggle_launcher(
    index: Arc<FileNameIndex>,
    home: Option<PathBuf>,
    cx: &mut App,
) -> Result<()> {
    if let Some(handle) = open_window(cx) {
        cx.set_global(OpenLauncher(None));
        // The close is deferred because the summon key is handled while another
        // window is mid-update, and gpui will not let a second window be updated
        // from inside the first. Running it once the dispatch unwinds does.
        cx.defer(move |cx| {
            handle
                .update(cx, |_, window, _| window.remove_window())
                .log_err();
        });
        return Ok(());
    }

    let handle = open_launcher(index, home, cx)?;
    cx.set_global(OpenLauncher(Some(handle)));
    Ok(())
}

/// A window that exists only so the process keeps running.
///
/// GPUI's Linux backend stops its event loop the moment the last window closes
/// (`windows.is_empty()` in its X11 client), so a launcher that lives on a
/// global hotkey would quit the first time it was dismissed — the hotkey would
/// work exactly once. One window that is never closed keeps the loop alive.
/// macOS keeps the application running without any windows, so it needs none.
///
/// It renders nothing, is one pixel, and is a pop-up (`_NET_WM_WINDOW_TYPE_
/// _NOTIFICATION` on X11), which keeps it out of taskbars and window switchers.
#[cfg(not(target_os = "macos"))]
struct KeepAlive;

#[cfg(not(target_os = "macos"))]
impl gpui::Render for KeepAlive {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}

/// Nothing to do: a macOS application outlives its windows on its own.
///
/// Compile-gated rather than a runtime branch so neither platform carries the
/// other's dead code.
#[cfg(target_os = "macos")]
pub fn open_keep_alive_window(_cx: &mut App) -> Result<()> {
    Ok(())
}

/// Opens the keep-alive window described by [`KeepAlive`].
///
/// Only daemon-style runs need this: when the explorer is open, its own window
/// already holds the loop.
#[cfg(not(target_os = "macos"))]
pub fn open_keep_alive_window(cx: &mut App) -> Result<()> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1.0), px(1.0)),
        })),
        titlebar: None,
        // Taking focus would pull it away from whatever the user is doing.
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: None,
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        app_id: None,
        window_min_size: None,
        window_decorations: None,
        tabbing_identifier: None,
    };

    cx.open_window(options, |_window, cx| cx.new(|_cx| KeepAlive))?;
    Ok(())
}

/// The launcher window, if one is actually open.
///
/// The remembered handle outlives the window it names — `escape` and opening a
/// result both close the window without going through [`toggle_launcher`] — so
/// it is checked against the live window list rather than trusted. Treating a
/// stale handle as an open window would make the next summon do nothing.
fn open_window(cx: &App) -> Option<WindowHandle<LauncherView>> {
    let handle = cx.try_global::<OpenLauncher>().and_then(|open| open.0)?;
    cx.windows()
        .iter()
        .any(|window| window.window_id() == handle.window_id())
        .then_some(handle)
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    /// A launcher over an index with nothing in it: these tests are about the
    /// window's lifetime, not about what it searches.
    fn summon(cx: &mut TestAppContext) -> Result<()> {
        cx.update(|cx| toggle_launcher(Arc::new(FileNameIndex::new()), None, cx))
    }

    /// How many windows the application is holding open.
    fn window_count(cx: &mut TestAppContext) -> usize {
        cx.update(|cx| cx.windows().len())
    }

    /// The globals [`LauncherView`] reads its colours and key bindings from.
    fn init(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(crate::field::init);
    }

    #[test]
    fn window_options_are_a_fixed_borderless_panel() {
        let bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT)),
        };
        let options = launcher_window_options(bounds);

        assert!(options.titlebar.is_none());
        assert!(!options.is_resizable);
        assert!(!options.is_minimizable);
        assert!(options.focus);
        assert!(matches!(options.kind, WindowKind::PopUp));
        assert!(matches!(
            options.window_bounds,
            Some(WindowBounds::Windowed(_))
        ));
    }

    #[gpui::test]
    fn the_window_sits_high_on_the_primary_display(cx: &mut TestAppContext) {
        let (bounds, screen) = cx.update(|cx| {
            let screen = cx.primary_display().map(|display| display.bounds());
            (launcher_bounds(cx), screen)
        });
        let screen = screen.expect("the test platform has a display");

        assert_eq!(bounds.size.width, px(LAUNCHER_WIDTH));
        assert_eq!(bounds.size.height, px(LAUNCHER_HEIGHT));
        // Centred across, and above the middle: a panel centred vertically sits
        // below where the eye rests.
        assert_eq!(bounds.center().x, screen.origin.x + screen.size.width / 2.0);
        assert!(
            bounds.origin.y < screen.origin.y + screen.size.height / 2.0,
            "{bounds:?} is not in the upper half of {screen:?}"
        );
    }

    #[gpui::test]
    fn summoning_twice_leaves_no_window_behind(cx: &mut TestAppContext) {
        init(cx);

        summon(cx).expect("the launcher should open");
        assert_eq!(window_count(cx), 1);

        summon(cx).expect("the launcher should close");
        // The close is deferred until the dispatch that asked for it unwinds,
        // so it has not happened yet.
        cx.run_until_parked();
        assert_eq!(
            window_count(cx),
            0,
            "the second summon should have closed it"
        );

        summon(cx).expect("the launcher should open again");
        cx.run_until_parked();
        assert_eq!(window_count(cx), 1);
    }

    #[gpui::test]
    fn a_window_closed_behind_our_back_still_reopens(cx: &mut TestAppContext) {
        init(cx);
        summon(cx).expect("the launcher should open");

        // What `escape` and opening a result both do: close the window without
        // going through `toggle_launcher`, leaving the remembered handle stale.
        let handle = cx
            .update(|cx| open_window(cx))
            .expect("the launcher window should be remembered");
        handle
            .update(cx, |_, window, _| window.remove_window())
            .expect("the launcher window should still be open");
        cx.run_until_parked();
        assert_eq!(window_count(cx), 0);

        // A stale handle taken at face value would make this summon do nothing.
        summon(cx).expect("the launcher should open");
        cx.run_until_parked();
        assert_eq!(window_count(cx), 1);
    }

    #[cfg(not(target_os = "macos"))]
    #[gpui::test]
    fn the_keep_alive_window_holds_the_event_loop_open(cx: &mut TestAppContext) {
        cx.update(open_keep_alive_window)
            .expect("the keep-alive window should open");
        assert_eq!(window_count(cx), 1);
    }
}
