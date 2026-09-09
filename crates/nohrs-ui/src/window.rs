/// Platform traffic-light (window control button) configuration and hooks.
pub mod traffic_lights;

use gpui::{Bounds, Pixels, WindowBounds, WindowOptions};

use self::traffic_lights::TrafficLightsHook;

/// Construct window options that enable the unified toolbar styling and apply the provided
/// traffic light configuration.
pub fn unified_window_options(
    bounds: Bounds<Pixels>,
    traffic_lights: &TrafficLightsHook,
) -> WindowOptions {
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..Default::default()
    };

    traffic_lights.apply(&mut options);
    options
}
