//! A list of the fonts installed on the computer, to search and pick from.
//!
//! Every font is in the list (it scrolls, only the rows in view are built), each name written in
//! its own font, and the first row puts the choice back on the default. The picker tells what was
//! picked with a [`FontChosen`] event; `None` is the default font.

use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, EventEmitter, ScrollStrategy, SharedString, Subscription,
    UniformListScrollHandle, Window, div, px, uniform_list,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputEvent, InputState};

use crate::{icon, theme, tokens};

/// What the picker says when a font is picked: its name, or `None` for the default font.
pub struct FontChosen(pub Option<String>);

/// The names of the fonts installed, sorted, without the system's hidden ones and without twins.
pub fn installed(cx: &App) -> Vec<String> {
    let mut fonts = cx.text_system().all_font_names();
    fonts
        .retain(|name| !name.trim().is_empty() && !name.starts_with('.') && !name.starts_with('@'));
    fonts.sort_by_key(|name| name.to_lowercase());
    fonts.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    fonts
}

/// The indexes of the fonts whose name has every word of `query`, in order.
pub fn matching(fonts: &[String], query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    fonts
        .iter()
        .enumerate()
        .filter(|(_, name)| {
            let name = name.to_lowercase();
            words.iter().all(|word| name.contains(word.as_str()))
        })
        .map(|(index, _)| index)
        .collect()
}

pub struct FontPicker {
    fonts: Rc<Vec<String>>,
    filter: Entity<InputState>,
    /// The fonts that pass the search, as indexes into `fonts`.
    shown: Vec<usize>,
    current: Option<String>,
    /// What the first row is called: the font used when none is picked.
    default_name: SharedString,
    scroll: UniformListScrollHandle,
    _subscription: Subscription,
}

impl EventEmitter<FontChosen> for FontPicker {}

impl FontPicker {
    /// A picker showing `current` as chosen (`None` is the default font, called `default_name`).
    pub fn new(
        current: Option<String>,
        default_name: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fonts = Rc::new(installed(cx));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search the fonts"));
        let subscription = cx.subscribe(
            &filter,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Change => this.refilter(cx),
                InputEvent::PressEnter { .. } => {
                    if let Some(first) = this.shown.first() {
                        let name = this.fonts[*first].clone();
                        this.choose(Some(name), cx);
                    }
                }
                _ => {}
            },
        );
        let mut this = Self {
            shown: (0..fonts.len()).collect(),
            fonts,
            filter,
            current,
            default_name: default_name.into(),
            scroll: UniformListScrollHandle::new(),
            _subscription: subscription,
        };
        this.scroll_to_current();
        this
    }

    /// The font shown as chosen, when it is changed from outside.
    pub fn set_current(&mut self, current: Option<String>, cx: &mut Context<Self>) {
        if self.current != current {
            self.current = current;
            cx.notify();
        }
    }

    fn query(&self, cx: &App) -> String {
        self.filter.read(cx).value().to_string()
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.shown = matching(&self.fonts, &query);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// Brings the chosen font into view, if it is in the list.
    fn scroll_to_current(&mut self) {
        let Some(current) = &self.current else {
            return;
        };
        if let Some(row) = self
            .shown
            .iter()
            .position(|index| self.fonts[*index].eq_ignore_ascii_case(current))
        {
            // The first row of the list is the default one.
            self.scroll.scroll_to_item(row + 1, ScrollStrategy::Center);
        }
    }

    fn choose(&mut self, font: Option<String>, cx: &mut Context<Self>) {
        self.current = font.clone();
        cx.emit(FontChosen(font));
        cx.notify();
    }

    fn row(&self, row: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let this = cx.entity();
        let base = div()
            .id(("font-row", row))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(tokens::height::CONTROL))
            .px_2()
            .cursor_pointer()
            .text_size(px(tokens::text::EMPHASIS));
        if row == 0 {
            let chosen = self.current.is_none();
            return base
                .text_color(if chosen {
                    theme::fg()
                } else {
                    theme::muted_fg()
                })
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .hover(|s| s.bg(theme::surface_hover()))
                .on_click(move |_, _window, cx| this.update(cx, |e, cx| e.choose(None, cx)))
                .child(SharedString::from(format!(
                    "Default ({})",
                    self.default_name
                )))
                .children(chosen.then(|| icon::small(IconName::Check, theme::accent())))
                .into_any_element();
        }
        let name = self.fonts[self.shown[row - 1]].clone();
        let chosen = self
            .current
            .as_ref()
            .is_some_and(|current| current.eq_ignore_ascii_case(&name));
        base.font_family(SharedString::from(name.clone()))
            .text_color(if chosen {
                theme::fg()
            } else {
                theme::muted_fg()
            })
            .when(chosen, |el| el.bg(theme::accent_selected()))
            .hover(|s| s.bg(theme::surface_hover()))
            .on_click({
                let name = name.clone();
                move |_, _window, cx| {
                    let name = name.clone();
                    this.update(cx, |e, cx| e.choose(Some(name), cx));
                }
            })
            .child(SharedString::from(name))
            .children(chosen.then(|| icon::small(IconName::Check, theme::accent())))
            .into_any_element()
    }
}

impl Render for FontPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let missing = self.current.as_ref().filter(|current| {
            !self
                .fonts
                .iter()
                .any(|name| name.eq_ignore_ascii_case(current))
        });
        let count = self.shown.len();
        // The default row comes first, and only while nothing is searched.
        let searching = !self.query(cx).trim().is_empty();
        let rows = count + 1;
        let list = uniform_list(
            "font-picker-list",
            rows,
            cx.processor(move |this, range: Range<usize>, _window, cx| {
                range
                    .filter(|row| *row <= this.shown.len())
                    .map(|row| this.row(row, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .h(px(220.));
        let summary = if searching && count == 0 {
            "No font matches.".to_owned()
        } else if searching {
            format!("{count} of {} fonts", self.fonts.len())
        } else {
            format!("{} fonts installed", self.fonts.len())
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&self.filter).small())
            .child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::border_subtle())
                    .overflow_hidden()
                    // The list scrolls by itself: the wheel stops here, so a panel around it stays put.
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(list),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_size(px(tokens::text::SMALL))
                    .text_color(theme::muted_fg())
                    .child(SharedString::from(summary))
                    .children(
                        missing.map(|name| {
                            SharedString::from(format!("{name} is not installed here"))
                        }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        [
            "Arial",
            "Arial Black",
            "DejaVu Sans Mono",
            "Inter",
            "Noto Serif",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    #[test]
    fn an_empty_search_keeps_every_font() {
        assert_eq!(matching(&names(), "  "), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn every_word_of_a_search_must_be_in_the_name() {
        assert_eq!(matching(&names(), "arial black"), vec![1]);
        assert_eq!(matching(&names(), "SANS mono"), vec![2]);
        assert_eq!(matching(&names(), "serif noto"), vec![4]);
        assert!(matching(&names(), "comic").is_empty());
    }
}
