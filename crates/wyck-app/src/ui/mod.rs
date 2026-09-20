//! The windows of the application, drawn with GPUI.
//!
//! Everything here is drawing and wiring. What a screen says, when it changes and what a click
//! does are decided by the plain modules of this crate ([`crate::flow`], [`crate::presentation`],
//! [`crate::messages`], [`crate::controller`]), which are tested without a window. A function in
//! this module reads that state and returns elements, or calls one of those modules.
//!
//! | Module | Role |
//! |---|---|
//! | [`theme`] | The palette, the fonts, and the dark theme installed in GPUI |
//! | [`assets`] | The icons and the Geist fonts, compiled into the binary |
//! | [`motion`] | Durations, curves and directions: how screens change and controls react |
//! | [`widgets`] | Small drawing helpers: card, buttons, rows, badges, spinner |
//! | [`titlebar`] | The custom title bar and its window buttons |
//! | [`app_view`] | The root view: title bar, banners, current screen, toasts |
//! | [`screens`] | One function per screen of the connection flow, and the connected screen |
//! | [`preview`] | Names for the screens, to open the application on any of them while designing |
//!
//! # Looks
//!
//! The design is a dark neutral palette with one accent for success and one for danger (see
//! [`theme`]), in Geist, everywhere. Window chrome is drawn by the application, not by the
//! operating system: the title bar, its three buttons and the drag area are in [`titlebar`].
//! The account kind is never drawn as a demo unless the account is known to be one (see
//! [`crate::presentation::kind_badge`]).

pub mod app_view;
pub mod assets;
pub mod motion;
pub mod preview;
pub mod screens;
pub mod theme;
pub mod titlebar;
pub mod widgets;

pub use app_view::AppView;
