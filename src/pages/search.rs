use crate::ui::theme::theme;
use gpui::{
    div, prelude::*, px, rgb, AnyElement, Context, ElementId, Entity, FocusHandle, Focusable,
    InteractiveElement, Render, Rgba, SharedString, Window,
};
use gpui_component::input::{InputState, TextInput};
use gpui_component::resizable::{h_resizable, resizable_panel, ResizableState};
use gpui_component::Icon;
use std::collections::HashSet;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

// ============================================================================
// Data Structures
// ============================================================================

/// A single match within a file
#[derive(Clone)]
pub struct SearchMatch {
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

/// A file with its search matches
#[derive(Clone)]
pub struct SearchFileResult {
    pub path: String,
    pub folder: String,
    pub filename: String,
    pub matches: Vec<SearchMatch>,
}

// ============================================================================
// SearchPage Component
// ============================================================================

pub struct SearchPage {
    focus_handle: FocusHandle,
    resizable: Entity<ResizableState>,
    search_input: Entity<InputState>,

    // Search filters
    match_case: bool,
    match_whole_word: bool,
    use_regex: bool,

    // Search results
    search_results: Vec<SearchFileResult>,
    expanded_folders: HashSet<String>,
    expanded_files: HashSet<usize>,
    selected_file: Option<usize>,
    selected_match: Option<(usize, usize)>, // (file_idx, match_idx)

    // Preview
    preview_content: Option<String>,
    preview_path: Option<String>,

    // Syntax Highlighting
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl SearchPage {
    pub fn new(
        resizable: Entity<ResizableState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx));

        // Initialize with dummy data
        let search_results = Self::create_dummy_results();
        let mut expanded_files = HashSet::new();
        let mut expanded_folders = HashSet::new();
        if !search_results.is_empty() {
            expanded_files.insert(0);
            // Expand first folder by default
            if let Some(first) = search_results.first() {
                expanded_folders.insert(first.folder.clone());
            }
        }

        // Set initial preview based on first result
        let (preview_content, preview_path) = if let Some(first) = search_results.first() {
            (
                Some(Self::generate_dummy_file_content(&first.path)),
                Some(first.path.clone()),
            )
        } else {
            (None, None)
        };

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        Self {
            focus_handle: cx.focus_handle(),
            resizable,
            search_input,
            match_case: false,
            match_whole_word: false,
            use_regex: false,
            search_results,
            expanded_folders,
            expanded_files,
            selected_file: Some(0),
            selected_match: None,
            preview_content,
            preview_path,
            syntax_set,
            theme_set,
        }
    }

    fn create_dummy_results() -> Vec<SearchFileResult> {
        vec![
            // src folder
            SearchFileResult {
                path: "/Users/shuya/project/src/main.rs".into(),
                folder: "src".into(),
                filename: "main.rs".into(),
                matches: vec![
                    SearchMatch {
                        line_number: 15,
                        line_content: "fn main() {".into(),
                        match_start: 3,
                        match_end: 7,
                    },
                    SearchMatch {
                        line_number: 42,
                        line_content: "    println!(\"Hello, main world!\");".into(),
                        match_start: 22,
                        match_end: 26,
                    },
                ],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/lib.rs".into(),
                folder: "src".into(),
                filename: "lib.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 8,
                    line_content: "pub mod main_utils;".into(),
                    match_start: 8,
                    match_end: 12,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/app.rs".into(),
                folder: "src".into(),
                filename: "app.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 25,
                    line_content: "impl App { fn main_loop() {} }".into(),
                    match_start: 18,
                    match_end: 22,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/config.rs".into(),
                folder: "src".into(),
                filename: "config.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 12,
                    line_content: "// Main configuration".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/error.rs".into(),
                folder: "src".into(),
                filename: "error.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 5,
                    line_content: "pub enum MainError {".into(),
                    match_start: 9,
                    match_end: 13,
                }],
            },
            // src/utils folder
            SearchFileResult {
                path: "/Users/shuya/project/src/utils/helper.rs".into(),
                folder: "src/utils".into(),
                filename: "helper.rs".into(),
                matches: vec![
                    SearchMatch {
                        line_number: 23,
                        line_content: "/// Main helper function".into(),
                        match_start: 4,
                        match_end: 8,
                    },
                    SearchMatch {
                        line_number: 24,
                        line_content: "pub fn main_helper(input: &str) -> String {".into(),
                        match_start: 7,
                        match_end: 11,
                    },
                ],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/utils/parser.rs".into(),
                folder: "src/utils".into(),
                filename: "parser.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 18,
                    line_content: "fn parse_main_config() {}".into(),
                    match_start: 9,
                    match_end: 13,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/utils/logger.rs".into(),
                folder: "src/utils".into(),
                filename: "logger.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 8,
                    line_content: "// Main logger setup".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/utils/cache.rs".into(),
                folder: "src/utils".into(),
                filename: "cache.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 33,
                    line_content: "impl MainCache { }".into(),
                    match_start: 5,
                    match_end: 9,
                }],
            },
            // src/models folder
            SearchFileResult {
                path: "/Users/shuya/project/src/models/user.rs".into(),
                folder: "src/models".into(),
                filename: "user.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 15,
                    line_content: "// Main user model".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/models/post.rs".into(),
                folder: "src/models".into(),
                filename: "post.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 22,
                    line_content: "fn main_query() {}".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/models/comment.rs".into(),
                folder: "src/models".into(),
                filename: "comment.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 10,
                    line_content: "struct MainComment {}".into(),
                    match_start: 7,
                    match_end: 11,
                }],
            },
            // src/services folder
            SearchFileResult {
                path: "/Users/shuya/project/src/services/auth.rs".into(),
                folder: "src/services".into(),
                filename: "auth.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 45,
                    line_content: "// Main auth flow".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/services/api.rs".into(),
                folder: "src/services".into(),
                filename: "api.rs".into(),
                matches: vec![
                    SearchMatch {
                        line_number: 12,
                        line_content: "pub fn main_handler() {}".into(),
                        match_start: 7,
                        match_end: 11,
                    },
                    SearchMatch {
                        line_number: 88,
                        line_content: "// Main API routes".into(),
                        match_start: 3,
                        match_end: 7,
                    },
                ],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/services/database.rs".into(),
                folder: "src/services".into(),
                filename: "database.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 55,
                    line_content: "fn main_connection() {}".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/services/search.rs".into(),
                folder: "src/services".into(),
                filename: "search.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 30,
                    line_content: "impl MainSearch {}".into(),
                    match_start: 5,
                    match_end: 9,
                }],
            },
            // src/ui folder
            SearchFileResult {
                path: "/Users/shuya/project/src/ui/components.rs".into(),
                folder: "src/ui".into(),
                filename: "components.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 100,
                    line_content: "// Main UI components".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/ui/layout.rs".into(),
                folder: "src/ui".into(),
                filename: "layout.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 25,
                    line_content: "fn main_layout() {}".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/src/ui/theme.rs".into(),
                folder: "src/ui".into(),
                filename: "theme.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 8,
                    line_content: "pub const MAIN_COLOR: u32".into(),
                    match_start: 10,
                    match_end: 14,
                }],
            },
            // tests folder
            SearchFileResult {
                path: "/Users/shuya/project/tests/integration.rs".into(),
                folder: "tests".into(),
                filename: "integration.rs".into(),
                matches: vec![
                    SearchMatch {
                        line_number: 10,
                        line_content: "#[test]".into(),
                        match_start: 0,
                        match_end: 7,
                    },
                    SearchMatch {
                        line_number: 11,
                        line_content: "fn test_main_flow() {".into(),
                        match_start: 8,
                        match_end: 12,
                    },
                ],
            },
            SearchFileResult {
                path: "/Users/shuya/project/tests/unit.rs".into(),
                folder: "tests".into(),
                filename: "unit.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 5,
                    line_content: "fn test_main_helper() {}".into(),
                    match_start: 8,
                    match_end: 12,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/tests/e2e.rs".into(),
                folder: "tests".into(),
                filename: "e2e.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 20,
                    line_content: "// Main E2E test".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/tests/bench.rs".into(),
                folder: "tests".into(),
                filename: "bench.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 15,
                    line_content: "fn bench_main() {}".into(),
                    match_start: 9,
                    match_end: 13,
                }],
            },
            // docs folder
            SearchFileResult {
                path: "/Users/shuya/project/docs/README.md".into(),
                folder: "docs".into(),
                filename: "README.md".into(),
                matches: vec![SearchMatch {
                    line_number: 1,
                    line_content: "# Main Documentation".into(),
                    match_start: 2,
                    match_end: 6,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/docs/ARCHITECTURE.md".into(),
                folder: "docs".into(),
                filename: "ARCHITECTURE.md".into(),
                matches: vec![SearchMatch {
                    line_number: 15,
                    line_content: "## Main Components".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/docs/API.md".into(),
                folder: "docs".into(),
                filename: "API.md".into(),
                matches: vec![SearchMatch {
                    line_number: 8,
                    line_content: "The main API endpoint".into(),
                    match_start: 4,
                    match_end: 8,
                }],
            },
            // examples folder
            SearchFileResult {
                path: "/Users/shuya/project/examples/basic.rs".into(),
                folder: "examples".into(),
                filename: "basic.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 3,
                    line_content: "fn main() { }".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/examples/advanced.rs".into(),
                folder: "examples".into(),
                filename: "advanced.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 10,
                    line_content: "fn main() { advanced_setup(); }".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            SearchFileResult {
                path: "/Users/shuya/project/examples/demo.rs".into(),
                folder: "examples".into(),
                filename: "demo.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 5,
                    line_content: "fn main() { run_demo(); }".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
            // benches folder
            SearchFileResult {
                path: "/Users/shuya/project/benches/perf.rs".into(),
                folder: "benches".into(),
                filename: "perf.rs".into(),
                matches: vec![SearchMatch {
                    line_number: 12,
                    line_content: "fn main_benchmark() {}".into(),
                    match_start: 3,
                    match_end: 7,
                }],
            },
        ]
    }

    fn generate_dummy_file_content(path: &str) -> String {
        // ... existing dummy content logic ...
        if path.ends_with("main.rs") {
            r#"use std::io;
use crate::lib::process;

mod config;
mod utils;

/// Application entry point
///
/// This is the main function that starts the application.
/// It initializes all required components and runs the event loop.

#[derive(Debug)]
struct AppState {
    running: bool,
    counter: u32,
}

fn main() {
    println!("Starting application...");

    let state = AppState {
        running: true,
        counter: 0,
    };

    // Initialize logging
    tracing_subscriber::init();

    // Start the main event loop
    loop {
        if !state.running {
            break;
        }

        process_events(&state);

        // Check for shutdown
        if should_exit() {
            break;
        }
    }

    println!("Hello, main world!");

    println!("Application shutdown complete");
}

fn process_events(state: &AppState) {
    // Event processing logic
}

fn should_exit() -> bool {
    false
}
"#
            .into()
        } else if path.ends_with("lib.rs") {
            r#"//! Library root module
//!
//! This module exports all public APIs.

pub mod core;
pub mod utils;
pub mod services;
pub mod main_utils;

pub use core::*;
pub use utils::helpers;

/// Library version
pub const VERSION: &str = "0.1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
"#
            .into()
        } else if path.ends_with("helper.rs") {
            r#"//! Helper utilities module
//!
//! Provides various helper functions for common operations.

use std::path::Path;
use std::collections::HashMap;

/// Configuration for helpers
#[derive(Debug, Clone)]
pub struct HelperConfig {
    pub cache_enabled: bool,
    pub max_retries: u32,
}

impl Default for HelperConfig {
    fn default() -> Self {
        Self {
            cache_enabled: true,
            max_retries: 3,
        }
    }
}

/// Main helper function
pub fn main_helper(input: &str) -> String {
    let processed = input.trim().to_lowercase();

    // Apply transformations
    let result = processed
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>();

    result
}

/// Process a file path
pub fn process_path(path: &Path) -> Option<String> {
    path.to_str().map(|s| s.to_string())
}

/// Parse key-value pairs
pub fn parse_pairs(input: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for line in input.lines() {
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    // Call main processing

    map
}
"#
            .into()
        } else {
            r#"//! Integration tests

use super::*;

mod common;

/// Setup test environment
fn setup() -> TestContext {
    TestContext::new()
}

#[test]
fn test_main_flow() {
    let ctx = setup();

    // Test the main flow
    let result = ctx.run_main();
    assert!(result.is_ok());
}

#[test]
fn test_error_handling() {
    let ctx = setup();

    // Test error cases
    let result = ctx.run_with_invalid_input();
    assert!(result.is_err());
}

#[test]
fn test_edge_cases() {
    let ctx = setup();

    // Empty input
    assert_eq!(ctx.process(""), "");

    // Very long input
    let long_input = "a".repeat(10000);
    assert!(ctx.process(&long_input).len() <= 10000);
}
"#
            .into()
        }
    }

    fn toggle_match_case(&mut self, cx: &mut Context<Self>) {
        self.match_case = !self.match_case;
        cx.notify();
    }

    fn toggle_match_whole_word(&mut self, cx: &mut Context<Self>) {
        self.match_whole_word = !self.match_whole_word;
        cx.notify();
    }

    fn toggle_use_regex(&mut self, cx: &mut Context<Self>) {
        self.use_regex = !self.use_regex;
        cx.notify();
    }

    fn toggle_file(&mut self, file_idx: usize) {
        if self.expanded_files.contains(&file_idx) {
            self.expanded_files.remove(&file_idx);
        } else {
            self.expanded_files.insert(file_idx);
        }
    }

    fn select_file(&mut self, file_idx: usize) {
        self.selected_file = Some(file_idx);
        self.selected_match = None;

        if let Some(result) = self.search_results.get(file_idx) {
            self.preview_path = Some(result.path.clone());
            self.preview_content = Some(Self::generate_dummy_file_content(&result.path));
        }
    }

    fn select_match(&mut self, file_idx: usize, match_idx: usize) {
        self.selected_file = Some(file_idx);
        self.selected_match = Some((file_idx, match_idx));

        if let Some(result) = self.search_results.get(file_idx) {
            self.preview_path = Some(result.path.clone());
            self.preview_content = Some(Self::generate_dummy_file_content(&result.path));
        }
    }

    fn total_matches(&self) -> usize {
        self.search_results.iter().map(|r| r.matches.len()).sum()
    }

    fn syntect_color_to_gpui(color: Color) -> Rgba {
        let r = color.r as u32;
        let g = color.g as u32;
        let b = color.b as u32;
        let a = color.a as u32;
        let hex = (r << 24) | (g << 16) | (b << 8) | a;
        gpui::rgba(hex)
    }
}

impl Focusable for SearchPage {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SearchPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .track_focus(&self.focus_handle)
            .child(
                div().flex_1().flex().flex_row().min_h(px(0.0)).child(
                    h_resizable("search-page", self.resizable.clone())
                        .child(
                            resizable_panel()
                                .size(px(320.0))
                                .size_range(px(240.0)..px(480.0))
                                .child(self.render_left_pane(window, cx)),
                        )
                        .child(resizable_panel().child(self.render_right_pane(window, cx)))
                        .into_any_element(),
                ),
            )
    }
}

impl SearchPage {
    fn render_left_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .border_r_1()
            .border_color(rgb(theme::BORDER))
            .overflow_hidden()
            .child(self.render_search_header(window, cx))
            .child(self.render_search_results(cx))
    }

    fn render_search_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let match_case = self.match_case;
        let match_whole_word = self.match_whole_word;
        let use_regex = self.use_regex;

        div()
            .flex()
            .flex_col()
            .p(px(12.0))
            .gap_2()
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            // Main search input - VS Code style
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    // Search input row
                    .child(
                        div()
                            .w_full()
                            .bg(rgb(theme::WHITE))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgb(theme::BORDER))
                            .child({
                                let si = self.search_input.clone();
                                div()
                                    .w_full()
                                    .min_h(px(28.0))
                                    .p(px(6.0))
                                    .child(TextInput::new(&si))
                            }),
                    )
                    // Toggle buttons row below input
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("toggle-case")
                                    .px(px(5.0))
                                    .py(px(2.0))
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .bg(if match_case {
                                        rgb(theme::ACCENT_LIGHT)
                                    } else {
                                        rgb(theme::BG)
                                    })
                                    .border_1()
                                    .border_color(if match_case {
                                        rgb(theme::ACCENT)
                                    } else {
                                        rgb(theme::BORDER)
                                    })
                                    .hover(|s| s.border_color(rgb(theme::ACCENT)))
                                    .text_color(if match_case {
                                        rgb(theme::ACCENT)
                                    } else {
                                        rgb(theme::MUTED)
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_match_case(cx)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child("Aa"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("toggle-word")
                                    .px(px(5.0))
                                    .py(px(2.0))
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .bg(if match_whole_word {
                                        rgb(theme::ACCENT_LIGHT)
                                    } else {
                                        rgb(theme::BG)
                                    })
                                    .border_1()
                                    .border_color(if match_whole_word {
                                        rgb(theme::ACCENT)
                                    } else {
                                        rgb(theme::BORDER)
                                    })
                                    .hover(|s| s.border_color(rgb(theme::ACCENT)))
                                    .text_color(if match_whole_word {
                                        rgb(theme::ACCENT)
                                    } else {
                                        rgb(theme::MUTED)
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_match_whole_word(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child("ab"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("toggle-regex")
                                    .px(px(5.0))
                                    .py(px(2.0))
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .bg(if use_regex {
                                        rgb(theme::ACCENT_LIGHT)
                                    } else {
                                        rgb(theme::BG)
                                    })
                                    .border_1()
                                    .border_color(if use_regex {
                                        rgb(theme::ACCENT)
                                    } else {
                                        rgb(theme::BORDER)
                                    })
                                    .hover(|s| s.border_color(rgb(theme::ACCENT)))
                                    .text_color(if use_regex {
                                        rgb(theme::ACCENT)
                                    } else {
                                        rgb(theme::MUTED)
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_use_regex(cx)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(".*"),
                                    ),
                            ),
                    ),
            )
            // Results summary
            .child(
                div()
                    .flex()
                    .items_center()
                    .pt(px(6.0))
                    // Buttons on the left
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("clear-results")
                                    .px(px(4.0))
                                    .py(px(2.0))
                                    .rounded(px(2.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_results.clear();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme::MUTED))
                                            .child("Clear"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("collapse-all")
                                    .px(px(4.0))
                                    .py(px(2.0))
                                    .rounded(px(2.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.expanded_folders.clear();
                                        this.expanded_files.clear();
                                        cx.notify();
                                    }))
                                    .child(
                                        Icon::new(gpui_component::IconName::Minus)
                                            .size_3()
                                            .text_color(rgb(theme::MUTED)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("expand-all")
                                    .px(px(4.0))
                                    .py(px(2.0))
                                    .rounded(px(2.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        for result in &this.search_results {
                                            this.expanded_folders.insert(result.folder.clone());
                                        }
                                        for i in 0..this.search_results.len() {
                                            this.expanded_files.insert(i);
                                        }
                                        cx.notify();
                                    }))
                                    .child(
                                        Icon::new(gpui_component::IconName::Plus)
                                            .size_3()
                                            .text_color(rgb(theme::MUTED)),
                                    ),
                            ),
                    )
                    // Spacer pushes match count to the right
                    .child(div().flex_1())
                    // Match count on the right
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .child(format!(
                                "{} matches in {} files",
                                self.total_matches(),
                                self.search_results.len()
                            )),
                    ),
            )
    }

    fn render_search_results(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        use std::collections::BTreeMap;

        // Group files by folder
        let mut folders: BTreeMap<String, Vec<(usize, SearchFileResult)>> = BTreeMap::new();
        for (file_idx, result) in self.search_results.clone().into_iter().enumerate() {
            folders
                .entry(result.folder.clone())
                .or_default()
                .push((file_idx, result));
        }

        let mut results_list = div()
            .id("search-results-scroll")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .py(px(4.0));

        for (folder_name, files) in folders {
            let folder_cloned = folder_name.clone();
            let is_folder_expanded = self.expanded_folders.contains(&folder_name);
            let folder_match_count: usize = files.iter().map(|(_, f)| f.matches.len()).sum();
            let folder_file_count = files.len();

            // Folder header
            results_list = results_list.child(
                div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "folder-{}",
                        folder_name
                    ))))
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .py(px(5.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.expanded_folders.contains(&folder_cloned) {
                            this.expanded_folders.remove(&folder_cloned);
                        } else {
                            this.expanded_folders.insert(folder_cloned.clone());
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(if is_folder_expanded {
                                    gpui_component::IconName::ChevronDown
                                } else {
                                    gpui_component::IconName::ChevronRight
                                })
                                .size_3()
                                .text_color(rgb(theme::MUTED)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(gpui_component::IconName::Folder)
                                    .size_4()
                                    .text_color(rgb(0xE8A838)), // Folder yellow color
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(theme::FG))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(folder_name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::MUTED))
                                    .child(format!("({} files)", folder_file_count)),
                            ),
                    )
                    // Spacer to push badge right
                    .child(div().flex_1())
                    .child(
                        div()
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(8.0))
                            .bg(rgb(theme::ACCENT_LIGHT))
                            .text_xs()
                            .font_family("monospace")
                            .text_color(rgb(theme::ACCENT))
                            .child(format!("{}", folder_match_count)),
                    ),
            );

            // Files in folder (if expanded)
            if is_folder_expanded {
                for (file_idx, result) in files {
                    let is_file_expanded = self.expanded_files.contains(&file_idx);
                    let is_selected = self.selected_file == Some(file_idx);

                    // File header (indented under folder)
                    results_list = results_list.child(
                        div()
                            .id(ElementId::Name(SharedString::from(format!(
                                "file-{}",
                                file_idx
                            ))))
                            .flex()
                            .items_center()
                            .pl(px(24.0)) // Indent under folder
                            .pr(px(8.0))
                            .py(px(3.0))
                            .cursor_pointer()
                            .when(is_selected && self.selected_match.is_none(), |s| {
                                s.bg(rgb(theme::ACCENT_LIGHT))
                            })
                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_file(file_idx);
                                this.select_file(file_idx);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .w(px(16.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Icon::new(if is_file_expanded {
                                            gpui_component::IconName::ChevronDown
                                        } else {
                                            gpui_component::IconName::ChevronRight
                                        })
                                        .size_3()
                                        .text_color(rgb(theme::MUTED)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Icon::new(gpui_component::IconName::File)
                                            .size_4()
                                            .text_color(rgb(theme::GRAY_500)),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme::FG))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .child(result.filename.clone()),
                                    ),
                            )
                            // Spacer to push badge/close button right
                            .child(div().flex_1())
                            .child(if is_selected {
                                // Show X button when selected
                                div()
                                    .id(ElementId::Name(SharedString::from(format!(
                                        "close-file-{}",
                                        file_idx
                                    ))))
                                    .px(px(4.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        // Remove file from search results by index
                                        if file_idx < this.search_results.len() {
                                            this.search_results.remove(file_idx);
                                        }
                                        this.selected_file = None;
                                        this.preview_content = None;
                                        this.preview_path = None;
                                        this.expanded_files.clear(); // Clear expansion state since indices changed
                                        cx.notify();
                                    }))
                                    .child(
                                        Icon::new(gpui_component::IconName::Close)
                                            .size_3()
                                            .text_color(rgb(theme::MUTED)),
                                    )
                                    .into_any_element()
                            } else {
                                // Show match count badge when not selected
                                div()
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(8.0))
                                    .bg(rgb(theme::GRAY_200))
                                    .text_xs()
                                    .font_family("monospace")
                                    .text_color(rgb(theme::MUTED))
                                    .child(format!("{}", result.matches.len()))
                                    .into_any_element()
                            }),
                    );

                    // Match lines (if file expanded)
                    if is_file_expanded {
                        for (match_idx, search_match) in result.matches.iter().enumerate() {
                            let is_match_selected =
                                self.selected_match == Some((file_idx, match_idx));

                            results_list = results_list.child(
                                div()
                                    .id(ElementId::Name(SharedString::from(format!(
                                        "match-{}-{}",
                                        file_idx, match_idx
                                    ))))
                                    .flex()
                                    .items_center()
                                    .pl(px(56.0)) // Further indent under file
                                    .pr(px(8.0))
                                    .py(px(2.0))
                                    .cursor_pointer()
                                    .when(is_match_selected, |s| s.bg(rgb(theme::ACCENT_LIGHT)))
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_match(file_idx, match_idx);
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family("monospace") // Consistent width
                                            .text_color(rgb(theme::MUTED))
                                            .w(px(40.0))
                                            .flex_shrink_0()
                                            .pl(px(8.0)) // Add left padding
                                            .child(format!("{}", search_match.line_number)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family("monospace")
                                            .text_color(rgb(theme::FG_SECONDARY))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .child(Self::render_highlighted_line(search_match)),
                                    ),
                            );
                        }
                    }
                }
            }
        }

        results_list
    }

    fn render_highlighted_line(search_match: &SearchMatch) -> String {
        // Just text for now in tree view (could add highlighting if sophisticated)
        search_match.line_content.trim().to_string()
    }

    fn render_right_pane(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .overflow_hidden()
            .child(self.render_preview_header())
            .child(self.render_preview_content().into_any_element()) // Fix return type
    }

    fn render_preview_header(&self) -> impl IntoElement {
        let path_display = self
            .preview_path
            .as_ref()
            .map(|p| {
                if let Some(idx) = p.rfind('/') {
                    let parent = if idx > 0 { &p[..idx] } else { "/" };
                    let file = &p[idx + 1..];
                    format!("{}  /  {}", parent.split('/').last().unwrap_or(""), file)
                } else {
                    p.clone()
                }
            })
            .unwrap_or_else(|| "No file selected".into());

        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .bg(rgb(theme::BG_SECONDARY))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        Icon::new(gpui_component::IconName::File)
                            .size_4()
                            .text_color(rgb(theme::GRAY_500)),
                    )
                    .child(
                        div()
                            .ml(px(8.0))
                            .text_sm()
                            .text_color(rgb(theme::FG))
                            .child(path_display),
                    ),
            )
            // Preview status
            .child(div().text_xs().text_color(rgb(theme::MUTED)).child(
                if self.preview_content.is_some() {
                    "Read-only"
                } else {
                    ""
                },
            ))
    }

    fn render_preview_content(&self) -> impl IntoElement {
        let content = self
            .preview_content
            .clone()
            .unwrap_or_else(|| "Select a file to preview its contents.".to_string());

        if self.preview_content.is_none() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme::FG_SECONDARY))
                .child(content)
                .into_any_element();
        }

        let lines: Vec<&str> = content.lines().collect();

        // Highlighting
        let extension = self
            .preview_path
            .as_ref()
            .and_then(|p| p.split('.').last())
            .unwrap_or("rs");

        let syntax = self
            .syntax_set
            .find_syntax_by_extension(extension)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        // Use a theme that matches light/dark. Hardcoded to 'base16-ocean.dark' example,
        // but we should pick one from the set.
        // Actually load_defaults() gives standard themes. "base16-ocean.dark", "base16-eighties.dark", "base16-mocha.dark", "base16-ocean.light"
        let theme_name = "base16-ocean.light"; // Matching the light theme of the app roughly
        let theme = &self.theme_set.themes[theme_name];

        let mut highlighter = HighlightLines::new(syntax, theme);

        // Find matching lines for highlighting
        let highlight_lines: HashSet<usize> = if let Some((file_idx, _)) = self.selected_match {
            if let Some(result) = self.search_results.get(file_idx) {
                result.matches.iter().map(|m| m.line_number).collect()
            } else {
                HashSet::new()
            }
        } else if let Some(file_idx) = self.selected_file {
            if let Some(result) = self.search_results.get(file_idx) {
                result.matches.iter().map(|m| m.line_number).collect()
            } else {
                HashSet::new()
            }
        } else {
            HashSet::new()
        };

        let mut content_div = div()
            .id("preview-content-scroll")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_scroll()
            .pb(px(8.0)) // Only bottom padding, no top gap
            .bg(rgb(theme::WHITE)); // Code background

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let is_highlighted = highlight_lines.contains(&line_num);

            // Syntax Highlight the line
            let ranges: Vec<(SyntectStyle, &str)> = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            let mut code_line_div = div().flex().flex_row();
            for (style, text) in ranges {
                let color = style.foreground;
                code_line_div = code_line_div.child(
                    div()
                        .text_color(Self::syntect_color_to_gpui(color))
                        .child(text.to_string()),
                );
            }

            content_div = content_div.child(
                div()
                    .flex()
                    .items_start()
                    .min_h(px(20.0))
                    .when(is_highlighted, |s| s.bg(rgb(0xFFF9C4))) // Yellow highlight
                    .child(
                        div()
                            .w(px(52.0))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .pr(px(16.0)) // Right padding for the number
                            .pl(px(8.0)) // Left padding from edge
                            .text_xs()
                            .font_family("monospace") // Consistent width
                            .text_color(rgb(theme::MUTED))
                            .bg(rgb(theme::GRAY_50))
                            .child(format!("{}", line_num)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .pl(px(8.0)) // Padding for code
                            .text_xs()
                            .font_family("monospace")
                            .whitespace_nowrap() // Preserve whitespace flow but prevent wrapping
                            .child(code_line_div),
                    ),
            );
        }

        content_div.into_any_element()
    }
}

impl crate::pages::Page for SearchPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}
