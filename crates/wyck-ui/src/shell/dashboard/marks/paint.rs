//! Drawing a mark: the round picture, the ring around a second mark, and the colors of letters.

use super::{Icon, Mark};
use gpui::prelude::*;
use gpui::{AnyElement, FontWeight, Rgba, div, img, px, rgb, svg};

/// A stable, readable color for letters that stand for something with no picture.
pub(super) fn hashed_color(text: &str) -> u32 {
    const COLORS: [u32; 8] = [
        0x007C_86FF,
        0x0038_BDF8,
        0x0034_D399,
        0x00FB_BF24,
        0x00FB_7185,
        0x00C0_84FC,
        0x00F9_7316,
        0x002D_D4BF,
    ];
    let sum = text
        .bytes()
        .fold(0usize, |a, b| a.wrapping_add(usize::from(b)));
    COLORS.get(sum % COLORS.len()).copied().unwrap_or_default()
}

/// Whether a brand color is too dark to show on the app's dark background.
pub(super) fn is_dark(color: u32) -> bool {
    let (r, g, b) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
    (2126 * r + 7152 * g + 722 * b) / 10_000 < 60
}

fn tinted(color: u32, alpha: f32) -> Rgba {
    let mut c = rgb(color);
    c.a = alpha;
    c
}

/// One mark as a round picture of `diameter` pixels.
fn render_mark(mark: &Mark, diameter: f32) -> AnyElement {
    let disc = || {
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(diameter))
            .rounded_full()
            .overflow_hidden()
    };
    match mark {
        Mark::Flag(code) => img(format!("marks/flags/{code}.svg"))
            .size(px(diameter))
            .flex_none()
            .into_any_element(),
        Mark::Logo(path) => img(path.clone())
            .size(px(diameter))
            .flex_none()
            .into_any_element(),
        Mark::Brand { path, color } => {
            // A dark logo goes on a light disc, so it stays visible on the dark interface.
            let (background, glyph) = if is_dark(*color) {
                (rgb(0x00F4_F4F5), rgb(*color))
            } else {
                (rgb(*color), rgb(0x00FF_FFFF))
            };
            disc()
                .bg(background)
                .child(
                    svg()
                        .path(path.clone())
                        .size(px(diameter * 0.56))
                        .flex_none()
                        .text_color(glyph),
                )
                .into_any_element()
        }
        Mark::Metal { symbol, color } => disc()
            .bg(rgb(*color))
            .text_size(px(diameter * 0.42))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0x002B_2100))
            .child(*symbol)
            .into_any_element(),
        Mark::Glyph { icon, color } => disc()
            .bg(tinted(*color, 0.18))
            .child(
                svg()
                    .path(icon.path())
                    .size(px(diameter * 0.54))
                    .flex_none()
                    .text_color(rgb(*color)),
            )
            .into_any_element(),
        Mark::Letters { text, color } => disc()
            .bg(tinted(*color, 0.2))
            .text_size(px(diameter * 0.34))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(*color))
            .child(text.clone())
            .into_any_element(),
    }
}

/// The icon in a square of `size` pixels. A second mark is drawn smaller at the bottom right, ringed
/// in `ring` (the color behind the icon) so it reads as laid over the first.
pub(crate) fn render(icon: &Icon, size: f32, ring: Rgba) -> AnyElement {
    let Some(secondary) = &icon.secondary else {
        return div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(size))
            .child(render_mark(&icon.primary, size * 0.94))
            .into_any_element();
    };
    // A pair of flags is two equal discs. Otherwise the main picture (a coin, a company, a metal)
    // is big and the currency it is priced in is a small badge at the corner, so it never covers
    // the main one's letters.
    let both_flags = matches!(icon.primary, Mark::Flag(_)) && matches!(secondary, Mark::Flag(_));
    let (main, badge) = if both_flags {
        (size * 0.68, size * 0.68)
    } else {
        (size * 0.82, size * 0.46)
    };
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .child(render_mark(&icon.primary, main)),
        )
        .child(
            div()
                .absolute()
                .bottom_0()
                .right_0()
                .rounded_full()
                .border_2()
                .border_color(ring)
                .child(render_mark(secondary, badge - 3.0)),
        )
        .into_any_element()
}
