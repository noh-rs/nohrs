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
                .soft_wrap(false)
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
        // TODO: gpui-component InputState does not expose highlight API
    }

    pub fn scroll_to(&mut self, _offset: usize, _window: &mut Window, _cx: &mut Context<Self>) {
        // TODO: gpui-component InputState::scroll_to is pub(crate), not accessible
        // scroll_to will be a no-op until the API is exposed
    }

    pub fn set_search_query(
        &mut self,
        _query: String,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // TODO: gpui-component InputState does not expose set_search_query API
        // This will be a no-op until the API is exposed
    }
}

impl Render for PreviewEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        TextInput::new(&self.editor_state)
            .size_full()
            .h_full()
            .focus_bordered(false)
            .appearance(false)
    }
}
