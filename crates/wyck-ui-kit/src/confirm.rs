//! The question the app asks before it does something it cannot take back: send an order, close a
//! position. It is a small dialog in the modal (see [`crate::modal`]), with the same look as the
//! settings panels.
//!
//! The dialog says how serious the question is with a [`Tone`]: the icon, the colors of the header
//! and of the words, and the confirming button follow it. Its height follows the words, so a short
//! question does not float in an empty box.

use std::rc::Rc;
use wyck_core::math::{count_f32, f32_to_usize};

use gpui::prelude::*;
use gpui::{App, Context, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::StyledExt as _;

use crate::form::{self, Head, Tint};
use crate::{button, modal, theme, tokens};

/// The width of the dialog.
const WIDTH: f32 = 440.0;
/// The most it grows, and the least it shrinks to.
const MAX_HEIGHT: f32 = 640.0;
const MIN_HEIGHT: f32 = 200.0;
/// The header and the footer, and the padding around the body, as `form::dialog` draws them.
const CHROME: f32 = 56.0 + 56.0 + 32.0;
/// What a box around the words (padding and border) adds, in each direction.
const BOX_PAD: f32 = 26.0;

/// What to run once the user confirms.
type Then = Rc<dyn Fn(&mut Window, &mut App)>;

/// How serious the question is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tone {
    /// A plain question.
    #[default]
    Info,
    /// Something worth a second look: real money, an action that skips a check.
    Caution,
    /// Something that destroys or is hard to take back; the confirming button is red.
    Danger,
}

impl Tone {
    fn icon(self) -> IconName {
        match self {
            Self::Info => IconName::CircleQuestionMark,
            Self::Caution => IconName::TriangleAlert,
            Self::Danger => IconName::OctagonAlert,
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::Info => "This asks for your confirmation",
            Self::Caution => "Worth a second look",
            Self::Danger => "This is hard to take back",
        }
    }

    /// The main color of the tone.
    fn color(self) -> gpui::Rgba {
        match self {
            Self::Info => theme::accent(),
            Self::Caution => theme::amber(),
            Self::Danger => theme::destructive(),
        }
    }

    /// The soft color behind it.
    fn soft(self) -> gpui::Rgba {
        match self {
            Self::Info => theme::accent_alpha(0.15),
            Self::Caution => theme::amber_bg(),
            Self::Danger => theme::destructive_bg(),
        }
    }

    fn tint(self) -> Tint {
        Tint {
            glyph: self.color(),
            tile: self.soft(),
        }
    }
}

/// Asks for a confirmation before `then` runs. Cancel, Escape and the close button run nothing.
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    then: impl Fn(&mut Window, &mut App) + 'static,
) {
    confirm_as(window, cx, Tone::Info, title, text, then);
}

/// [`confirm`] with a tone: [`Tone::Danger`] for what destroys, [`Tone::Caution`] for what risks.
pub fn confirm_as(
    window: &mut Window,
    cx: &mut App,
    tone: Tone,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    then: impl Fn(&mut Window, &mut App) + 'static,
) {
    open(
        window,
        cx,
        Confirm {
            tone,
            title: title.into(),
            text: text.into(),
            badge: None,
            details: Vec::new(),
            warnings: Vec::new(),
            label: "Confirm",
            then: Rc::new(then),
        },
    );
}

/// What a detailed confirmation shows besides its headline.
#[derive(Default)]
pub struct Details {
    /// Rows of a label and a value: the size, the price, what is at stake.
    pub rows: Vec<(SharedString, SharedString)>,
    /// Things worth a second look, in the color of a warning.
    pub warnings: Vec<SharedString>,
    /// A badge above the words, such as `LIVE`, drawn as a warning.
    pub badge: Option<SharedString>,
    /// The words of the confirming button; `Confirm` when empty.
    pub label: Option<&'static str>,
    /// How serious it is; a badge or a warning makes it at least a caution.
    pub tone: Tone,
}

/// [`confirm`] with rows of details and warnings under the headline, for an order: what it is,
/// what it risks, and what is off about it.
pub fn confirm_details(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    details: Details,
    then: impl Fn(&mut Window, &mut App) + 'static,
) {
    let tone = if details.tone == Tone::Info
        && (details.badge.is_some() || !details.warnings.is_empty())
    {
        Tone::Caution
    } else {
        details.tone
    };
    open(
        window,
        cx,
        Confirm {
            tone,
            title: title.into(),
            text: text.into(),
            badge: details.badge,
            label: details.label.unwrap_or("Confirm"),
            details: details.rows,
            warnings: details.warnings,
            then: Rc::new(then),
        },
    );
}

fn open(window: &mut Window, cx: &mut App, confirm: Confirm) {
    let height = confirm.height();
    let label = confirm.title.clone();
    let view = cx.new(|_| confirm);
    modal::open_over(
        view,
        modal::Options::new(WIDTH, height).label(label),
        window,
        cx,
    );
}

struct Confirm {
    tone: Tone,
    title: SharedString,
    text: SharedString,
    badge: Option<SharedString>,
    details: Vec<(SharedString, SharedString)>,
    warnings: Vec<SharedString>,
    label: &'static str,
    then: Then,
}

impl Confirm {
    /// Whether the words sit in a tinted box.
    fn boxed(&self) -> bool {
        self.tone != Tone::Info
    }

    /// The height that holds the words, the rows and the warnings, estimated from their length.
    fn height(&self) -> f32 {
        let scale = tokens::scaled(1.0);
        let pad = if self.boxed() { BOX_PAD } else { 0.0 };
        let glyph = tokens::text::title() * 0.58;
        let per_line = f32_to_usize(((WIDTH - 32.0 - pad) / glyph).floor().max(16.0));
        let lines = self.text.chars().count().div_ceil(per_line).max(1);
        let mut body = count_f32(lines) * tokens::text::title() * 1.5 + pad;
        if self.badge.is_some() {
            body += 30.0 * scale;
        }
        if !self.details.is_empty() {
            body += 24.0 * scale + 22.0 * scale * count_f32(self.details.len());
        }
        body += 40.0 * scale * count_f32(self.warnings.len());
        // The gaps between the parts.
        let parts = 1
            + usize::from(self.badge.is_some())
            + usize::from(!self.details.is_empty())
            + self.warnings.len();
        body += 8.0 * count_f32(parts.saturating_sub(1));
        (CHROME + body + 8.0).clamp(MIN_HEIGHT, MAX_HEIGHT)
    }
}

impl Render for Confirm {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let then = Rc::clone(&self.then);
        let tone = self.tone;
        let head = Head {
            icon: tone.icon(),
            title: self.title.clone(),
            subtitle: tone.subtitle().into(),
        };
        let mut body = div().flex().flex_col().gap_2();
        if let Some(badge) = &self.badge {
            body = body.child(
                div().flex().flex_row().child(
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(theme::amber_bg())
                        .border_1()
                        .border_color(theme::amber())
                        .text_size(px(tokens::text::small()))
                        .font_semibold()
                        .text_color(theme::amber())
                        .child(badge.clone()),
                ),
            );
        }
        let words = div()
            .text_size(px(tokens::text::title()))
            .text_color(theme::fg())
            .child(self.text.clone());
        body = body.child(if self.boxed() {
            words
                .p_3()
                .rounded_lg()
                .bg(tone.soft())
                .border_1()
                .border_color(gpui::Rgba {
                    a: 0.45,
                    ..tone.color()
                })
        } else {
            words
        });
        if !self.details.is_empty() {
            let mut rows = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_lg()
                .border_1()
                .border_color(theme::border_subtle())
                .bg(theme::surface());
            for (label, value) in &self.details {
                rows = rows.child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .gap_2()
                        .text_size(px(tokens::text::body()))
                        .child(div().text_color(theme::muted_fg()).child(label.clone()))
                        .child(
                            div()
                                .font_semibold()
                                .text_color(theme::fg())
                                .child(value.clone()),
                        ),
                );
            }
            body = body.child(rows);
        }
        for warning in &self.warnings {
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .bg(theme::amber_bg())
                    .border_1()
                    .border_color(theme::amber())
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::fg())
                    .child(crate::icon::tinted(
                        IconName::TriangleAlert,
                        tokens::text::title(),
                        theme::amber(),
                    ))
                    .child(div().flex_1().min_w_0().child(warning.clone())),
            );
        }
        let run = move |window: &mut Window, cx: &mut App| {
            modal::close(window, cx);
            then(window, cx);
        };
        let confirm = if tone == Tone::Danger {
            button::action_danger("confirm-ok", self.label, run)
        } else {
            button::action("confirm-ok", self.label, None, true, run)
        };
        let footer = form::footer(
            Vec::new(),
            vec![
                button::action("confirm-cancel", "Cancel", None, false, modal::close)
                    .into_any_element(),
                confirm.into_any_element(),
            ],
        );
        form::dialog_tinted(head, Some(tone.tint()), modal::dismiss, body, footer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(tone: Tone, text: &str) -> Confirm {
        Confirm {
            tone,
            title: "Title".into(),
            text: text.to_owned().into(),
            badge: None,
            details: Vec::new(),
            warnings: Vec::new(),
            label: "Confirm",
            then: Rc::new(|_, _| {}),
        }
    }

    #[test]
    #[expect(
        clippy::float_cmp_const,
        reason = "the height is clamped to exactly the limit, a constant returned as is"
    )]
    fn a_short_question_gets_the_smallest_box() {
        assert_eq!(with(Tone::Info, "Sure?").height(), MIN_HEIGHT);
    }

    #[test]
    #[expect(
        clippy::float_cmp_const,
        reason = "the height is clamped to exactly the limit, a constant returned as is"
    )]
    fn the_box_grows_with_the_words_and_stops_at_the_maximum() {
        let short = with(Tone::Danger, &"word ".repeat(10)).height();
        let long = with(Tone::Danger, &"word ".repeat(60)).height();
        assert!(long > short);
        assert_eq!(
            with(Tone::Danger, &"word ".repeat(2000)).height(),
            MAX_HEIGHT
        );
    }

    #[test]
    fn a_tinted_box_around_the_words_needs_more_room() {
        let text = "word ".repeat(40);
        assert!(with(Tone::Caution, &text).height() > with(Tone::Info, &text).height());
    }
}
