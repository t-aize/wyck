//! Buttons. They are gpui-kit's, restyled through [`crate::ui::kit::theme::apply`], so every button of
//! the app takes one of these shapes instead of configuring a `Button` itself.
//!
//! There are three sizes and a screen never picks a gpui one by hand:
//!
//! | [`Size`] | Height | Where |
//! |---|---|---|
//! | `Sm` | 20 | a dense strip or a table row (the editor toolbar, a list row) |
//! | `Md` | 24 | everything else: forms, dialogs, menus, panels, footers |
//! | `Lg` | 32 | buy and sell in the ticket, the main button of the sign-in modal |
//!
//! The fields of the kit are `Md` high too, so a button and a field side by side line up.

use gpui::prelude::*;
use gpui::{App, ClickEvent, ElementId, SharedString, Window};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
pub use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants;

use crate::ui::kit::theme;

/// How big a button is. See the module docs for what each is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// Dense strips and table rows.
    Sm,
    /// The default.
    Md,
    /// The one action of a screen.
    Lg,
}

/// `button` at `size`: the one place that maps a size to gpui-kit's.
fn sized(button: Button, size: Size) -> Button {
    match size {
        Size::Sm => button.xsmall().compact(),
        Size::Md => button.small(),
        Size::Lg => button.large(),
    }
}

/// The filled, accent-colored button that fills the width it is given: the one action of a small
/// panel ("Try again", "Sign in again").
pub fn primary(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    sized(Button::new(id).primary(), Size::Md)
        .w_full()
        .label(label)
        .cursor_pointer()
        .on_click(on_click)
}

/// [`primary`] at the large size: the main button of the sign-in modal, nothing else.
pub fn hero(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    sized(Button::new(id).primary(), Size::Lg)
        .w_full()
        .label(label)
        .cursor_pointer()
        .on_click(on_click)
}

/// A button of a footer, with an optional icon. `primary` is the one that confirms.
pub fn action(
    id: impl Into<ElementId>,
    label: &'static str,
    icon: Option<IconName>,
    primary: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    let button = sized(Button::new(id), Size::Md)
        .cursor_pointer()
        .label(label)
        .on_click(move |_, window, cx| on_click(window, cx));
    let button = if primary {
        button.primary()
    } else {
        button.ghost()
    };
    match icon {
        Some(icon) => button.icon(icon),
        None => button,
    }
}

/// A button that is only an icon, with `tip` as its tooltip and its name for a screen reader.
/// [`icon_dense`] is the one for a dense strip.
pub fn icon(id: impl Into<ElementId>, glyph: IconName, tip: &'static str) -> Button {
    sized(Button::new(id).ghost(), Size::Md)
        .icon(glyph)
        .tooltip(tip)
        .accessibility_label(tip)
        .cursor_pointer()
}

/// A ghost button: a tool in a bar, a text button of a panel. Chain `.icon(..)`, `.label(..)`,
/// `.tooltip(..)` and `.on_click(..)` on it.
pub fn quiet(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id).ghost(), Size::Md).cursor_pointer()
}

/// A ghost button for a dense strip or a table row (the editor toolbar, a list row).
pub fn dense(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id).ghost(), Size::Sm).cursor_pointer()
}

/// The filled button that confirms.
pub fn accent(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id).primary(), Size::Md).cursor_pointer()
}

/// A button that destroys something.
pub fn danger(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id).danger(), Size::Md).cursor_pointer()
}

/// An outlined button.
pub fn outlined(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id).outline(), Size::Md).cursor_pointer()
}

/// A button with the kit's default look.
pub fn standard(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id), Size::Md).cursor_pointer()
}

/// The large button that sends an order: green for a buy, red for a sell.
pub fn trade(id: impl Into<ElementId>, buy: bool) -> Button {
    sized(Button::new(id), Size::Lg)
        .cursor_pointer()
        .bg(if buy {
            theme::chart_up()
        } else {
            theme::chart_down()
        })
        .text_color(theme::bg())
}

/// The icon-only button of a dense strip or a panel's own bar.
pub fn icon_dense(id: impl Into<ElementId>, glyph: IconName, tip: &'static str) -> Button {
    sized(Button::new(id).ghost(), Size::Sm)
        .icon(glyph)
        .tooltip(tip)
        .accessibility_label(tip)
        .cursor_pointer()
}

/// A wide button that disconnects or deletes: [`primary`] in the danger color.
pub fn wide_danger(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    primary(id, label, on_click).danger()
}
