use gpui::prelude::*;
use gpui::*;
use gpui_component::input::{Input, InputState, RopeExt};
use std::sync::Once;

actions!(
    preview,
    [
        /// Opens the editor's built-in search panel without triggering a double-borrow panic.
        SafeSearch
    ]
);

static BIND_SEARCH_ONCE: Once = Once::new();

/// A read-only code editor view used to render textual file previews.
pub struct PreviewEditor {
    editor_state: Entity<InputState>,
}

impl PreviewEditor {
    /// Creates a new preview editor, binding a panic-safe search keybinding once.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        BIND_SEARCH_ONCE.call_once(|| {
            // Override the default search keybinding to prevent panic in gpui-component
            // caused by double borrowing InputState during search panel initialization.
            // We bind "cmd-f"/"ctrl-f" to our SafeSearch action in the "Input" context.
            cx.bind_keys(vec![
                KeyBinding::new("cmd-f", SafeSearch, Some("Input")),
                KeyBinding::new("ctrl-f", SafeSearch, Some("Input")),
            ]);
        });

        let editor_state = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor("plain")
                .searchable(true)
                .line_number(true)
                .soft_wrap(false) // Enable horizontal scrolling
        });
        Self { editor_state }
    }

    /// Replaces the editor's contents with `text`.
    pub fn set_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_state.update(cx, |state, cx| {
            state.set_value(text, window, cx);
        });
    }

    /// Sets the syntax-highlighting language for the editor.
    pub fn set_language(&mut self, language: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor_state.update(cx, |state, cx| {
            state.set_highlighter(language, cx);
        });
    }

    /// Applies custom highlight ranges to the editor (currently a no-op).
    pub fn set_highlights(
        &mut self,
        _highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // TODO: Implement custom highlighting matching ranges to InputState's highlighting mechanism
        // InputState uses tree-sitter based highlighting usually, or DiagnosticSet for errors.
        // For search results, we might need a different approach or see if `search` functionality covers it.
    }

    /// Scrolls the editor so the byte `offset` is brought into view.
    pub fn scroll_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        // `InputState::scroll_to` is private; moving the cursor to the offset triggers the
        // same scroll-into-view logic through the public `set_cursor_position` API.
        self.editor_state.update(cx, |state, cx| {
            let position = state.text().offset_to_position(offset);
            state.set_cursor_position(position, window, cx);
        });
    }

    /// Syncs the explorer's search query into the editor (currently a no-op).
    pub fn set_search_query(
        &mut self,
        _query: String,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // gpui-component 0.3.1 exposes no public API to drive the editor's search panel
        // programmatically, so syncing the explorer's query into the preview is a no-op here.
        // Users can still open the in-editor search panel manually (see `on_safe_search`).
    }

    fn on_safe_search(&mut self, _: &SafeSearch, window: &mut Window, cx: &mut Context<Self>) {
        // Open the built-in search panel by focusing the editor and dispatching the
        // gpui-component `Search` action.
        self.editor_state.update(cx, |state, cx| {
            state.focus(window, cx);
        });
        window.dispatch_action(Box::new(gpui_component::input::Search), cx);
    }
}

impl Render for PreviewEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(
                Input::new(&self.editor_state)
                    .size_full()
                    .h_full()
                    .focus_bordered(false) // Remove black focus border
                    .appearance(false), // Remove default border/background for cleaner look
            )
            .on_action(cx.listener(Self::on_safe_search))
    }
}
