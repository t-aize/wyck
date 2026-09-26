//! The themes that come with the app: a name, an id that is saved, and a palette.
//!
//! Each is a small set of colors (see [`Colors`]), so adding one is adding a line. They follow the
//! well known editor palettes, and the chart colors are picked to read on the background: a green
//! and a red that stand apart from it. A theme is dark or light by what its background is, not by
//! a flag.

use crate::app::theme::Colors;

/// A theme that comes with the app.
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub colors: Colors,
}

/// The tint of a selection over a background: the accent, at about 40% (`0xRRGGBBAA`).
const fn selection(accent: u32) -> u32 {
    (accent << 8) | 0x66
}

/// Fills in what the palettes below leave to a rule: the tint of the selection, the text on the
/// accent, and the chart's background, which is the page's.
#[allow(clippy::too_many_arguments)]
const fn palette(
    bg: u32,
    surface: u32,
    hover: u32,
    pressed: u32,
    fg: u32,
    muted: u32,
    accent: u32,
    danger: u32,
    amber: u32,
    emerald: u32,
    up: u32,
    down: u32,
    line: u32,
    tag: u32,
) -> Colors {
    Colors {
        bg,
        surface,
        hover,
        pressed,
        fg,
        muted,
        accent,
        // Black or white on the accent, whichever reads better.
        accent_fg: if accent_is_light(accent) {
            0x0a0a0a
        } else {
            0xffffff
        },
        selected: selection(accent),
        danger,
        amber,
        emerald,
        up,
        down,
        line,
        tag,
        chart_bg: bg,
    }
}

/// Whether text on this color should be dark. A cheap version of the contrast rule, fit for a
/// constant: the weighted average of the channels.
const fn accent_is_light(color: u32) -> bool {
    let (r, g, b) = ((color >> 16) & 0xff, (color >> 8) & 0xff, color & 0xff);
    (r * 299 + g * 587 + b * 114) / 1000 > 150
}

pub const DEFAULT_DARK: &str = "wyck-dark";
pub const DEFAULT_LIGHT: &str = "wyck-light";

/// Every theme that comes with the app, dark ones first.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "wyck-dark",
        name: "Wyck Dark",
        colors: Colors::WYCK_DARK,
    },
    Preset {
        id: "midnight",
        name: "Midnight",
        colors: palette(
            0x0b0f1a, 0x131a2b, 0x1b2439, 0x25304a, 0xe6ebf5, 0x8b96b1, 0x5b8def, 0xff6467,
            0xffb900, 0x00d492, 0x26a69a, 0xef5350, 0x5b8def, 0x2a3450,
        ),
    },
    Preset {
        id: "graphite",
        name: "Graphite",
        colors: palette(
            0x121212, 0x1c1c1c, 0x262626, 0x303030, 0xf0f0f0, 0x9a9a9a, 0xe5a00d, 0xff6467,
            0xe5a00d, 0x3ecf8e, 0x3ecf8e, 0xf2555a, 0xe5a00d, 0x383838,
        ),
    },
    Preset {
        id: "nord",
        name: "Nord",
        colors: palette(
            0x2e3440, 0x3b4252, 0x434c5e, 0x4c566a, 0xeceff4, 0x9aa5b8, 0x88c0d0, 0xbf616a,
            0xebcb8b, 0xa3be8c, 0xa3be8c, 0xbf616a, 0x81a1c1, 0x4c566a,
        ),
    },
    Preset {
        id: "dracula",
        name: "Dracula",
        colors: palette(
            0x282a36, 0x343746, 0x44475a, 0x515570, 0xf8f8f2, 0xa3a7c2, 0xbd93f9, 0xff5555,
            0xf1fa8c, 0x50fa7b, 0x50fa7b, 0xff5555, 0x8be9fd, 0x44475a,
        ),
    },
    Preset {
        id: "tokyo-night",
        name: "Tokyo Night",
        colors: palette(
            0x1a1b26, 0x24283b, 0x2f3549, 0x3b4261, 0xc0caf5, 0x7a83a6, 0x7aa2f7, 0xf7768e,
            0xe0af68, 0x9ece6a, 0x9ece6a, 0xf7768e, 0x7dcfff, 0x3b4261,
        ),
    },
    Preset {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        colors: palette(
            0x1e1e2e, 0x313244, 0x45475a, 0x585b70, 0xcdd6f4, 0xa6adc8, 0xcba6f7, 0xf38ba8,
            0xf9e2af, 0xa6e3a1, 0xa6e3a1, 0xf38ba8, 0x89b4fa, 0x45475a,
        ),
    },
    Preset {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        colors: palette(
            0x282828, 0x3c3836, 0x504945, 0x665c54, 0xebdbb2, 0xa89984, 0xfabd2f, 0xfb4934,
            0xfabd2f, 0xb8bb26, 0xb8bb26, 0xfb4934, 0x83a598, 0x504945,
        ),
    },
    Preset {
        id: "one-dark",
        name: "One Dark",
        colors: palette(
            0x282c34, 0x2c313a, 0x3e4451, 0x4b5263, 0xabb2bf, 0x7f848e, 0x61afef, 0xe06c75,
            0xe5c07b, 0x98c379, 0x98c379, 0xe06c75, 0x61afef, 0x3e4451,
        ),
    },
    Preset {
        id: "solarized-dark",
        name: "Solarized Dark",
        colors: palette(
            0x002b36, 0x073642, 0x0e4554, 0x1a5563, 0x93a1a1, 0x6c8089, 0x268bd2, 0xdc322f,
            0xb58900, 0x859900, 0x859900, 0xdc322f, 0x2aa198, 0x1a5563,
        ),
    },
    Preset {
        id: "wyck-light",
        name: "Wyck Light",
        colors: palette(
            0xffffff, 0xf4f4f5, 0xe9e9ec, 0xdcdce0, 0x18181b, 0x6b6b76, 0x5b5bd6, 0xdc2626,
            0xd97706, 0x059669, 0x089981, 0xf23645, 0x2962ff, 0xe4e4e7,
        ),
    },
    Preset {
        id: "github-light",
        name: "GitHub Light",
        colors: palette(
            0xffffff, 0xf6f8fa, 0xeaeef2, 0xd0d7de, 0x1f2328, 0x656d76, 0x0969da, 0xcf222e,
            0x9a6700, 0x1a7f37, 0x1a7f37, 0xcf222e, 0x0969da, 0xd0d7de,
        ),
    },
    Preset {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        colors: palette(
            0xeff1f5, 0xe6e9ef, 0xccd0da, 0xbcc0cc, 0x4c4f69, 0x6c6f85, 0x8839ef, 0xd20f39,
            0xdf8e1d, 0x40a02b, 0x40a02b, 0xd20f39, 0x1e66f5, 0xccd0da,
        ),
    },
    Preset {
        id: "solarized-light",
        name: "Solarized Light",
        colors: palette(
            0xfdf6e3, 0xeee8d5, 0xe4ddc4, 0xd8d0b4, 0x586e75, 0x657b83, 0x268bd2, 0xdc322f,
            0xb58900, 0x859900, 0x859900, 0xdc322f, 0x268bd2, 0xd8d0b4,
        ),
    },
    Preset {
        id: "gruvbox-light",
        name: "Gruvbox Light",
        colors: palette(
            0xfbf1c7, 0xebdbb2, 0xd5c4a1, 0xbdae93, 0x3c3836, 0x7c6f64, 0xb57614, 0x9d0006,
            0xb57614, 0x79740e, 0x79740e, 0x9d0006, 0x076678, 0xd5c4a1,
        ),
    },
    Preset {
        id: "paper",
        name: "Paper",
        colors: palette(
            0xfafaf7, 0xf1f0ea, 0xe7e5dc, 0xdcd9cd, 0x2b2b2b, 0x77756b, 0x2f6f4f, 0xb42318,
            0xb54708, 0x2f6f4f, 0x2f6f4f, 0xb42318, 0x335c8a, 0xdcd9cd,
        ),
    },
];

/// The theme that comes with the app under `id`.
pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Ready-made candle colors: a name, and the color of a rising and of a falling candle.
pub const CANDLE_SETS: &[(&str, u32, u32)] = &[
    ("Classic", 0x26a69a, 0xef5350),
    ("Green and red", 0x22c55e, 0xef4444),
    ("Blue and orange", 0x3b82f6, 0xf97316),
    ("Blue and red", 0x2962ff, 0xf23645),
    ("Teal and pink", 0x14b8a6, 0xec4899),
    ("Gold and violet", 0xeab308, 0x8b5cf6),
    ("Monochrome", 0xe5e5e5, 0x6b6b6b),
    ("Neon", 0x00ff9c, 0xff2e63),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::theme::luminance;

    #[test]
    fn every_preset_has_its_own_id_and_name() {
        let mut ids: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len(), "an id is used twice");
        let mut names: Vec<&str> = PRESETS.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PRESETS.len(), "a name is used twice");
        assert!(preset(DEFAULT_DARK).is_some() && preset(DEFAULT_LIGHT).is_some());
        assert!(preset("no-such-theme").is_none());
    }

    #[test]
    fn the_default_themes_are_a_dark_one_and_a_light_one() {
        assert!(!preset(DEFAULT_DARK).unwrap().colors.is_light());
        assert!(preset(DEFAULT_LIGHT).unwrap().colors.is_light());
    }

    #[test]
    fn every_preset_can_be_read() {
        // Enough contrast between the text and the background to read, and a rising candle that
        // does not look like a falling one.
        for p in PRESETS {
            let c = p.colors;
            let (a, b) = (luminance(c.bg), luminance(c.fg));
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(
                ratio >= 4.5,
                "{}: text on the background is {ratio:.1}:1",
                p.name
            );
            assert_ne!(c.up, c.down, "{}: up and down are the same", p.name);
            // The text of the muted kind still reads.
            let (m, bg) = (luminance(c.muted), luminance(c.bg));
            let muted = (m.max(bg) + 0.05) / (m.min(bg) + 0.05);
            assert!(muted >= 3.0, "{}: muted text is {muted:.1}:1", p.name);
            // What is written on the accent reads too.
            let (t, ac) = (luminance(c.accent_fg), luminance(c.accent));
            let on_accent = (t.max(ac) + 0.05) / (t.min(ac) + 0.05);
            assert!(
                on_accent >= 3.0,
                "{}: text on the accent is {on_accent:.1}:1",
                p.name
            );
        }
    }

    #[test]
    fn the_candle_sets_have_two_different_colors() {
        for (name, up, down) in CANDLE_SETS {
            assert_ne!(up, down, "{name}");
        }
    }
}
