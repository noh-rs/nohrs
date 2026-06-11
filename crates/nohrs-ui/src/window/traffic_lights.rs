use gpui::{Pixels, Point, TitlebarOptions, WindowOptions, point, px};

const DEFAULT_HORIZONTAL_OFFSET: Pixels = px(12.0);
const DEFAULT_BUTTON_SIZE: Pixels = px(16.0);
const HIDDEN_HORIZONTAL_OFFSET: Pixels = px(-200.0);

/// Provides a simple API to control the macOS traffic light buttons when using a unified toolbar.
/// The configuration is applied during window creation via [`TrafficLightsHook::apply`].
#[derive(Clone)]
pub struct TrafficLightsHook {
    horizontal_offset: Pixels,
    toolbar_height: Pixels,
    button_size: Pixels,
    visible: bool,
}

impl Default for TrafficLightsHook {
    fn default() -> Self {
        Self {
            horizontal_offset: DEFAULT_HORIZONTAL_OFFSET,
            toolbar_height: px(34.0),
            button_size: DEFAULT_BUTTON_SIZE,
            visible: true,
        }
    }
}

impl TrafficLightsHook {
    /// Create a new hook with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the horizontal offset where the traffic lights should appear.
    pub fn with_horizontal_offset(mut self, offset: Pixels) -> Self {
        self.horizontal_offset = offset;
        self
    }

    /// Vertically center the traffic lights using the given unified toolbar height.
    pub fn center_vertically(mut self, toolbar_height: Pixels) -> Self {
        self.toolbar_height = toolbar_height;
        self
    }

    /// Toggle visibility of the traffic lights. When hidden, the buttons are moved outside of the
    /// window bounds.
    pub fn set_visibility(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Apply the configuration to the supplied [`WindowOptions`].
    pub fn apply(&self, options: &mut WindowOptions) {
        let titlebar = options
            .titlebar
            .get_or_insert_with(TitlebarOptions::default);
        titlebar.appears_transparent = true;
        titlebar.traffic_light_position = Some(self.position());
    }

    fn position(&self) -> Point<Pixels> {
        let vertical_offset = self.vertical_offset();
        if self.visible {
            point(self.horizontal_offset, vertical_offset)
        } else {
            point(HIDDEN_HORIZONTAL_OFFSET, vertical_offset)
        }
    }

    fn vertical_offset(&self) -> Pixels {
        let toolbar_height = f32::from(self.toolbar_height);
        let button_size = f32::from(self.button_size);
        let offset = (toolbar_height - button_size).max(0.0) / 2.0;
        px(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_centers_within_the_toolbar() {
        // Defaults: toolbar 34, button 16 → (34 - 16) / 2 = 9.
        assert_eq!(TrafficLightsHook::new().vertical_offset(), px(9.0));
    }

    #[test]
    fn vertical_offset_clamps_when_button_exceeds_toolbar() {
        let centered = TrafficLightsHook::new().center_vertically(px(16.0));
        assert_eq!(centered.vertical_offset(), px(0.0));
        let clamped = TrafficLightsHook::new().center_vertically(px(8.0));
        assert_eq!(clamped.vertical_offset(), px(0.0));
    }

    #[test]
    fn builders_update_configuration() {
        let hook = TrafficLightsHook::new()
            .with_horizontal_offset(px(20.0))
            .set_visibility(false);
        assert_eq!(hook.horizontal_offset, px(20.0));
        assert!(!hook.visible);
    }
}
