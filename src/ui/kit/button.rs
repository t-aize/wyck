//! Buttons. They are gpui-kit's, restyled through [`crate::ui::kit::theme::apply`], so every button of
//! the app takes one of these shapes instead of configuring a `Button` itself.

use gpui::prelude::*;
use gpui::{App, ClickEvent, ElementId, SharedString, Window, div};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
pub use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants;

use crate::ui::kit::theme;

/// The "Back" button in the top-left corner of a [`crate::ui::kit::layout::screen`].
pub fn back(
    id: impl Into<gpui::ElementId>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div().absolute().top_5().left_5().child(
        Button::new(id)
            .ghost()
            .icon(IconName::ArrowLeft)
            .label("Back")
            .cursor_pointer()
            .tooltip("Go back")
            .on_click(on_click),
    )
}

/// The filled, accent-colored call-to-action button.
pub fn primary(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .primary()
        .large()
        .w_full()
        .label(label)
        .cursor_pointer()
        .on_click(on_click)
}

/// A borderless, muted text button (e.g. "Cancel", "Use a different cTrader ID").
pub fn ghost(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .label(label)
        .cursor_pointer()
        .on_click(on_click)
}

/// An outlined, secondary button (e.g. "Disconnect").
pub fn secondary(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .outline()
        .large()
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
    let button = Button::new(id)
        .cursor_pointer()
        .small()
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

/// A button that is only an icon, with `tip` as its tooltip and its name for a screen reader. It
/// is small; a dense strip makes it `.xsmall()`.
pub fn icon(id: impl Into<ElementId>, glyph: IconName, tip: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .icon(glyph)
        .tooltip(tip)
        .accessibility_label(tip)
        .cursor_pointer()
}

/// A ghost button in the standard small size: a tool in a bar, a text button of a panel. Chain
/// `.icon(..)`, `.label(..)`, `.tooltip(..)` and `.on_click(..)` on it.
pub fn quiet(id: impl Into<ElementId>) -> Button {
    Button::new(id).ghost().small().cursor_pointer()
}

/// A ghost button for a dense strip or a table row (the editor toolbar, a list row).
pub fn dense(id: impl Into<ElementId>) -> Button {
    Button::new(id).ghost().xsmall().compact().cursor_pointer()
}

/// The filled button that confirms, in the standard small size.
pub fn accent(id: impl Into<ElementId>) -> Button {
    Button::new(id).primary().small().cursor_pointer()
}

/// A button that destroys something, in the standard small size.
pub fn danger(id: impl Into<ElementId>) -> Button {
    Button::new(id).danger().small().cursor_pointer()
}

/// An outlined button in the standard small size.
pub fn outlined(id: impl Into<ElementId>) -> Button {
    Button::new(id).outline().small().cursor_pointer()
}

/// A button with the kit's default look in the standard small size.
pub fn standard(id: impl Into<ElementId>) -> Button {
    Button::new(id).small().cursor_pointer()
}

/// The large button that sends an order: green for a buy, red for a sell.
pub fn trade(id: impl Into<ElementId>, buy: bool) -> Button {
    Button::new(id)
        .large()
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
    icon(id, glyph, tip).xsmall()
}

/// A wide button that disconnects or deletes: [`primary`] in the danger color.
pub fn wide_danger(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    primary(id, label, on_click).danger()
}
