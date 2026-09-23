//! The app's bundled files: the UI font and the icon set, compiled into the binary so it never
//! depends on anything being installed on the machine it runs on.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Inter (SIL Open Font License; see `assets/fonts/LICENSE.txt`), registered once at startup in
/// [`super::run`] and used everywhere as the app's font.
pub const FONT: &[u8] = include_bytes!("../../assets/fonts/Inter.ttf");

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/arrow-left.svg" => include_bytes!("../../assets/icons/arrow-left.svg"),
            "icons/check.svg" => include_bytes!("../../assets/icons/check.svg"),
            "icons/copy.svg" => include_bytes!("../../assets/icons/copy.svg"),
            "icons/external-link.svg" => include_bytes!("../../assets/icons/external-link.svg"),
            "icons/triangle-alert.svg" => include_bytes!("../../assets/icons/triangle-alert.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
