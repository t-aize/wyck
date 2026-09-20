//! The icons and the fonts, compiled into the binary.
//!
//! [`AppAssets`] serves the application's own SVG icons (under `wyck/`) and falls back to the
//! bundled icon set of the component library for everything else. The Geist fonts are TTF files
//! registered with [`load_fonts`]: the text engine on Windows does not read WOFF2, so the web
//! fonts of the design could not be used as they are. Both fonts are under the SIL Open Font
//! License, whose text is `assets/fonts/OFL.txt`.

use std::borrow::Cow;

use gpui_kit::{App, AssetSource, Result, SharedString};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        &[$((
            concat!("wyck/", $name, ".svg"),
            include_bytes!(concat!("../../assets/icons/", $name, ".svg")).as_slice(),
        )),*]
    };
}

static ICONS: &[(&str, &[u8])] = icons![
    "app-window",
    "arc",
    "arrow-up-right",
    "check",
    "chevron-left",
    "chevron-right",
    "circle",
    "circle-alert",
    "circle-check",
    "circle-x",
    "clipboard",
    "cloud",
    "eye",
    "eye-off",
    "info",
    "logo-bars",
    "logo-tile",
    "monitor",
    "ring",
    "scan",
    "triangle-alert",
    "user",
    "sliders",
    "layout-grid",
    "square",
    "log-out",
    "tick-up",
    "tick-down",
    "win-close",
    "win-max",
    "win-min",
    "win-restore",
    "x",
    "lucide-arrow-down",
    "lucide-arrow-up",
    "lucide-bitcoin",
    "lucide-building-2",
    "lucide-chevrons-up-down",
    "lucide-corner-down-left",
    "lucide-fuel",
    "lucide-gem",
    "lucide-landmark",
    "lucide-layers",
    "lucide-search",
    "lucide-trending-up",
    "lucide-wheat",
];

macro_rules! flags {
    ($($code:literal),* $(,)?) => {
        &[$((
            concat!("wyck/flags/", $code, ".svg"),
            include_bytes!(concat!("../../assets/flags/", $code, ".svg")).as_slice(),
        )),*]
    };
}

static FLAGS: &[(&str, &[u8])] = flags![
    "at", "au", "be", "br", "ca", "ch", "cn", "cz", "de", "dk", "es", "eu", "fi", "fr", "gb", "hk",
    "hu", "ie", "il", "in", "it", "jp", "kr", "mx", "nl", "no", "nz", "pl", "pt", "ru", "se", "sg",
    "th", "tr", "us", "za",
];

static FONTS: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../../assets/fonts/Geist-SemiBold.ttf"),
];

/// The application's asset source.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().chain(FLAGS).find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out: Vec<SharedString> = ICONS
            .iter()
            .chain(FLAGS)
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect();
        out.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(out)
    }
}

/// Registers the Geist fonts. A failure is logged and the system font is used instead.
pub fn load_fonts(cx: &mut App) {
    let fonts = FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        tracing::warn!(%error, "the Geist fonts could not be loaded, using the system font");
    } else {
        let names = cx.text_system().all_font_names();
        let ours: Vec<_> = names.iter().filter(|n| n.contains("Geist")).collect();
        tracing::info!(?ours, total = names.len(), "fonts registered");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_an_svg_document() {
        for (name, bytes) in ICONS {
            let text = std::str::from_utf8(bytes).unwrap();
            assert!(text.trim_start().starts_with("<svg"), "{name}");
            assert!(text.contains("viewBox"), "{name}");
        }
    }

    #[test]
    fn the_fonts_are_truetype_files() {
        for bytes in FONTS {
            assert_eq!(
                bytes[..4],
                [0, 1, 0, 0],
                "a TrueType file starts with 0x00010000"
            );
        }
    }

    use crate::symbols::FLAG_CODES;

    #[test]
    fn every_flag_code_has_its_file_and_a_flag_is_served_like_an_icon() {
        assert_eq!(FLAG_CODES.len(), FLAGS.len());
        for code in FLAG_CODES {
            let path = format!("wyck/flags/{code}.svg");
            let file = AppAssets.load(&path).unwrap();
            assert!(file.is_some(), "{path}");
        }
        for (name, bytes) in FLAGS {
            assert!(
                std::str::from_utf8(bytes).unwrap().contains("<svg"),
                "{name}"
            );
        }
    }

    #[test]
    fn an_own_icon_is_served_and_an_unknown_path_is_not() {
        assert!(AppAssets.load("wyck/monitor.svg").unwrap().is_some());
        assert!(!matches!(AppAssets.load("wyck/nope.svg"), Ok(Some(_))));
    }
}
