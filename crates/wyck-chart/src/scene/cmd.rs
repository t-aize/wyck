//! The drawing commands a frame is made of, in window coordinates.
//!
//! They name shapes, not a graphics library: the desktop painter turns them into gpui quads,
//! paths and text on screen, and the PNG renderer into pixels for a picture of the chart.
//! Building them is plain arithmetic, so it is tested without a window.

use super::color::{Hsla, Rgba, rgb};

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

/// The size of the text of the chart.
pub const FONT: f32 = 11.0;
/// The line height of that text.
pub const LINE: f32 = FONT * 1.3;

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
            Cmd::Text { text, .. } | Cmd::Tag { text, .. } => out.push(text.clone()),
            Cmd::Clip { inner, .. } => out.extend(texts(inner)),
            _ => {}
        }
    }
    out
}
