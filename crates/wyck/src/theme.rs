//! Design tokens for the desktop app: colors, radii and type scale.
//!
//! Every color is read through a function of this module (`theme::bg()`, `theme::fg()`), and
//! every function reads the palette in force. The palette is a plain value, [`Colors`], that
//! [`set_colors`] replaces: that is how a theme, an accent or a set of candle colors chosen by the
//! user reaches the whole interface without a screen having to know about it. The themes
//! themselves (their names, their colors, what the user changed) live in
//! [`super::appearance`]. A screen should read `theme::` values and never spell out a hex color
//! inline.
//!
//! The palette is small on purpose: the backgrounds, the text colors, the accent, the three
//! signal colors and the colors of the chart. The lines between things, the tints of the
//! selection and of the warnings are derived from those, so a theme only has to say what it looks
//! like and not how each border is drawn.

use std::sync::RwLock;

use gpui::{Rgba, rgb, rgba};

/// The colors of the interface and of the chart, as `0xRRGGBB` (the tint of a selection carries
/// its own alpha, as `0xRRGGBBAA`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Colors {
    /// Window and page background. Also the color of an unfocused text input.
    pub bg: u32,
    /// Raised surface: cards, panels, list rows.
    pub surface: u32,
    /// A row or an option while the pointer is over it, and while pressed.
    pub hover: u32,
    pub pressed: u32,
    /// Primary text, and secondary text (descriptions, hints, timestamps).
    pub fg: u32,
    pub muted: u32,
    /// The action color: buttons, active icons, links, focus rings. And what sits on top of it.
    pub accent: u32,
    pub accent_fg: u32,
    /// The tint of the selected segment of a control, with its alpha.
    pub selected: u32,
    /// Error, warning and success.
    pub danger: u32,
    pub amber: u32,
    pub emerald: u32,
    /// The chart: a rising and a falling candle, a line, the background of the crosshair's tags,
    /// and the background of the chart itself.
    pub up: u32,
    pub down: u32,
    pub line: u32,
    pub tag: u32,
    pub chart_bg: u32,
}

impl Colors {
    /// The dark palette the app started with.
    pub const WYCK_DARK: Self = Self {
        bg: 0x0a0a0a,
        surface: 0x171717,
        hover: 0x1f1f1f,
        pressed: 0x282828,
        fg: 0xfafafa,
        muted: 0xa1a1a1,
        accent: 0x7c86ff,
        accent_fg: 0x0a0a0a,
        selected: 0x615fff73,
        danger: 0xff6467,
        amber: 0xffb900,
        emerald: 0x00d492,
        up: 0x26a69a,
        down: 0xef5350,
        line: 0x5b8def,
        tag: 0x363a45,
        chart_bg: 0x0a0a0a,
    };
}

impl Colors {
    /// Whether this is a light palette: the background is lighter than the text.
    pub fn is_light(&self) -> bool {
        luminance(self.bg) > luminance(self.fg)
    }

    /// The text colors for what is drawn straight on the chart: the text, the secondary text and
    /// the background of the crosshair's tags. The chart can have a background of its own, so a
    /// dark theme can sit on a white chart: the theme's text is kept while it reads on that
    /// background, and a dark or light one takes over when it does not.
    pub fn chart_text(&self) -> (u32, u32, u32) {
        if contrast(self.fg, self.chart_bg) >= 4.5 && contrast(self.muted, self.chart_bg) >= 3.0 {
            (self.fg, self.muted, self.tag)
        } else {
            let bg = self.chart_bg;
            let light = (0xf0f0f0, 0x9aa0a6, 0x363a45);
            let dark = (0x1f2328, 0x57606a, 0xd0d7de);
            let best = if contrast(dark.0, bg) >= contrast(light.0, bg) {
                dark
            } else {
                light
            };
            if contrast(best.0, bg) >= 4.5 {
                best
            } else if contrast(0x000000, bg) >= contrast(0xffffff, bg) {
                // A background in the middle of the range: plain black or white reads best.
                (0x000000, 0x3a3a3a, dark.2)
            } else {
                (0xffffff, 0xd0d0d0, light.2)
            }
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self::WYCK_DARK
    }
}

fn store() -> &'static RwLock<Colors> {
    static COLORS: RwLock<Colors> = RwLock::new(Colors::WYCK_DARK);
    &COLORS
}

/// The palette in force.
pub fn colors() -> Colors {
    // A poisoned lock only means a thread panicked while writing a Copy value: the value is fine.
    *store().read().unwrap_or_else(|e| e.into_inner())
}

/// Puts a palette in force. The screens read it on their next paint: refresh the windows (see
/// [`apply`]) to make that now.
pub fn set_colors(colors: Colors) {
    *store().write().unwrap_or_else(|e| e.into_inner()) = colors;
}

/// `color` at the opacity `alpha`.
fn with_alpha(color: u32, alpha: f32) -> Rgba {
    Rgba {
        a: alpha,
        ..rgb(color)
    }
}

/// The text color at the opacity `alpha`: a faint wash on a surface, that shows on a light theme as
/// well as on a dark one.
pub fn fg_alpha(alpha: f32) -> Rgba {
    with_alpha(colors().fg, alpha)
}

/// The page background at the opacity `alpha`, for what floats over the chart.
pub fn bg_alpha(alpha: f32) -> Rgba {
    with_alpha(colors().bg, alpha)
}

/// The surface color at the opacity `alpha`.
pub fn surface_alpha(alpha: f32) -> Rgba {
    with_alpha(colors().surface, alpha)
}

/// The accent at the opacity `alpha`.
pub fn accent_alpha(alpha: f32) -> Rgba {
    with_alpha(colors().accent, alpha)
}

/// Window and page background. Also the color of an unfocused text input.
pub fn bg() -> Rgba {
    rgb(colors().bg)
}

/// Raised surface: cards, panels, number badges, list rows.
pub fn surface() -> Rgba {
    rgb(colors().surface)
}

/// Primary text.
pub fn fg() -> Rgba {
    rgb(colors().fg)
}

/// Secondary text: descriptions, hints, timestamps.
pub fn muted_fg() -> Rgba {
    rgb(colors().muted)
}

/// A subtle divider between stacked sections.
pub fn border_hairline() -> Rgba {
    with_alpha(colors().fg, 0.10)
}

/// A slightly stronger border: input outlines, badge rings, card edges.
pub fn border_subtle() -> Rgba {
    with_alpha(colors().fg, 0.15)
}

/// A card or row edge while the pointer is over it.
pub fn border_strong() -> Rgba {
    with_alpha(colors().fg, 0.25)
}

/// Primary action color: buttons, active icons, links, focus rings.
pub fn accent() -> Rgba {
    rgb(colors().accent)
}

/// Text/icon color to place on top of [`accent`].
pub fn accent_fg() -> Rgba {
    rgb(colors().accent_fg)
}

/// The tint used for the selected segment of a segmented control.
pub fn accent_selected() -> Rgba {
    rgba(colors().selected)
}

/// Error text, icons and borders.
pub fn destructive() -> Rgba {
    rgb(colors().danger)
}

/// Background tint for an error banner or an invalid field.
pub fn destructive_bg() -> Rgba {
    with_alpha(colors().danger, 0.10)
}

/// The "LIVE" account badge.
pub fn amber() -> Rgba {
    rgb(colors().amber)
}

/// Background tint for an amber badge.
pub fn amber_bg() -> Rgba {
    with_alpha(colors().amber, 0.15)
}

/// Success state: the connected checkmark, "Connected" text.
pub fn emerald() -> Rgba {
    rgb(colors().emerald)
}

/// Row and option background while the pointer is over it.
pub fn surface_hover() -> Rgba {
    rgb(colors().hover)
}

/// Row and option background while pressed.
pub fn surface_pressed() -> Rgba {
    rgb(colors().pressed)
}

/// A rising candle, volume column or price tag.
pub fn chart_up() -> Rgba {
    rgb(colors().up)
}

/// A falling candle, volume column or price tag.
pub fn chart_down() -> Rgba {
    rgb(colors().down)
}

/// The line of a line, area or step chart.
pub fn chart_line() -> Rgba {
    rgb(colors().line)
}

/// The background of the chart.
pub fn chart_bg() -> Rgba {
    rgb(colors().chart_bg)
}

/// Text drawn straight on the chart, and the secondary kind: they follow the background of the
/// chart, which is not always the theme's (see [`Colors::chart_text`]).
pub fn chart_fg() -> Rgba {
    rgb(colors().chart_text().0)
}

pub fn chart_muted() -> Rgba {
    rgb(colors().chart_text().1)
}

/// The grid behind the prices.
pub fn chart_grid() -> Rgba {
    with_alpha(colors().chart_text().0, 0.045)
}

/// The lines between the parts of the chart (the edge of the axes, the panes).
pub fn chart_border() -> Rgba {
    with_alpha(colors().chart_text().0, 0.10)
}

/// The background of the crosshair's tags on the axes.
pub fn chart_tag() -> Rgba {
    rgb(colors().chart_text().2)
}

/// The crosshair's dashed lines.
pub fn chart_crosshair() -> Rgba {
    with_alpha(colors().chart_text().1, 0.69)
}

/// Whether the palette in force is a light one.
pub fn is_light() -> bool {
    colors().is_light()
}

/// The contrast ratio of two `0xRRGGBB` colors, from 1 to 21.
pub fn contrast(a: u32, b: u32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The relative luminance of a `0xRRGGBB` color, from 0 (black) to 1 (white).
pub fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| {
        let c = ((color >> shift) & 0xff) as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// Points gpui-component's theme at the palette in force, so its buttons, spinners and tooltips
/// match the hand-styled parts of the UI, then repaints every window. Call it at startup, after
/// `gpui_kit::init`, and again whenever the palette changes.
pub fn apply(cx: &mut gpui::App) {
    use gpui::Hsla;
    use gpui_kit::component::{Theme, ThemeMode};

    let palette = colors();
    let mode = if is_light() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    Theme::change(mode, None, cx);
    let theme_colors = &mut Theme::global_mut(cx).colors;
    let hsla = |color: Rgba| -> Hsla { color.into() };

    theme_colors.background = hsla(bg());
    theme_colors.foreground = hsla(fg());
    theme_colors.border = hsla(border_subtle());
    theme_colors.input = hsla(border_subtle());
    theme_colors.ring = hsla(accent());
    theme_colors.muted = hsla(surface());
    theme_colors.muted_foreground = hsla(muted_fg());
    theme_colors.popover = hsla(surface());
    theme_colors.popover_foreground = hsla(fg());
    theme_colors.title_bar = hsla(surface());
    theme_colors.title_bar_border = hsla(border_hairline());

    theme_colors.primary = hsla(accent());
    theme_colors.primary_hover = hsla(accent()).opacity(0.88);
    theme_colors.primary_active = hsla(accent()).opacity(0.75);
    theme_colors.primary_foreground = hsla(accent_fg());

    theme_colors.secondary = hsla(surface());
    theme_colors.secondary_hover = hsla(surface_hover());
    theme_colors.secondary_active = hsla(surface_pressed());
    theme_colors.secondary_foreground = hsla(fg());

    theme_colors.accent = hsla(surface());
    theme_colors.accent_foreground = hsla(fg());

    theme_colors.danger = hsla(destructive());
    theme_colors.danger_hover = hsla(destructive()).opacity(0.88);
    theme_colors.danger_active = hsla(destructive()).opacity(0.75);
    theme_colors.danger_foreground = hsla(rgb(if luminance(palette.danger) > 0.4 {
        0x0a0a0a
    } else {
        0xffffff
    }));
    theme_colors.success = hsla(emerald());
    theme_colors.warning = hsla(amber());
    theme_colors.link = hsla(accent());
    // The veil behind a dialog, so what is open stands out from the screen under it.
    theme_colors.overlay = hsla(gpui::rgba(if is_light() {
        0x0000_0055
    } else {
        0x0000_0099
    }));

    // Buttons read their own set of colors, which default to unrelated values.
    theme_colors.button = theme_colors.secondary;
    theme_colors.button_hover = theme_colors.secondary_hover;
    theme_colors.button_active = theme_colors.secondary_active;
    theme_colors.button_foreground = theme_colors.secondary_foreground;
    theme_colors.button_primary = theme_colors.primary;
    theme_colors.button_primary_hover = theme_colors.primary_hover;
    theme_colors.button_primary_active = theme_colors.primary_active;
    theme_colors.button_primary_foreground = theme_colors.primary_foreground;
    theme_colors.button_secondary = theme_colors.secondary;
    theme_colors.button_secondary_hover = theme_colors.secondary_hover;
    theme_colors.button_secondary_active = theme_colors.secondary_active;
    theme_colors.button_secondary_foreground = theme_colors.secondary_foreground;
    theme_colors.button_danger = theme_colors.danger;
    theme_colors.button_danger_hover = theme_colors.danger_hover;
    theme_colors.button_danger_active = theme_colors.danger_active;
    theme_colors.button_danger_foreground = theme_colors.danger_foreground;

    // Components read a second, derived copy of the palette; rebuild it from the edited colors.
    let theme = Theme::global_mut(cx);
    theme.tokens = theme.colors.into();
    // Colors are read as they are painted, not through a binding, so no view knows its colors
    // went stale: every window has to paint again, from scratch.
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests never put a palette in force: the others run beside them and read it.

    #[test]
    fn the_default_palette_is_the_one_the_app_started_with() {
        let c = Colors::default();
        assert_eq!(c, Colors::WYCK_DARK);
        assert_eq!(rgba(c.selected), rgba(0x615fff73));
        assert!(!c.is_light());
    }

    #[test]
    fn light_and_dark_palettes_are_told_apart() {
        let light = Colors {
            bg: 0xffffff,
            fg: 0x111111,
            ..Colors::WYCK_DARK
        };
        assert!(light.is_light());
        assert!(!Colors::WYCK_DARK.is_light());
    }

    #[test]
    fn text_on_the_chart_follows_the_background_of_the_chart() {
        // A dark theme on its own dark chart keeps its own text.
        let dark = Colors::WYCK_DARK;
        assert_eq!(dark.chart_text().0, dark.fg);
        // The same theme with a white chart gets dark text and a light tag.
        let white = Colors {
            chart_bg: 0xffffff,
            ..Colors::WYCK_DARK
        };
        let (fg, muted, tag) = white.chart_text();
        assert!(contrast(fg, 0xffffff) >= 7.0, "text {fg:x}");
        assert!(contrast(muted, 0xffffff) >= 4.5, "secondary {muted:x}");
        assert!(luminance(tag) > 0.5, "the crosshair tag is light");
        assert!(contrast(fg, tag) >= 4.5, "the text reads on its tag");
        // A light theme on a black chart gets light text.
        let black = Colors {
            bg: 0xffffff,
            fg: 0x111111,
            muted: 0x555555,
            chart_bg: 0x000000,
            ..Colors::WYCK_DARK
        };
        let (fg, _, tag) = black.chart_text();
        assert!(contrast(fg, 0x000000) >= 7.0);
        assert!(contrast(fg, tag) >= 4.5);
        // Any background: what is drawn on it reads.
        for bg in [
            0x000000, 0x202020, 0x808080, 0xc0c0c0, 0xffffff, 0xffff00, 0x0000ff,
        ] {
            for base in [
                Colors::WYCK_DARK,
                Colors {
                    bg: 0xffffff,
                    fg: 0x111111,
                    muted: 0x555555,
                    ..Colors::WYCK_DARK
                },
            ] {
                let c = Colors {
                    chart_bg: bg,
                    ..base
                };
                let (fg, _, _) = c.chart_text();
                assert!(
                    contrast(fg, bg) >= 4.5,
                    "text on {bg:06x}: {:.1}",
                    contrast(fg, bg)
                );
            }
        }
    }

    #[test]
    fn luminance_runs_from_black_to_white() {
        assert!(luminance(0x000000) < 0.001);
        assert!(luminance(0xffffff) > 0.99);
        assert!(luminance(0x808080) > luminance(0x202020));
    }
}
