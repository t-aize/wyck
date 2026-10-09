//! Wyck's widget kit: every control, menu, dialog and color of the desktop app, in one look.
//!
//! A screen builds its UI from these pieces and reads its colors from [`theme`], so a change of
//! size, spacing or color made here reaches every screen at once. Screens never configure a
//! gpui-kit component or spell out a color themselves.

pub mod anim;
pub mod button;
pub mod color_picker;
pub mod confirm;
pub mod controls;
pub mod field;
pub mod focus;
pub mod font_picker;
pub mod form;
pub mod icon;
pub mod layout;
pub mod menu;
pub mod modal;
pub mod number;
pub mod text_input;
pub mod theme;
pub mod toast;
pub mod tokens;
pub mod window_bar;
