use gpui::prelude::*;
use gpui::*;
use gpui_component::input::{InputState, TextInput};

pub struct PreviewEditor {
    editor_state: Entity<InputState>,
}

impl PreviewEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor_state = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor("plain")
                .searchable(true)
                .line_number(true)
                .soft_wrap(false) // Enable horizontal scrolling
        });
        Self { editor_state }
    }

    pub fn set_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_state.update(cx, |state, cx| {
            state.set_value(text, window, cx);
        });
    }

    pub fn set_language(&mut self, language: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor_state.update(cx, |state, cx| {
            state.set_highlighter(language, cx);
        });
    }

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

    pub fn scroll_to_line(&mut self, _line: usize, _window: &mut Window, _cx: &mut Context<Self>) {
        // TODO: Implement scroll to line if InputState exposes it or via scroll_handle
    }
}

impl Render for PreviewEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        TextInput::new(&self.editor_state)
            .size_full()
            .h_full()
            .focus_bordered(false) // Remove black focus border
            .appearance(false) // Remove default border/background for cleaner look
    }
}
