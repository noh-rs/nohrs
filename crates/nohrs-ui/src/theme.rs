/// Modern theme tokens inspired by macOS and modern file explorers
// Consumers import this as `theme::theme` (e.g. `use nohrs_ui::theme::theme`);
// the inner module name is part of the established public path.
#[allow(clippy::module_inception)]
pub mod theme {
    // Base colors
    /// Pure white.
    pub const WHITE: u32 = 0xFFFFFF;
    /// Pure black.
    pub const BLACK: u32 = 0x000000;

    // Gray scale
    /// Lightest gray, used for the lightest background.
    pub const GRAY_50: u32 = 0xF9FAFB; // Lightest background
    /// Light gray, used for light backgrounds.
    pub const GRAY_100: u32 = 0xF3F4F6; // Light background
    /// Gray used for default borders.
    pub const GRAY_200: u32 = 0xE5E7EB; // Border
    /// Gray used for borders on hover.
    pub const GRAY_300: u32 = 0xD1D5DB; // Border hover
    /// Gray used for muted text.
    pub const GRAY_400: u32 = 0x9CA3AF; // Muted text
    /// Gray used for secondary text.
    pub const GRAY_500: u32 = 0x6B7280; // Secondary text
    /// Gray used for primary text.
    pub const GRAY_600: u32 = 0x4B5563; // Primary text
    /// Dark gray used for high-emphasis text.
    pub const GRAY_700: u32 = 0x374151; // Dark text
    /// Darker gray, used for darker backgrounds.
    pub const GRAY_800: u32 = 0x1F2937; // Darker background
    /// Darkest gray, used for the darkest background (e.g. toolbar).
    pub const GRAY_900: u32 = 0x111827; // Darkest background (toolbar)

    // Main background and text
    /// Primary surface background color.
    pub const BG: u32 = WHITE;
    /// Secondary surface background color.
    pub const BG_SECONDARY: u32 = GRAY_50;
    /// Background color for hovered surfaces.
    pub const BG_HOVER: u32 = GRAY_100;
    /// Primary foreground (text) color.
    pub const FG: u32 = GRAY_900;
    /// Secondary foreground (text) color for less prominent content.
    pub const FG_SECONDARY: u32 = GRAY_500;
    /// Muted foreground color for de-emphasized content.
    pub const MUTED: u32 = GRAY_400;

    // UI elements
    /// Default border color.
    pub const BORDER: u32 = GRAY_200;
    /// Border color on hover.
    pub const BORDER_HOVER: u32 = GRAY_300;

    // Chrome surfaces (nav rail, tab bar, status bar) share one tone so the
    // window reads as a frame around the white content areas.
    /// Toolbar background color.
    pub const TOOLBAR_BG: u32 = GRAY_100; // Light gray background
    /// Toolbar item background color on hover.
    pub const TOOLBAR_HOVER: u32 = GRAY_200; // Slightly darker on hover
    /// Toolbar text color.
    pub const TOOLBAR_TEXT: u32 = GRAY_600; // Dark gray text
    /// Background color of the active toolbar item.
    pub const TOOLBAR_ACTIVE_BG: u32 = WHITE; // White background for active item
    /// Text color of the active toolbar item.
    pub const TOOLBAR_ACTIVE_TEXT: u32 = ACCENT;
    /// Toolbar border color.
    pub const TOOLBAR_BORDER: u32 = GRAY_200; // Border color
    /// Status bar background; matches the other chrome surfaces.
    pub const FOOTER_BG: u32 = TOOLBAR_BG;

    // Accent ramp, derived from the burnt orange in the app icon. `ACCENT` is
    // dark enough to carry `ACCENT_FG` text at WCAG AA (4.9:1), which the
    // filled controls (search button, active scope chips) rely on.
    /// Primary accent color.
    pub const ACCENT: u32 = 0xB85214;
    /// Accent color on hover / pressed.
    pub const ACCENT_HOVER: u32 = 0x93400F;
    /// Light accent, for borders bounding an accent-tinted surface.
    pub const ACCENT_LIGHT: u32 = 0xF0C3A1;
    /// Faint accent tint, for selected rows and active chip backgrounds.
    pub const ACCENT_SUBTLE: u32 = 0xFDF1E9;
    /// Text and icon color on top of an `ACCENT`-filled surface.
    pub const ACCENT_FG: u32 = WHITE;

    // Status colors
    /// Danger color, used for error messages and destructive actions.
    pub const DANGER: u32 = 0xDC2626; // Red, for error messages

    /// Backdrop behind an image preview; deliberately dark so transparent and
    /// light-colored images stay legible.
    pub const PREVIEW_BACKDROP: u32 = GRAY_800;

    /// Opacity of the scrim painted behind a modal dialog. `gpui_component`
    /// ships 5%, which is invisible against this theme's white surfaces and
    /// leaves confirmations looking like floating panels rather than modals.
    pub const MODAL_SCRIM_ALPHA: f32 = 0.40;
}
