//! The app's bundled files: the UI font and the icon set, compiled into the binary so it never
//! depends on anything being installed on the machine it runs on.
//!
//! Icons come from `gpui-kit-assets`, which embeds the whole Lucide set: reference them with
//! `gpui_kit::assets::IconName` instead of file paths.

/// Inter (SIL Open Font License; see `assets/fonts/LICENSE.txt`), registered once at startup in
/// [`super::run`] and used everywhere as the app's font.
pub const FONT: &[u8] = include_bytes!("../../assets/fonts/Inter.ttf");

pub use gpui_kit::assets::AllAssets as Assets;
