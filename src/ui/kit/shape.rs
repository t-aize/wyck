//! The corners and the shadow of what is drawn by hand: three radii, all small, and one shadow for
//! what floats.
//!
//! The interface is square and flat: a control has a 2 px corner, a card or a menu 3 px, a dialog
//! 4 px, and only what floats over something (a menu, a bar over the chart) has a shadow, a light
//! one. A circle (a dot, a switch) is `rounded_full` and the only other shape. A screen takes its
//! corners from here; `tests/design_system.rs` fails on a `rounded_md`, `rounded_lg` or the like
//! written outside the kit.

use gpui::{Styled, px};

use super::tokens;

/// Corners and shadow for any gpui element.
pub trait Shape: Styled + Sized {
    /// A button, a field, a tab, a swatch.
    fn round_control(self) -> Self {
        self.rounded(px(tokens::radius::control()))
    }

    /// A card, a menu, a popover, a bar over the chart.
    fn round_card(self) -> Self {
        self.rounded(px(tokens::radius::card()))
    }

    /// A dialog.
    fn round_dialog(self) -> Self {
        self.rounded(px(tokens::radius::dialog()))
    }

    /// The light shadow of what floats over something else.
    fn floats(self) -> Self {
        self.shadow_sm()
    }
}

impl<T: Styled + Sized> Shape for T {}
