//! Buttons. They are gpui-kit's, restyled through [`crate::theme::apply`], so every button of
//! the app takes one of these shapes instead of configuring a `Button` itself.

use gpui::prelude::*;
use gpui::{App, ClickEvent, ElementId, SharedString, Window, div};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants};

/// The "Back" button in the top-left corner of a [`crate::layout::screen`].
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
