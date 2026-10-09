//! The question the app asks before it does something it cannot take back: send an order, close a
//! position. It is a small dialog in the modal (see [`crate::ui::kit::modal`]), with the same look as the
//! settings panels.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, Context, SharedString, Window, div, px};
use gpui_kit::assets::IconName;

use crate::ui::kit::form::{self, Head};
use crate::ui::kit::{button, modal, theme};

/// The size of the dialog.
const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 260.0;

/// What to run once the user confirms.
type Then = Rc<dyn Fn(&mut Window, &mut App)>;

/// Asks for a confirmation before `then` runs. Cancel, Escape and the close button run nothing.
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    then: impl Fn(&mut Window, &mut App) + 'static,
) {
    let title: SharedString = title.into();
    let label = title.clone();
    let view = cx.new(|_| Confirm {
        title,
        text: text.into(),
        details: Vec::new(),
        warnings: Vec::new(),
        label: "Confirm",
        then: Rc::new(then),
    });
    modal::open_over(
        view,
        modal::Options::new(WIDTH, HEIGHT).label(label),
        window,
        cx,
    );
}

/// What a detailed confirmation shows besides its headline.
#[derive(Default)]
pub struct Details {
    /// Rows of a label and a value: the size, the price, what is at stake.
    pub rows: Vec<(SharedString, SharedString)>,
    /// Things worth a second look, in the color of a warning.
    pub warnings: Vec<SharedString>,
    /// A badge next to the headline, such as `LIVE`, drawn as a warning.
    pub badge: Option<SharedString>,
    /// The words of the confirming button; `Confirm` when empty.
    pub label: Option<&'static str>,
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
    let height = HEIGHT + 22.0 * details.rows.len() as f32 + 34.0 * details.warnings.len() as f32;
    let text: SharedString = match &details.badge {
        Some(badge) => format!("[{badge}] {}", text.into()).into(),
        None => text.into(),
    };
    let title: SharedString = title.into();
    let label = title.clone();
    let view = cx.new(|_| Confirm {
        title,
        text,
        label: details.label.unwrap_or("Confirm"),
        details: details.rows,
        warnings: details.warnings,
        then: Rc::new(then),
    });
    modal::open_over(
        view,
        modal::Options::new(WIDTH, height.min(640.0)).label(label),
        window,
        cx,
    );
}

struct Confirm {
    title: SharedString,
    text: SharedString,
    details: Vec<(SharedString, SharedString)>,
    warnings: Vec<SharedString>,
    label: &'static str,
    then: Then,
}

impl Render for Confirm {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let then = self.then.clone();
        let head = Head {
            icon: IconName::CircleQuestionMark,
            title: self.title.clone(),
            subtitle: "This asks for your confirmation".into(),
        };
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .text_size(px(crate::ui::kit::tokens::text::title()))
                .text_color(theme::fg())
                .child(self.text.clone()),
        );
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
                        .text_size(px(crate::ui::kit::tokens::text::body()))
                        .child(div().text_color(theme::muted_fg()).child(label.clone()))
                        .child(div().text_color(theme::fg()).child(value.clone())),
                );
            }
            body = body.child(rows);
        }
        for warning in &self.warnings {
            body = body.child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(theme::amber_bg())
                    .border_1()
                    .border_color(theme::amber())
                    .text_size(px(crate::ui::kit::tokens::text::body()))
                    .text_color(theme::fg())
                    .child(warning.clone()),
            );
        }
        let footer = form::footer(
            Vec::new(),
            vec![
                button::action("confirm-cancel", "Cancel", None, false, modal::close)
                    .into_any_element(),
                button::action("confirm-ok", self.label, None, true, move |window, cx| {
                    modal::close(window, cx);
                    then(window, cx);
                })
                .into_any_element(),
            ],
        );
        form::dialog(head, modal::dismiss, body, footer)
    }
}
