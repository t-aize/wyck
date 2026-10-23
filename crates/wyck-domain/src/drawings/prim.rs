//! The vocabulary of drawing geometry: a point on the screen (`P`), the shapes a tool makes
//! (`Prim`), the `Projection` that maps time and price to the screen, and the small helpers every
//! tool's shapes are built from. Plain arithmetic, tied to the chart only through `Projection`.

pub(crate) mod fib;

use super::look::Face;
use super::model::style::Dash;
use super::model::{Drawing, Point};
use crate::base::math::count_f32;

pub type P = (f32, f32);

/// One bar of the chart, for the tools that read the data: where it is on the screen, and its
/// prices as the chart holds them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarView {
    pub time: i64,
    pub x: f32,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// The plot area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

impl Rect {
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            l: 0.0,
            t: 0.0,
            r: w,
            b: h,
        }
    }
}

/// What the drawing code needs to know about the chart it is on.
pub trait Projection {
    fn plot(&self) -> Rect;
    /// Where a point is on the screen, or `None` when the chart has no data to place it by.
    fn to_screen(&self, point: Point) -> Option<P>;
    /// The point under a screen position: its time snapped to the nearest bar (or, past the last
    /// bar, to the bar it would be), its price from the height. With `magnet` the price snaps to
    /// the open, high, low or close of that bar when one is close.
    fn point_at(&self, x: f32, y: f32, magnet: bool) -> Option<Point>;
    /// Where a time falls among the bars, as a fractional bar index.
    fn index_of(&self, time_ms: i64) -> Option<f64>;
    /// The time `bars` bars after `time_ms` (before, when negative).
    fn shift_bars(&self, time_ms: i64, bars: f64) -> Option<i64>;
    /// A price as the symbol writes it.
    fn format_price(&self, price: f64) -> String;
    /// How much price the plot spans, for sizing things that have no size of their own.
    fn price_span(&self) -> f64;
    /// The bars opening from `from_ms` to `to_ms` (both included), oldest first. A chart that
    /// holds ticks, or no data, has none.
    fn bars_between(&self, _from_ms: i64, _to_ms: i64) -> Vec<BarView> {
        Vec::new()
    }
    /// The height of a price on the screen.
    fn y_of(&self, _price: f64) -> f32 {
        0.0
    }
    /// A price as the drawings hold it, as a real one (what a trader reads: 1.2345), for the
    /// tools that work out money.
    fn real_price(&self, raw: f64) -> f64 {
        raw
    }
    /// The smallest step of a real price, or 0 when it is not known.
    fn tick(&self) -> f64 {
        0.0
    }
    /// The size of one pip in real price, or 0 when the symbol does not provide it.
    fn pip(&self) -> f64 {
        0.0
    }
}

/// A shape to draw. Colors are `0xRRGGBB` with an alpha.
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Segment {
        a: P,
        b: P,
        color: u32,
        alpha: f32,
        width: f32,
        dash: Dash,
    },
    Rect {
        a: P,
        b: P,
        fill: Option<(u32, f32)>,
        stroke: Option<(u32, f32)>,
    },
    Ellipse {
        center: P,
        rx: f32,
        ry: f32,
        fill: Option<(u32, f32)>,
        stroke: Option<(u32, f32)>,
    },
    Polygon {
        points: Vec<P>,
        fill: (u32, f32),
    },
    Polyline {
        points: Vec<P>,
        color: u32,
        width: f32,
        alpha: f32,
    },
    /// Words on a rounded box, from its top left corner. The box is as wide as the longest row
    /// is estimated to be (see [`board_size`]).
    Board {
        tl: P,
        rows: Vec<String>,
        fg: u32,
        bg: (u32, f32),
        size: f32,
        bold: bool,
        face: Face,
    },
    Label {
        at: P,
        text: String,
        color: u32,
        /// A filled background behind the text, with its color and alpha.
        background: Option<(u32, f32)>,
        anchor: Anchor,
        /// The size of the text, in points.
        size: f32,
        bold: bool,
        face: Face,
    },
    FittedLabel {
        bounds: Rect,
        text: String,
        color: u32,
        background: Option<(u32, f32)>,
        align: super::look::HAlign,
        valign: super::look::VAlign,
        size: f32,
        bold: bool,
        face: Face,
    },
    /// A grip at a point of a selected drawing.
    Handle {
        at: P,
    },
}

/// Which point of a label sits on its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The label starts at the position and is centered vertically on it.
    Left,
    /// The label ends at the position.
    Right,
    /// The label is centered on the position.
    Center,
}

/// What part of a drawing a pointer is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// A grip: the index of the point.
    Handle(usize),
    /// The drawing itself.
    Body,
}

/// The size of the words a drawing writes by itself (prices, ratios).
pub fn label_size() -> f32 {
    crate::base::text_scale::scaled(11.0)
}

/// How far the words of a board are from its edge.
const BOARD_PAD_X: f32 = 8.0;
const BOARD_PAD_Y: f32 = 5.0;

/// The size of a board of `rows` in text of `size`: wide enough for the longest row (the width of
/// a letter is estimated, since the drawing code does not measure text), one line per row.
pub fn board_size(rows: &[String], size: f32) -> (f32, f32) {
    let chars = count_f32(rows.iter().map(|r| r.chars().count()).max().unwrap_or(0));
    (
        chars * size * 0.58 + BOARD_PAD_X * 2.0,
        count_f32(rows.len()) * size * 1.45 + BOARD_PAD_Y * 2.0,
    )
}

/// The row height and paddings of a board, for the code that paints it.
pub const fn board_metrics() -> (f32, f32, f32) {
    (1.45, BOARD_PAD_X, BOARD_PAD_Y)
}

/// How close the pointer must be to a line to be on it, and to a grip.
pub const LINE_REACH: f32 = 6.0;
pub(crate) const HANDLE_REACH: f32 = 9.0;

/// The distance from `p` to the segment `a` to `b`.
pub(crate) fn distance_to_segment(p: P, a: P, b: P) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let length_sq = abx * abx + aby * aby;
    if length_sq < 1e-9 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / length_sq).clamp(0.0, 1.0);
    let (cx, cy) = (a.0 + abx * t, a.1 + aby * t);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// The part of the line through `a` and `b` that is inside `rect`, as the two parameters `t`
/// where the line (at `a + t * (b - a)`) enters and leaves it, or `None` when it misses.
pub(crate) fn clip_line(a: P, b: P, rect: Rect) -> Option<(f32, f32)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    for (p, q) in [
        (-dx, a.0 - rect.l),
        (dx, rect.r - a.0),
        (-dy, a.1 - rect.t),
        (dy, rect.b - a.1),
    ] {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

/// The line through `a` and `b`, cut to `rect` where it is extended: `before` past `a`, `after`
/// past `b`. Where it is not extended it ends at the point.
pub fn extended(a: P, b: P, before: bool, after: bool, rect: Rect) -> Option<(P, P)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
        return Some((a, b));
    }
    let (t_in, t_out) = clip_line(a, b, rect)?;
    let start = if before { t_in } else { 0.0 };
    let end = if after { t_out } else { 1.0 };
    if start > end {
        return None;
    }
    let at = |t: f32| (a.0 + dx * t, a.1 + dy * t);
    Some((at(start), at(end)))
}

pub(crate) fn seg(a: P, b: P, color: u32, alpha: f32, width: f32, dash: Dash) -> Prim {
    Prim::Segment {
        a,
        b,
        color,
        alpha,
        width,
        dash,
    }
}

/// How long a time span is, in the two largest units: `3d 4h`, `2h 15m`, `45m`, `30s`.
pub fn duration_text(ms: i64) -> String {
    let seconds = ms.abs() / 1_000;
    let (days, hours, minutes) = (
        seconds / 86_400,
        seconds % 86_400 / 3_600,
        seconds % 3_600 / 60,
    );
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{seconds}s")
    }
}

/// The screen positions of a drawing's points, or `None` if any cannot be placed.
pub fn anchors(drawing: &Drawing, proj: &dyn Projection) -> Option<Vec<P>> {
    drawing
        .points
        .iter()
        .map(|point| proj.to_screen(*point))
        .collect()
}

/// The first `N` of `items`, when there are that many. The shapes of a tool ask for the points the
/// tool takes (see `Tool::anchors`), which `Drawing::normalized` guarantees; a drawing with fewer
/// gets no shapes instead of a panic.
pub(crate) fn lead<T: Copy, const N: usize>(items: &[T]) -> Option<[T; N]> {
    items.first_chunk().copied()
}

/// The head of an arrow that runs from `a` to `b`, if the two are apart.
pub(crate) fn arrow_head(a: P, b: P, color: u32, width: f32) -> Option<Prim> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 1.0 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let size = 8.0 + width * 2.0;
    let base = (b.0 - ux * size, b.1 - uy * size);
    let (nx, ny) = (-uy * size * 0.45, ux * size * 0.45);
    Some(Prim::Polygon {
        points: vec![b, (base.0 + nx, base.1 + ny), (base.0 - nx, base.1 - ny)],
        fill: (color, 1.0),
    })
}
