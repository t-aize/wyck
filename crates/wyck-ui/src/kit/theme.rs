//! Design tokens for the desktop app: colors, radii and type scale.
//!
//! Every color is read through a function of this module (`theme::bg()`, `theme::fg()`), and
//! every function reads the palette in force. The palette is a plain value, [`Colors`], that
//! `set_colors` replaces: that is how a set of candle colors chosen by the user reaches the whole
//! interface without a screen having to know about it. There is one theme, a dark one; what the
//! user can change lives in the app's `appearance` module. A screen should read `theme::` values
//! and never spell out a hex color inline.
//!
//! The palette is small on purpose: the backgrounds, the text colors, the accent, the three
//! signal colors and the colors of the chart. The lines between things, the tints of the
//! selection and of the warnings are derived from those.

use std::sync::RwLock;
use wyck_domain::base::math::f32_of;

use gpui::{Rgba, rgb, rgba};

pub(crate) use wyck_domain::base::appearance::{Colors, contrast};

fn store() -> &'static RwLock<Colors> {
    static COLORS: RwLock<Colors> = RwLock::new(Colors::WYCK_DARK);
    &COLORS
}

/// The palette in force.
pub fn colors() -> Colors {
    // A poisoned lock only means a thread panicked while writing a Copy value: the value is fine.
    *store()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Puts a palette in force. The screens read it on their next paint: refresh the windows (see
/// [`apply`]) to make that now.
pub(crate) fn set_colors(colors: Colors) {
    *store()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = colors;
}

fn font_store() -> &'static RwLock<Option<String>> {
    static FONT: RwLock<Option<String>> = RwLock::new(None);
    &FONT
}

/// Sets the font of the interface. `None` or a blank name leaves the system's. The screens read it
/// on their next paint: refresh the windows (see [`apply`]) to make that now.
pub(crate) fn set_font(name: Option<&str>) {
    *font_store()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
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
pub(crate) fn accent_fg() -> Rgba {
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

/// The veil behind a dialog, so what is open stands out from the screen under it.
pub fn veil() -> Rgba {
    rgba(0x0000_0099)
}

/// The veil over a locked app: strong enough that the frame behind reads as soft shapes and not
/// as content, which is the closest GPUI allows to a blur.
pub fn lock_veil() -> Rgba {
    let c = bg();
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: 0.78,
    }
}

/// Points gpui-component's theme at the palette in force, so its buttons, spinners and tooltips
/// match the hand-styled parts of the UI, then repaints every window. Call it at startup, after
/// `gpui_kit::init`, and again whenever the palette changes.
pub fn apply(cx: &mut gpui::App) {
    use gpui::Hsla;
    use gpui_kit::component::{Theme, ThemeMode};

    let palette = colors();
    Theme::change(ThemeMode::Dark, None, cx);
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
    theme_colors.danger_foreground = hsla(rgb(
        if contrast(0x0a_0a0a, palette.danger) >= contrast(0xff_ffff, palette.danger) {
            0x0a_0a0a
        } else {
            0xff_ffff
        },
    ));
    theme_colors.success = hsla(emerald());
    theme_colors.warning = hsla(amber());
    theme_colors.link = hsla(accent());
    theme_colors.overlay = hsla(veil());

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
    // The root of every window sets its text in the font of the theme.
    let font = font_store()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(name) = font {
        theme.font_family = name.into();
    }
    // Colors are read as they are painted, not through a binding, so no view knows its colors
    // went stale: every window has to paint again, from scratch.
    cx.refresh_windows();
}

/// Puts a look in force: animations, the scale of the interface, the font and the palette. The
/// windows are refreshed, so what is on screen follows at once.
pub fn put_in_force(look: &wyck_domain::base::appearance::Look, cx: &mut gpui::App) {
    crate::kit::anim::set_enabled(look.animations);
    // The widgets read this scale, and so does the text drawn on the charts.
    wyck_domain::base::text_scale::set(look.scale);
    // What is sized in rems (the components of gpui-kit) follows the same scale.
    for window in cx.windows() {
        if window
            .update(cx, |_, window, _| {
                window.set_rem_size(gpui::px(16.0 * f32_of(f64::from(look.scale)) / 100.0));
            })
            .is_err()
        {
            tracing::debug!("a window closed before its text size could follow the scale");
        }
    }
    set_font(Some(&look.font));
    set_colors(look.colors);
    apply(cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck_domain::base::appearance::luminance;

    // These tests never put a palette in force: the others run beside them and read it.

    #[test]
    fn the_default_palette_is_the_one_of_the_interface() {
        let c = Colors::default();
        assert_eq!(c, Colors::WYCK_DARK);
        assert_eq!(rgba(c.selected), rgba(0x615f_ff73));
        assert!(luminance(c.bg) < luminance(c.fg), "dark");
        assert!(
            contrast(c.fg, c.bg) >= 7.0 && contrast(c.muted, c.bg) >= 4.5,
            "the text reads on the background"
        );
    }

    #[test]
    fn text_on_the_chart_follows_the_background_of_the_chart() {
        // A dark theme on its own dark chart keeps its own text.
        let dark = Colors::WYCK_DARK;
        assert_eq!(dark.chart_text().0, dark.fg);
        // The same theme with a white chart gets dark text and a light tag.
        let white = Colors {
            chart_bg: 0xff_ffff,
            ..Colors::WYCK_DARK
        };
        let (fg, muted, tag) = white.chart_text();
        assert!(contrast(fg, 0xff_ffff) >= 7.0, "text {fg:x}");
        assert!(contrast(muted, 0xff_ffff) >= 4.5, "secondary {muted:x}");
        assert!(luminance(tag) > 0.5, "the crosshair tag is light");
        assert!(contrast(fg, tag) >= 4.5, "the text reads on its tag");
        // A light theme on a black chart gets light text.
        let black = Colors {
            bg: 0xff_ffff,
            fg: 0x11_1111,
            muted: 0x55_5555,
            chart_bg: 0x00_0000,
            ..Colors::WYCK_DARK
        };
        let (fg, _, tag) = black.chart_text();
        assert!(contrast(fg, 0x00_0000) >= 7.0);
        assert!(contrast(fg, tag) >= 4.5);
        // Any background: what is drawn on it reads.
        for bg in [
            0x00_0000, 0x20_2020, 0x80_8080, 0xc0_c0c0, 0xff_ffff, 0xff_ff00, 0x00_00ff,
        ] {
            for base in [
                Colors::WYCK_DARK,
                Colors {
                    bg: 0xff_ffff,
                    fg: 0x11_1111,
                    muted: 0x55_5555,
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
}
