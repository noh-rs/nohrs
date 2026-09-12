//! The launcher's search field.
//!
//! A single-line text field built directly on GPUI rather than taken from
//! `gpui-component`, because that library's `Input` reaches for a
//! `gpui_component::Root` at the window's first layer while painting. Rooting
//! the launcher window at `Root` means an opaque, square background is painted
//! under everything, which is exactly what the rounded, transparent panel of
//! docs/launcher.md §1 cannot have. Owning the field is what buys the window.
//!
//! The text handling follows GPUI's own `examples/input.rs`: offsets are byte
//! offsets into the content, converted at the boundary to the UTF-16 offsets the
//! platform input APIs speak, and the cursor moves by grapheme so that combining
//! marks and emoji are single characters to the user.
//!
//! Composition (IME) is what makes this more than a `String` plus a key handler.
//! [`EntityInputHandler`] is the interface through which macOS, X11 and Wayland
//! deliver preedit text: the in-progress reading is held in `marked_range`,
//! drawn underlined, and only committed when the platform says so.

use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla,
    InspectorElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, UnderlineStyle, Window, actions, div, fill, point, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

actions!(
    launcher_field,
    [
        /// Delete the character before the cursor.
        FieldBackspace,
        /// Delete the character after the cursor.
        FieldDelete,
        /// Delete the word before the cursor.
        FieldDeleteWord,
        /// Delete from the cursor back to the start of the query.
        FieldDeleteToStart,
        /// Move the cursor one character left.
        FieldLeft,
        /// Move the cursor one character right.
        FieldRight,
        /// Extend the selection one character left.
        FieldSelectLeft,
        /// Extend the selection one character right.
        FieldSelectRight,
        /// Select the whole query.
        FieldSelectAll,
        /// Move the cursor to the start of the query.
        FieldHome,
        /// Move the cursor to the end of the query.
        FieldEnd,
        /// Copy the selection.
        FieldCopy,
        /// Cut the selection.
        FieldCut,
        /// Paste over the selection.
        FieldPaste,
    ]
);

/// Key context the field's bindings live in.
const CONTEXT: &str = "LauncherField";

/// Registers the field's key bindings. Call once at startup.
///
/// These are deliberately scoped to [`CONTEXT`] so they cannot shadow the
/// launcher's own navigation keys, which are handled a layer up.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", FieldBackspace, Some(CONTEXT)),
        KeyBinding::new("delete", FieldDelete, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-backspace", FieldDeleteWord, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-backspace", FieldDeleteWord, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-backspace", FieldDeleteToStart, Some(CONTEXT)),
        KeyBinding::new("left", FieldLeft, Some(CONTEXT)),
        KeyBinding::new("right", FieldRight, Some(CONTEXT)),
        KeyBinding::new("shift-left", FieldSelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", FieldSelectRight, Some(CONTEXT)),
        KeyBinding::new("home", FieldHome, Some(CONTEXT)),
        KeyBinding::new("end", FieldEnd, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", FieldSelectAll, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", FieldCopy, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", FieldCut, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", FieldPaste, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", FieldSelectAll, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", FieldCopy, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", FieldCut, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", FieldPaste, Some(CONTEXT)),
    ]);
}

/// Emitted when the committed query text changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChanged;

/// Colours the field paints itself with, so it stays independent of any one
/// theme and can be told what to look like by its host.
#[derive(Debug, Clone, Copy)]
pub struct FieldStyle {
    /// Colour of the query text.
    pub text: Hsla,
    /// Colour of the placeholder shown while the field is empty.
    pub placeholder: Hsla,
    /// Colour of the insertion caret.
    pub cursor: Hsla,
    /// Fill behind selected text.
    pub selection: Hsla,
}

/// A single-line text field with composition support.
pub struct SearchField {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    style: FieldStyle,
    /// Byte offsets into `content`.
    selected_range: Range<usize>,
    selection_reversed: bool,
    /// Byte range of in-progress composition, if any.
    marked_range: Option<Range<usize>>,
    /// Last painted line and bounds, needed to turn a mouse position into an
    /// offset and to tell the platform where to put its candidate window.
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl EventEmitter<FieldChanged> for SearchField {}

impl Focusable for SearchField {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SearchField {
    /// Builds an empty field showing `placeholder`.
    pub fn new(
        placeholder: impl Into<SharedString>,
        style: FieldStyle,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: SharedString::default(),
            placeholder: placeholder.into(),
            style,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    /// The current text.
    pub fn text(&self) -> &SharedString {
        &self.content
    }

    /// Whether a composition is in progress, so the text on screen is a reading
    /// the user has not committed yet.
    pub fn is_composing(&self) -> bool {
        self.marked_range.is_some()
    }

    /// Replaces the text and puts the cursor at its end.
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        let text = text.into();
        if text == self.content {
            return;
        }
        let end = text.len();
        self.content = text;
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.emit(FieldChanged);
        cx.notify();
    }

    /// Restyles the field, for a theme change.
    pub fn set_style(&mut self, style: FieldStyle, cx: &mut Context<Self>) {
        self.style = style;
        cx.notify();
    }

    /// Announces a change, unless a composition is still in progress.
    ///
    /// Preedit text is a reading, not a query — searching for the half-typed
    /// kana behind a kanji conversion would fill the list with noise and throw
    /// the selection away on every keystroke. The search waits for the commit.
    fn notify_changed(&mut self, cx: &mut Context<Self>) {
        if self.marked_range.is_none() {
            cx.emit(FieldChanged);
        }
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    /// Byte offset of the grapheme boundary before `offset`.
    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    /// Byte offset of the grapheme boundary after `offset`.
    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    /// Start of the word before `offset`, for word-wise delete.
    fn previous_word_boundary(&self, offset: usize) -> usize {
        let before = self.content.get(..offset).unwrap_or_default();
        before
            .split_word_bound_indices()
            .rfind(|(_, word)| !word.trim().is_empty())
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for character in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += character.len_utf16();
            utf8_offset += character.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for character in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += character.len_utf8();
            utf16_offset += character.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if self.content.is_empty() || position.x <= bounds.left() {
            return 0;
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn on_backspace(&mut self, _: &FieldBackspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_delete(&mut self, _: &FieldDelete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_delete_word(&mut self, _: &FieldDeleteWord, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_word_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_delete_to_start(
        &mut self,
        _: &FieldDeleteToStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(0, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_left(&mut self, _: &FieldLeft, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn on_right(&mut self, _: &FieldRight, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn on_select_left(
        &mut self,
        _: &FieldSelectLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn on_select_right(
        &mut self,
        _: &FieldSelectRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn on_select_all(&mut self, _: &FieldSelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn on_home(&mut self, _: &FieldHome, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn on_end(&mut self, _: &FieldEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn selected_text(&self) -> Option<&str> {
        if self.selected_range.is_empty() {
            return None;
        }
        self.content.get(self.selected_range.clone())
    }

    fn on_copy(&mut self, _: &FieldCopy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        }
    }

    fn on_cut(&mut self, _: &FieldCut, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_paste(&mut self, _: &FieldPaste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        // The field is one line, so pasted newlines become spaces rather than
        // silently truncating the paste at the first one.
        let flattened = text.replace(['\n', '\r'], " ");
        self.replace_text_in_range(None, &flattened, window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        let offset = self.offset_for_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            self.select_to(self.offset_for_position(event.position), cx);
        }
    }
}

impl EntityInputHandler for SearchField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content.get(range)?.to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.take().is_none() {
            return;
        }
        // The preedit text stays in `content` — the platform is saying it is no
        // longer a composition, which makes it a committed query worth running,
        // and the underline has to come off.
        self.notify_changed(cx);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let Some(replaced) = self.replaced(&range, new_text) else {
            return;
        };

        self.content = replaced.into();
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        // Committing ends any composition, which is what makes this the point
        // where the query is worth searching for.
        self.marked_range = None;
        self.notify_changed(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let Some(replaced) = self.replaced(&range, new_text) else {
            return;
        };

        self.content = replaced.into();
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        // The platform reports this selection in UTF-16 units *of `new_text`*,
        // not of the whole field. Converting it against the content would land
        // the caret somewhere else entirely whenever anything precedes the
        // composition — which is the common case, since people type before they
        // convert.
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selection| {
                let start = range.start + utf16_to_utf8_offset(new_text, selection.start);
                let end = range.start + utf16_to_utf8_offset(new_text, selection.end);
                start..end
            })
            .unwrap_or_else(|| {
                let cursor = range.start + new_text.len();
                cursor..cursor
            });
        self.selection_reversed = false;
        self.notify_changed(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                element_bounds.left() + line.x_for_index(range.start),
                element_bounds.top(),
            ),
            point(
                element_bounds.left() + line.x_for_index(range.end),
                element_bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(point.x - bounds.left())?;
        Some(self.offset_to_utf16(index))
    }
}

/// Converts a UTF-16 offset into `text` to the matching byte offset.
///
/// [`SearchField`]'s own conversions run against its content; this one runs
/// against whatever string the platform is talking about, which during
/// composition is the fragment it just handed over rather than the whole query.
fn utf16_to_utf8_offset(text: &str, offset: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for character in text.chars() {
        if utf16 >= offset {
            break;
        }
        utf16 += character.len_utf16();
        utf8 += character.len_utf8();
    }
    utf8
}

impl SearchField {
    /// Splices `new_text` into `range`, or returns `None` when the range is not
    /// on character boundaries — the platform can hand back a stale range after
    /// the content has moved under it, and slicing on it would panic.
    fn replaced(&self, range: &Range<usize>, new_text: &str) -> Option<String> {
        let before = self.content.get(..range.start)?;
        let after = self.content.get(range.end..)?;
        Some(format!("{before}{new_text}{after}"))
    }
}

impl Render for SearchField {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .size_full()
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_delete_word))
            .on_action(cx.listener(Self::on_delete_to_start))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(FieldElement { field: cx.entity() })
    }
}

/// Draws the field's one line of text, its caret, and its selection.
struct FieldElement {
    field: Entity<SearchField>,
}

/// What [`FieldElement::prepaint`] worked out for `paint` to draw.
struct FieldPrepaint {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for FieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = FieldPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let field = self.field.read(cx);
        let selected_range = field.selected_range.clone();
        let cursor_offset = field.cursor_offset();
        let field_style = field.style;
        let text_style = window.text_style();

        let (display_text, color) = if field.content.is_empty() {
            (field.placeholder.clone(), field_style.placeholder)
        } else {
            (field.content.clone(), field_style.text)
        };

        let run = TextRun {
            len: display_text.len(),
            font: text_style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // Composition is drawn underlined, the convention every platform's IME
        // users already read as "not committed yet".
        let runs = match field.marked_range.as_ref() {
            Some(marked) if !field.content.is_empty() => [
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.len(),
                    underline: Some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len().saturating_sub(marked.end),
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            _ => vec![run],
        };

        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let (selection, cursor) = if selected_range.is_empty() {
            let caret = Bounds::new(
                point(
                    bounds.left() + line.x_for_index(cursor_offset),
                    bounds.top(),
                ),
                size(px(1.5), bounds.bottom() - bounds.top()),
            );
            (None, Some(fill(caret, field_style.cursor)))
        } else {
            let highlight = Bounds::from_corners(
                point(
                    bounds.left() + line.x_for_index(selected_range.start),
                    bounds.top(),
                ),
                point(
                    bounds.left() + line.x_for_index(selected_range.end),
                    bounds.bottom(),
                ),
            );
            (Some(fill(highlight, field_style.selection)), None)
        };

        FieldPrepaint {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.field.read(cx).focus_handle.clone();
        // This is what routes composition to `EntityInputHandler`, and what
        // tells the platform where to put its candidate window.
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let Some(line) = prepaint.line.take() else {
            return;
        };
        if let Err(error) = line.paint(bounds.origin, window.line_height(), window, cx) {
            tracing::warn!("could not paint the search field: {error}");
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.field.update(cx, |field, _cx| {
            field.last_layout = Some(line);
            field.last_bounds = Some(bounds);
        });
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, WindowHandle};

    use super::*;

    fn style() -> FieldStyle {
        let plain = gpui::hsla(0.0, 0.0, 0.0, 1.0);
        FieldStyle {
            text: plain,
            placeholder: plain,
            cursor: plain,
            selection: plain,
        }
    }

    fn new_field(cx: &mut TestAppContext) -> WindowHandle<SearchField> {
        cx.add_window(|_window, cx| SearchField::new("Search…", style(), cx))
    }

    /// Types `text` the way a platform without composition does: one committed
    /// replacement of the current selection.
    fn type_text(field: &WindowHandle<SearchField>, cx: &mut TestAppContext, text: &str) {
        let text = text.to_string();
        field
            .update(cx, |field, window, cx| {
                field.replace_text_in_range(None, &text, window, cx);
            })
            .expect("the field's window should be open");
    }

    fn text_of(field: &WindowHandle<SearchField>, cx: &mut TestAppContext) -> String {
        field
            .read_with(cx, |field, _cx| field.text().to_string())
            .expect("the field's window should be open")
    }

    #[gpui::test]
    fn typing_and_deleting_edit_the_query(cx: &mut TestAppContext) {
        let field = new_field(cx);
        type_text(&field, cx, "cargo");
        assert_eq!(text_of(&field, cx), "cargo");

        field
            .update(cx, |field, window, cx| {
                field.on_backspace(&FieldBackspace, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "carg");
    }

    #[gpui::test]
    fn the_cursor_moves_by_grapheme_not_by_byte(cx: &mut TestAppContext) {
        let field = new_field(cx);
        // Each of these is three bytes, so a byte-wise backspace would split one
        // and leave invalid UTF-8 — or panic.
        type_text(&field, cx, "日本語");

        field
            .update(cx, |field, window, cx| {
                field.on_backspace(&FieldBackspace, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "日本");

        field
            .update(cx, |field, window, cx| {
                field.on_left(&FieldLeft, window, cx);
                field.on_backspace(&FieldBackspace, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "本");
    }

    #[gpui::test]
    fn word_delete_removes_the_word_before_the_cursor(cx: &mut TestAppContext) {
        let field = new_field(cx);
        type_text(&field, cx, "src/main.rs");

        // Unicode word segmentation joins a dot between letters into one word,
        // so `main.rs` goes as a unit — which is the useful answer for a path
        // query, where the file name is the thing being reconsidered.
        field
            .update(cx, |field, window, cx| {
                field.on_delete_word(&FieldDeleteWord, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "src/");

        field
            .update(cx, |field, window, cx| {
                field.on_delete_word(&FieldDeleteWord, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "src");
    }

    #[gpui::test]
    fn word_delete_skips_over_trailing_space(cx: &mut TestAppContext) {
        let field = new_field(cx);
        type_text(&field, cx, "open the ");

        // The space after the word goes with it, rather than the delete stopping
        // on the gap and needing a second press to do anything.
        field
            .update(cx, |field, window, cx| {
                field.on_delete_word(&FieldDeleteWord, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "open ");
    }

    #[gpui::test]
    fn select_all_then_typing_replaces_everything(cx: &mut TestAppContext) {
        let field = new_field(cx);
        type_text(&field, cx, "cargo");

        field
            .update(cx, |field, window, cx| {
                field.on_select_all(&FieldSelectAll, window, cx);
            })
            .expect("open");
        type_text(&field, cx, "toml");
        assert_eq!(text_of(&field, cx), "toml");
    }

    #[gpui::test]
    fn composition_is_held_back_until_it_is_committed(cx: &mut TestAppContext) {
        let field = new_field(cx);

        // A Japanese IME builds a reading first; it is on screen and marked, but
        // is not yet a query.
        field
            .update(cx, |field, window, cx| {
                field.replace_and_mark_text_in_range(None, "にほん", None, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "にほん");
        assert!(
            field
                .read_with(cx, |field, _cx| field.is_composing())
                .expect("open")
        );

        // Committing the conversion ends the composition.
        field
            .update(cx, |field, window, cx| {
                field.replace_text_in_range(None, "日本", window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "日本");
        assert!(
            !field
                .read_with(cx, |field, _cx| field.is_composing())
                .expect("open")
        );
    }

    #[gpui::test]
    fn a_composition_selection_is_placed_relative_to_the_new_text(cx: &mut TestAppContext) {
        let field = new_field(cx);
        // Something already typed, so the composition does not start at 0 — the
        // case where interpreting the IME's offsets against the whole content
        // puts the caret in the wrong place.
        type_text(&field, cx, "見て");

        field
            .update(cx, |field, window, cx| {
                // The IME supplies "にほん" and says the caret sits after its
                // second UTF-16 unit, i.e. after "にほ".
                field.replace_and_mark_text_in_range(None, "にほん", Some(2..2), window, cx);
            })
            .expect("open");

        assert_eq!(text_of(&field, cx), "見てにほん");
        field
            .read_with(cx, |field, _cx| {
                // "見て" is 6 bytes, "にほ" a further 6.
                assert_eq!(field.selected_range, 12..12);
                assert_eq!(field.marked_range, Some(6..15));
            })
            .expect("open");
    }

    #[gpui::test]
    fn ending_a_composition_publishes_the_text(cx: &mut TestAppContext) {
        let field = new_field(cx);
        field
            .update(cx, |field, window, cx| {
                field.replace_and_mark_text_in_range(None, "にほん", None, window, cx);
            })
            .expect("open");
        assert!(
            field
                .read_with(cx, |field, _cx| field.is_composing())
                .expect("open")
        );

        // The platform can end a composition without replacing it — the preedit
        // stays and becomes ordinary text, which makes it a query.
        field
            .update(cx, |field, window, cx| field.unmark_text(window, cx))
            .expect("open");

        assert!(
            !field
                .read_with(cx, |field, _cx| field.is_composing())
                .expect("open")
        );
        assert_eq!(text_of(&field, cx), "にほん");
    }

    #[gpui::test]
    fn delete_to_start_clears_everything_before_the_cursor(cx: &mut TestAppContext) {
        let field = new_field(cx);
        type_text(&field, cx, "src/main.rs");

        field
            .update(cx, |field, window, cx| {
                field.on_left(&FieldLeft, window, cx);
                field.on_left(&FieldLeft, window, cx);
                field.on_delete_to_start(&FieldDeleteToStart, window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "rs");
    }

    #[gpui::test]
    fn utf16_offsets_round_trip_through_byte_offsets(cx: &mut TestAppContext) {
        let field = new_field(cx);
        // An emoji outside the basic plane is one char, two UTF-16 units, and
        // four bytes — the case where all three offset spaces disagree.
        type_text(&field, cx, "a🚀b");

        field
            .read_with(cx, |field, _cx| {
                assert_eq!(field.offset_to_utf16(field.content.len()), 4);
                assert_eq!(field.offset_from_utf16(4), field.content.len());
                // The rocket starts at byte 1 and UTF-16 unit 1.
                assert_eq!(field.offset_from_utf16(1), 1);
                assert_eq!(field.offset_to_utf16(1), 1);
                // Byte 5 (after the rocket) is UTF-16 unit 3.
                assert_eq!(field.offset_to_utf16(5), 3);
            })
            .expect("open");
    }

    #[gpui::test]
    fn a_stale_range_from_the_platform_lands_in_bounds(cx: &mut TestAppContext) {
        let field = new_field(cx);
        type_text(&field, cx, "hi");

        // A range past the end arrives in practice when the content has moved
        // under the IME. Converting from UTF-16 clamps it to the content's end,
        // so the edit lands there instead of panicking on an out-of-range slice.
        field
            .update(cx, |field, window, cx| {
                field.replace_text_in_range(Some(90..99), "!", window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "hi!");

        // The same for a range that starts inside a multi-byte character: the
        // conversion only ever yields character boundaries, so the slice is
        // valid and the text stays well-formed.
        type_text(&field, cx, "");
        field
            .update(cx, |field, window, cx| {
                field.set_text("日本", cx);
                field.replace_text_in_range(Some(1..1), "x", window, cx);
            })
            .expect("open");
        assert_eq!(text_of(&field, cx), "日x本");
    }
}
