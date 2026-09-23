//! The app's bundled files: the UI font, the symbol marks (flags and logos) and the icon set,
//! compiled into the binary so it never depends on anything being installed on the machine it
//! runs on.
//!
//! Interface icons come from `gpui-kit-assets`, which embeds the whole Lucide set: reference
//! them with `gpui_kit::assets::IconName` instead of file paths. The marks under `assets/marks`
//! (see the license file next to each set) are served under the `marks/` prefix.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_kit::assets::AllAssets;

/// Inter (SIL Open Font License; see `assets/fonts/LICENSE.txt`), registered once at startup in
/// [`super::run`] and used everywhere as the app's font.
pub const FONT: &[u8] = include_bytes!("../../assets/fonts/Inter.ttf");

/// Country flags, crypto logos and company logos: `marks/flags/us.svg`, `marks/crypto/btc.svg`,
/// `marks/brands/apple.svg`.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/marks"]
struct Marks;

/// Whether a `marks/...` asset exists, so a symbol without a logo can fall back to letters.
pub fn has_mark(path: &str) -> bool {
    path.strip_prefix("marks/")
        .is_some_and(|rest| Marks::get(rest).is_some())
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(rest) = path.strip_prefix("marks/") {
            return Ok(Marks::get(rest).map(|file| file.data));
        }
        AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        AllAssets.list(path)
    }
}
