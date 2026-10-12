//! A single-line text input.
//!
//! GPUI ships no built-in text field (see its own `examples/input.rs`, which the README points
//! to as the way to learn this); this is that example trimmed to one line, restyled to the app's
//! theme, and given a `masked` mode for secrets. The cursor/selection/IME machinery below
//! (grapheme boundaries, UTF-16 index conversions, [`EntityInputHandler`]) is unchanged from it:
//! it is exactly the amount of code correct text editing takes, not something to shrink further.
//!
//! Masking substitutes one `*` per grapheme at render time and keeps the real `content` behind
//! it, so copy/paste and editing operate on the real text. That substitution assumes the content
//! stays ASCII, true for every value this app puts in a masked field (an OAuth client secret).

mod element;
mod handler;

use std::ops::Range;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt, App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler,
    FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ShapedLine, SharedString, Window, div, px,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::ui::kit::{anim, theme};

gpui::actions!(
    wyck_text_input,
    [
        TextInputBackspace,
        TextInputDelete,
        TextInputLeft,
        TextInputRight,
        TextInputSelectLeft,
        TextInputSelectRight,
        TextInputSelectAll,
        TextInputHome,
        TextInputEnd,
        TextInputPaste,
        TextInputCut,
        TextInputCopy,
    ]
);

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    masked: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// How far the text has scrolled left, in pixels, so the cursor stays visible once the
    /// content is wider than the field. Kept on the model (not derived each frame) because it
    /// only changes when the cursor moves outside the current viewport, not on every repaint.
    scroll_offset: Pixels,
    /// Whether the field had focus at the last render, and how many times that has flipped: the
    /// count keys the border-color animation so it replays on every focus change.
    was_focused: bool,
    focus_changes: u64,
}

impl TextInput {
    /// A new, empty field showing `placeholder` when blank.
    pub fn new(cx: &mut Context<Self>, placeholder: impl Into<SharedString>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            masked: false,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll_offset: px(0.),
            was_focused: false,
            focus_changes: 0,
        }
    }

    /// Renders every character as `*`, for a secret. The real content is unaffected.
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// The current text.
    pub fn text(&self) -> &str {
        &self.content
    }

    /// Replaces the text, with the cursor at its end.
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        let text = text.into();
        self.selected_range = text.len()..text.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.scroll_offset = px(0.);
        self.content = text;
        cx.notify();
    }

    fn left(&mut self, _: &TextInputLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &TextInputRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &TextInputSelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &TextInputSelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &TextInputSelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &TextInputHome, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &TextInputEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &TextInputBackspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &TextInputDelete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &TextInputPaste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    fn copy(&mut self, _: &TextInputCopy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &TextInputCut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left() + self.scroll_offset)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    /// The text to actually shape and paint: the real content, or `*` per grapheme when masked.
    fn display_text(&self) -> SharedString {
        if self.masked {
            self.content.graphemes(true).map(|_| '*').collect()
        } else {
            self.content.to_string()
        }
        .into()
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        if focused != self.was_focused {
            self.was_focused = focused;
            self.focus_changes += 1;
        }
        let changes = self.focus_changes;
        let (from, to) = if focused {
            (theme::border_subtle(), theme::accent())
        } else {
            (theme::accent(), theme::border_subtle())
        };

        let field = div()
            .flex()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .h(px(44.))
            .px_3p5()
            .items_center()
            .overflow_hidden()
            .rounded_lg()
            .bg(theme::bg())
            .border_1()
            .border_color(if focused {
                theme::accent()
            } else {
                theme::border_subtle()
            })
            .text_size(px(crate::ui::kit::tokens::text::title()))
            .text_color(theme::fg())
            .child(element::TextElement::new(cx.entity()));

        // The border eases between its resting and focused colors instead of snapping.
        if changes == 0 {
            return field.into_any_element();
        }
        field
            .with_animation(
                ("text-input-focus", cx.entity_id().as_u64() * 1000 + changes),
                Animation::new(Duration::from_millis(160)),
                move |el, delta| el.border_color(anim::lerp_color(from, to, delta)),
            )
            .into_any_element()
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
