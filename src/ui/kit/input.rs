//! Text and number fields, in the one size the app uses.
//!
//! Screens take the field types from here and build them with [`text`], [`dense`] and [`number`],
//! so no screen picks a size of its own.

use gpui::Entity;

use gpui_kit::component::Sizable;
pub use gpui_kit::component::highlighter::{Diagnostic, DiagnosticSeverity};
pub use gpui_kit::component::input::language_config::LanguageConfig;
pub use gpui_kit::component::input::*;

/// A single line text field.
pub fn text(state: &Entity<InputState>) -> Input {
    Input::new(state).small()
}

/// A single line text field for a dense strip.
pub fn dense(state: &Entity<InputState>) -> Input {
    Input::new(state).xsmall()
}

/// A number field with its stepper buttons.
pub fn number(state: &Entity<InputState>) -> NumberInput {
    NumberInput::new(state).small()
}
