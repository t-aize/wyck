//! Keyboard access for the controls the app draws itself.
//!
//! A control made of a `div` with a click handler is invisible to the keyboard until it is made a
//! stop of the Tab key. [`Keyboard::keyboard`] does that, so Tab reaches it, Enter or Space clicks
//! it (gpui does this for a focused element), and a ring shows where the keyboard is. The ring is
//! drawn only when the keyboard put the focus there, not after a click with the mouse (WCAG 2.4.7
//! and 2.4.11: a visible focus that stands out by at least 3:1).

use gpui::{InteractiveElement, Styled};

use crate::theme;

/// Makes an element a stop of the Tab key with a visible focus.
pub trait Keyboard: InteractiveElement + Sized {
    fn keyboard(self) -> Self {
        self.tab_index(0).focus_visible(|style| {
            // The edge for a control that has one, and a wash for one that has none.
            style
                .border_color(theme::accent())
                .bg(theme::accent_alpha(0.22))
        })
    }
}

impl<T: InteractiveElement + Sized> Keyboard for T {}
