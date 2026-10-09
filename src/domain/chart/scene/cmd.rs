//! The drawing commands a frame is made of, in window coordinates.
//!
//! They name shapes, not a graphics library: the desktop painter turns them into gpui quads,
//! paths and text on screen, and the PNG renderer into pixels for a picture of the chart.
//! Building them is plain arithmetic, so it is tested without a window.

use super::color::{Hsla, Rgba, rgb};
use crate::domain::drawings::look::{Face, VAlign};

pub type P = (f32, f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// A filled rectangle, with an optional border and rounded corners.
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Hsla,
        border: Option<(f32, Hsla)>,
        radius: f32,
    },
    /// A stroked polyline, solid or dashed (the lengths of a dash and of the gap after it).
    Stroke {
        points: Vec<P>,
        width: f32,
        color: Hsla,
        dash: Option<[f32; 2]>,
    },
    /// A filled polygon.
    Fill { points: Vec<P>, color: Hsla },
    /// Text whose top is at `y`, aligned on `x`.
    Text {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        color: Hsla,
        align: Align,
        bold: bool,
        face: Face,
    },
    /// A label fitted inside a drawing's bounds using the renderer's font metrics.
    FittedText {
        text: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        color: Hsla,
        background: Option<Hsla>,
        align: Align,
        valign: VAlign,
        bold: bool,
        face: Face,
    },
    /// Text on a filled, rounded tag sized to the text (or to `fixed_width`).
    Tag {
        text: String,
        x: f32,
        y: f32,
        height: f32,
        pad: f32,
        bg: Hsla,
        fg: Hsla,
        align: Align,
        fixed_width: Option<f32>,
        /// Kept inside `left..right` once its width is known.
        within: Option<(f32, f32)>,
        size: f32,
        bold: bool,
        face: Face,
    },
    /// Commands drawn only inside a rectangle.
    Clip {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        inner: Vec<Cmd>,
    },
}

/// The size of the text of the chart, at the scale of the text (see [`crate::domain::chart::text_scale`]).
pub fn font() -> f32 {
    crate::domain::chart::text_scale::scaled(11.0)
}

/// The line height of that text.
pub fn line_height() -> f32 {
    font() * 1.3
}

/// Finds the largest readable font size that fits a label inside its drawing.
pub fn fitted_size(
    text: &str,
    preferred: f32,
    width: f32,
    height: f32,
    measure: impl Fn(f32) -> f32,
) -> Option<f32> {
    let min = preferred.min(6.0);
    if text.is_empty() || width < 6.0 || height < min * 1.3 + 2.0 {
        return None;
    }
    let fits = |size: f32| measure(size) + 6.0 <= width && size * 1.3 + 2.0 <= height;
    if !fits(min) {
        return None;
    }
    let mut lo = min;
    let mut hi = preferred.max(min);
    for _ in 0..10 {
        let mid = (lo + hi) / 2.0;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo)
}

pub fn fitted_origin(
    bounds: (f32, f32, f32, f32),
    text_w: f32,
    size: f32,
    align: Align,
    valign: VAlign,
) -> (f32, f32) {
    let (x, y, w, h) = bounds;
    let left = match align {
        Align::Left => x + 3.0,
        Align::Center => x + (w - text_w) / 2.0,
        Align::Right => x + w - text_w - 3.0,
    };
    let top = match valign {
        VAlign::Top => y + 1.0,
        VAlign::Auto | VAlign::Middle => y + (h - size * 1.3) / 2.0,
        VAlign::Bottom => y + h - size * 1.3 - 1.0,
    };
    (left, top)
}

/// A `0xRRGGBB` color with an alpha, as the drawing commands take it.
pub fn rgb_alpha(color: u32, alpha: f32) -> Hsla {
    let mut hsla: Hsla = rgb(color).into();
    hsla.a = alpha;
    hsla
}

/// A theme color with another alpha.
pub fn with_alpha(color: Rgba, alpha: f32) -> Hsla {
    let mut hsla: Hsla = color.into();
    hsla.a = alpha;
    hsla
}

pub fn hsla(color: Rgba) -> Hsla {
    color.into()
}

/// Counts the leaf commands, for tests that bound the work of a frame.
#[cfg(test)]
pub fn count(cmds: &[Cmd]) -> usize {
    cmds.iter()
        .map(|c| match c {
            Cmd::Clip { inner, .. } => count(inner),
            _ => 1,
        })
        .sum()
}

/// The width of every rectangle of the frame, in order, for tests.
#[cfg(test)]
pub fn rect_widths(cmds: &[Cmd]) -> Vec<f32> {
    let mut out = Vec::new();
    for cmd in cmds {
        match cmd {
            Cmd::Rect { w, .. } => out.push(*w),
            Cmd::Clip { inner, .. } => out.extend(rect_widths(inner)),
            _ => {}
        }
    }
    out
}

/// Every text of the frame, in order, for tests.
#[cfg(test)]
pub fn texts(cmds: &[Cmd]) -> Vec<String> {
    let mut out = Vec::new();
    for cmd in cmds {
        match cmd {
            Cmd::Text { text, .. } | Cmd::Tag { text, .. } | Cmd::FittedText { text, .. } => {
                out.push(text.clone())
            }
            Cmd::Clip { inner, .. } => out.extend(texts(inner)),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod fitted_tests {
    use super::*;

    #[test]
    fn label_shrinks_then_disappears_in_a_tiny_box() {
        let measure = |size: f32| size * 4.0;
        let size = fitted_size("zone", 12.0, 35.0, 22.0, measure).unwrap();
        assert!((6.0..12.0).contains(&size));
        assert!(fitted_size("zone", 12.0, 20.0, 22.0, measure).is_none());
        let (x, y) = fitted_origin(
            (10.0, 20.0, 35.0, 22.0),
            measure(size),
            size,
            Align::Center,
            VAlign::Auto,
        );
        assert!((x - (10.0 + (35.0 - measure(size)) / 2.0)).abs() < 0.01);
        assert!(y > 20.0);
    }
}
