//! Buttons. They are gpui-kit's, restyled through [`crate::theme::apply`], so every button of
//! the app takes one of these shapes instead of configuring a `Button` itself.
//!
//! There are four sizes and a screen never picks a gpui one by hand. Their heights are the
//! control heights of [`crate::tokens::height`], so they scale with the interface and
//! match the fields:
//!
//! | [`Size`] | Height | Where |
//! |---|---|---|
//! | `Sm` | 24 | a dense strip or a table row (the editor toolbar, a list row) |
//! | `Md` | 28 | everything else: forms, dialogs, menus, panels, footers |
//! | `Rail` | 30 | a tool of the drawing rail |
//! | `Lg` | 34 | buy and sell in the ticket, the main button of the sign-in modal |
//!
//! An icon-only button is as wide as it is tall.

use gpui::prelude::*;
use gpui::{App, ClickEvent, ElementId, SharedString, Window, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
pub use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants;

use crate::{theme, tokens};

/// How big a button is. See the module docs for what each is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// Dense strips and table rows.
    Sm,
    /// The default.
    Md,
    /// A tool of a vertical rail.
    Rail,
    /// The one action of a screen.
    Lg,
}

/// `button` at `size`: the one place that maps a size to gpui-kit's. gpui-kit's own heights
/// (20, 24, 32) are not the ones of the kit, so the height is set here after the size has set the
/// text and the padding. `min_w` keeps an icon-only button square.
fn sized(button: Button, size: Size) -> Button {
    let (button, height) = match size {
        Size::Sm => (button.small(), tokens::height::compact()),
        Size::Md => (button.small(), tokens::height::control()),
        Size::Rail => (button.small(), tokens::height::rail()),
        Size::Lg => (button.large(), tokens::height::large()),
    };
    button.h(px(height)).min_w(px(height))
}

/// The color of a [`primary`] button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// The accent: the action to take.
    Accent,
    /// The danger color: what disconnects or deletes.
    Danger,
}

/// The filled button that fills the width it is given: the one action of a small panel ("Try
/// again", "Sign in again", "Disconnect"). At the large size it is the main button of the
/// sign-in screens.
pub fn primary(
    id: impl Into<ElementId>,
    fill: Fill,
    size: Size,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    let button = Button::new(id).primary();
    let button = match fill {
        Fill::Accent => button,
        Fill::Danger => button.danger(),
    };
    sized(button, size)
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

/// The button of a footer that confirms something that destroys: [`action`] filled in the danger
/// color.
pub(crate) fn action_danger(
    id: impl Into<ElementId>,
    label: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    sized(Button::new(id).primary().danger(), Size::Md)
        .cursor_pointer()
        .label(label)
        .on_click(move |_, window, cx| on_click(window, cx))
}

/// A button that is only an icon, with `tip` as its tooltip and its name for a screen reader.
/// `Size::Sm` is the one for a dense strip or a table row.
pub fn icon(id: impl Into<ElementId>, size: Size, glyph: IconName, tip: &'static str) -> Button {
    sized(Button::new(id).ghost(), size)
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

/// A ghost button of the rail size, for a vertical rail of tools that must be easy to hit (the
/// drawing tools). Chain `.icon(..)`, `.tooltip(..)` and `.on_click(..)` on it.
pub fn rail(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id).ghost(), Size::Rail).cursor_pointer()
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

pub fn outlined(id: impl Into<ElementId>) -> Button {
    sized(Button::new(id).outline(), Size::Md).cursor_pointer()
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
