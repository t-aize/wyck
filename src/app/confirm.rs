//! The question the app asks before it does something it cannot take back: send an order, close a
//! position. It is a small dialog in the modal (see [`super::modal`]), with the same look as the
//! settings panels.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, Context, SharedString, Window, div, px};
use gpui_kit::assets::IconName;

use super::settings_ui::{self as ui, Head};
use super::{modal, theme};

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
    let view = cx.new(|_| Confirm {
        title: title.into(),
        text: text.into(),
        then: Rc::new(then),
    });
    modal::open(view, modal::Options::new(WIDTH, HEIGHT), window, cx);
}

struct Confirm {
    title: SharedString,
    text: SharedString,
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
        let body = div()
            .text_size(px(14.))
            .text_color(theme::fg())
            .child(self.text.clone());
        let footer = ui::footer(
            Vec::new(),
            vec![
                ui::action("confirm-cancel", "Cancel", None, false, modal::close)
                    .into_any_element(),
                ui::action("confirm-ok", "Confirm", None, true, move |window, cx| {
                    modal::close(window, cx);
                    then(window, cx);
                })
                .into_any_element(),
            ],
        );
        ui::dialog(head, modal::dismiss, body, footer)
    }
}
