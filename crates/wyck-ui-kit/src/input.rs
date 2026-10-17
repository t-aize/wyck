//! Text and number fields, at the control height of the kit (the height of a medium button).
//!
//! Screens take the field types from here and build them with [`text`], [`large`], [`dense`] and
//! [`number`], so no screen picks a size of its own.

use gpui::{Entity, Styled as _, px};

use crate::tokens;

use gpui_kit::component::Sizable;
pub use gpui_kit::component::highlighter::{Diagnostic, DiagnosticSeverity};
pub use gpui_kit::component::input::language_config::LanguageConfig;
pub use gpui_kit::component::input::*;

/// A single line text field.
pub fn text(state: &Entity<InputState>) -> Input {
    Input::new(state).small().h(px(tokens::height::control()))
}

/// A single line text field for a page with few fields, such as the sign-in: as tall as a row of
/// a list (see [`tokens::height::large`]) so it is easy to aim at.
pub fn large(state: &Entity<InputState>) -> Input {
    Input::new(state).h(px(tokens::height::large()))
}

/// A single line text field for a dense strip.
pub fn dense(state: &Entity<InputState>) -> Input {
    Input::new(state).small().h(px(tokens::height::compact()))
}

/// A number field with its stepper buttons.
pub fn number(state: &Entity<InputState>) -> NumberInput {
    NumberInput::new(state)
        .small()
        .h(px(tokens::height::control()))
}
