//! The palette, the fonts, and the dark theme installed in GPUI.
//!
//! Colors are written once here as hex values and used through the small functions below, so a
//! screen never contains a color literal. The palette is neutral grays plus one green and one
//! red; amber is added for warnings, which the connection screens do not use but toasts do.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, Rgba, px, rgb};

use crate::presentation::Tone;

/// The UI font.
pub const SANS: &str = "Geist";
/// The font of tokens, addresses and figures.
pub const MONO: &str = "Geist Mono";

const BG: u32 = 0x0a_0a_0a;
const CARD: u32 = 0x17_17_17;
const MUTED: u32 = 0x26_26_26;
const FG: u32 = 0xfa_fa_fa;
const DIM: u32 = 0xa1_a1_a1;
const ACCENT: u32 = 0xe5_e5_e5;
const ACCENT_HOVER: u32 = 0xfa_fa_fa;
const RING: u32 = 0x73_73_73;
const GREEN: u32 = 0x34_d3_99;
const RED: u32 = 0xff_64_67;
const RED_PRESSED: u32 = 0xe5_48_4d;
const RED_TEXT: u32 = 0xff_b3_b4;
const AMBER: u32 = 0xfb_bf_24;

fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// `color` with its opacity replaced by `alpha` (0 to 1).
#[must_use]
pub fn alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

/// `top` laid over the opaque `base` at `amount` opacity: the opaque color that results. For a
/// ground that gets slightly lighter, where a translucent color would let what is behind show.
#[must_use]
pub fn over(base: Hsla, top: Hsla, amount: f32) -> Hsla {
    let (b, t) = (Rgba::from(base), Rgba::from(top));
    let amount = amount.clamp(0.0, 1.0) * t.a;
    Rgba {
        r: b.r + (t.r - b.r) * amount,
        g: b.g + (t.g - b.g) * amount,
        b: b.b + (t.b - b.b) * amount,
        a: 1.0,
    }
    .into()
}

/// The window background.
#[must_use]
pub fn bg() -> Hsla {
    hsla(BG)
}
/// The background of cards and of the title bar.
#[must_use]
pub fn card() -> Hsla {
    hsla(CARD)
}
/// The background of icon tiles, tracks and hovered buttons.
#[must_use]
pub fn muted() -> Hsla {
    hsla(MUTED)
}
/// Primary text.
#[must_use]
pub fn fg() -> Hsla {
    hsla(FG)
}
/// Secondary text and icons.
#[must_use]
pub fn dim() -> Hsla {
    hsla(DIM)
}
/// The hairline around cards and between rows: white at 10 percent.
#[must_use]
pub fn border() -> Hsla {
    alpha(hsla(FG), 0.10)
}
/// The fill of the main button.
#[must_use]
pub fn accent() -> Hsla {
    hsla(ACCENT)
}
/// The main button under the pointer.
#[must_use]
pub fn accent_hover() -> Hsla {
    hsla(ACCENT_HOVER)
}
/// The border of a focused field.
#[must_use]
pub fn ring() -> Hsla {
    hsla(RING)
}
/// Success.
#[must_use]
pub fn green() -> Hsla {
    hsla(GREEN)
}
/// Failure, danger.
#[must_use]
pub fn red() -> Hsla {
    hsla(RED)
}
/// The close button while pressed.
#[must_use]
pub fn red_pressed() -> Hsla {
    hsla(RED_PRESSED)
}
/// Error text on a red-tinted ground.
#[must_use]
pub fn red_text() -> Hsla {
    hsla(RED_TEXT)
}
/// Caution.
#[must_use]
pub fn amber() -> Hsla {
    hsla(AMBER)
}

/// The color that draws `tone`.
#[must_use]
pub fn tone_color(tone: Tone) -> Hsla {
    match tone {
        Tone::Neutral => dim(),
        Tone::Good => green(),
        Tone::Warn => amber(),
        Tone::Bad => red(),
    }
}

/// Installs the dark theme with this palette and the Geist fonts. Call once, after
/// `gpui_kit::init` and after [`super::assets::load_fonts`].
pub fn install(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.font_family = SANS.into();
    theme.mono_font_family = MONO.into();
    theme.font_size = px(13.);
    theme.mono_font_size = px(12.);
    theme.shadow = false;

    let colors = &mut theme.colors;
    colors.background = bg();
    colors.foreground = fg();
    colors.muted = muted();
    colors.muted_foreground = dim();
    colors.border = border();
    colors.input = border();
    colors.ring = ring();
    colors.caret = fg();
    colors.selection = alpha(hsla(RING), 0.45);
    colors.popover = card();
    colors.popover_foreground = fg();
    colors.title_bar = card();
    colors.title_bar_border = border();
    colors.danger = red();
    colors.success = green();
    Theme::sync_base(cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_border_is_a_faint_white() {
        let b = border();
        assert!((b.a - 0.10).abs() < f32::EPSILON);
        assert!(b.l > 0.9, "white, not gray");
    }

    #[test]
    fn a_color_laid_over_another_is_opaque_and_between_them() {
        let lifted = Rgba::from(over(bg(), fg(), 0.035));
        assert!((lifted.a - 1.0).abs() < 1e-6);
        assert!(lifted.r > Rgba::from(bg()).r && lifted.r < 0.2);
        assert_eq!(Rgba::from(over(bg(), fg(), 0.0)), Rgba::from(bg()));
    }

    #[test]
    fn every_tone_has_its_own_color() {
        let all = [Tone::Neutral, Tone::Good, Tone::Warn, Tone::Bad].map(tone_color);
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
