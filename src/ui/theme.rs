#![cfg(feature = "gui")]

/// Modern theme tokens inspired by macOS and modern file explorers
pub mod theme {
    // Base colors
    pub const WHITE: u32 = 0xFFFFFF;
    pub const BLACK: u32 = 0x000000;

    // Gray scale
    pub const GRAY_50: u32 = 0xF9FAFB;   // Lightest background
    pub const GRAY_100: u32 = 0xF3F4F6;  // Light background
    pub const GRAY_200: u32 = 0xE5E7EB;  // Border
    pub const GRAY_300: u32 = 0xD1D5DB;  // Border hover
    pub const GRAY_400: u32 = 0x9CA3AF;  // Muted text
    pub const GRAY_500: u32 = 0x6B7280;  // Secondary text
    pub const GRAY_600: u32 = 0x4B5563;  // Primary text
    pub const GRAY_700: u32 = 0x374151;  // Dark text
    pub const GRAY_800: u32 = 0x1F2937;  // Darker background
    pub const GRAY_900: u32 = 0x111827;  // Darkest background (toolbar)

    // Main background and text
    pub const BG: u32 = WHITE;
    pub const BG_SECONDARY: u32 = GRAY_50;
    pub const BG_HOVER: u32 = GRAY_100;
    pub const FG: u32 = GRAY_900;
    pub const FG_SECONDARY: u32 = GRAY_500;
    pub const MUTED: u32 = GRAY_400;

    // UI elements
    pub const BORDER: u32 = GRAY_200;
    pub const BORDER_HOVER: u32 = GRAY_300;

    // Toolbar (left side) - VSCode Light theme style
    pub const TOOLBAR_BG: u32 = GRAY_100;           // Light gray background
    pub const TOOLBAR_HOVER: u32 = GRAY_200;        // Slightly darker on hover
    pub const TOOLBAR_TEXT: u32 = GRAY_600;         // Dark gray text
    pub const TOOLBAR_ACTIVE_BG: u32 = WHITE;       // White background for active item
    pub const TOOLBAR_ACTIVE_TEXT: u32 = ACCENT;    // Blue text for active item
    pub const TOOLBAR_BORDER: u32 = GRAY_200;       // Border color

    // Accent colors
    pub const ACCENT: u32 = 0x3B82F6;      // Blue
    pub const ACCENT_HOVER: u32 = 0x2563EB;
    pub const ACCENT_LIGHT: u32 = 0xDCEEFF;
}
