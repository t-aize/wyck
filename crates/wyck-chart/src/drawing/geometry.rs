//! Turning a drawing into shapes on the screen, and finding out what is under the pointer.
//!
//! Everything works through a [`Projection`], which knows how the chart maps time and price to
//! screen positions. That is the only thing tied to the chart, so this file is plain arithmetic
//! and is tested with a simple linear projection.
//!
//! Positions are in the plot's own coordinates, the top left of the plot being (0, 0).

use super::decor;
use super::extras;
use super::figures;
use super::look::{LabelSide, LevelText};
use super::model::{Dash, Drawing, Level, Point, Style, Tool};

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
pub const LABEL_SIZE: f32 = 11.0;

/// How far the words of a board are from its edge.
const BOARD_PAD_X: f32 = 8.0;
const BOARD_PAD_Y: f32 = 5.0;

/// The size of a board of `rows` in text of `size`: wide enough for the longest row (the width of
/// a letter is estimated, since the drawing code does not measure text), one line per row.
pub fn board_size(rows: &[String], size: f32) -> (f32, f32) {
    let chars = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32;
    (
        chars * size * 0.58 + BOARD_PAD_X * 2.0,
        rows.len() as f32 * size * 1.45 + BOARD_PAD_Y * 2.0,
    )
}

/// The row height and paddings of a board, for the code that paints it.
pub const fn board_metrics() -> (f32, f32, f32) {
    (1.45, BOARD_PAD_X, BOARD_PAD_Y)
}

/// How close the pointer must be to a line to be on it, and to a grip.
pub const LINE_REACH: f32 = 6.0;
pub const HANDLE_REACH: f32 = 9.0;

/// The distance from `p` to the segment `a` to `b`.
pub fn distance_to_segment(p: P, a: P, b: P) -> f32 {
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
pub fn clip_line(a: P, b: P, rect: Rect) -> Option<(f32, f32)> {
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

pub(super) fn seg(a: P, b: P, color: u32, alpha: f32, width: f32, dash: Dash) -> Prim {
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

/// The head of an arrow that runs from `a` to `b`, if the two are apart.
pub(super) fn arrow_head(a: P, b: P, color: u32, width: f32) -> Option<Prim> {
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

/// The shapes of a drawing (without grips), as they show when it is selected.
pub fn prims(drawing: &Drawing, proj: &dyn Projection) -> Vec<Prim> {
    prims_with(drawing, proj, true)
}

/// The shapes of a drawing (without grips). `selected` says whether it is the one selected: a
/// position can keep its stats for then.
pub fn prims_with(drawing: &Drawing, proj: &dyn Projection, selected: bool) -> Vec<Prim> {
    let Some(pts) = anchors(drawing, proj) else {
        return Vec::new();
    };
    let rect = proj.plot();
    let style = &drawing.style;
    let (color, width, dash) = (style.color, style.width, style.dash);
    let fill = fill_of(style);
    let mut out = Vec::new();

    match drawing.tool {
        Tool::TrendLine => {
            if let Some((a, b)) =
                extended(pts[0], pts[1], style.extend_left, style.extend_right, rect)
            {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::Ray => {
            if let Some((a, b)) = extended(pts[0], pts[1], style.extend_left, true, rect) {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::ExtendedLine => {
            if let Some((a, b)) = extended(pts[0], pts[1], true, true, rect) {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::HorizontalLine => {
            let y = pts[0].1;
            out.push(seg((rect.l, y), (rect.r, y), color, 1.0, width, dash));
            if style.labels {
                out.push(Prim::Label {
                    at: (rect.r - 6.0, y),
                    text: proj.format_price(drawing.points[0].p),
                    color: 0x0a0a0a,
                    background: Some((color, 1.0)),
                    anchor: Anchor::Right,
                    size: LABEL_SIZE,
                    bold: false,
                });
            }
        }
        Tool::HorizontalRay => {
            let y = pts[0].1;
            out.push(seg((pts[0].0, y), (rect.r, y), color, 1.0, width, dash));
        }
        Tool::VerticalLine => {
            let x = pts[0].0;
            out.push(seg((x, rect.t), (x, rect.b), color, 1.0, width, dash));
        }
        Tool::CrossLine => {
            let (x, y) = pts[0];
            out.push(seg((rect.l, y), (rect.r, y), color, 1.0, width, dash));
            out.push(seg((x, rect.t), (x, rect.b), color, 1.0, width, dash));
        }
        Tool::Arrow => {
            let (a, b) = (pts[0], pts[1]);
            let start = extended(a, b, style.extend_left, false, rect).map_or(a, |(s, _)| s);
            out.push(seg(start, b, color, 1.0, width, dash));
            out.extend(arrow_head(a, b, color, width));
        }
        Tool::ArrowPath => {
            for pair in pts.windows(2) {
                out.push(seg(pair[0], pair[1], color, 1.0, width, dash));
            }
            // The head follows the last segment that has a length, so it stays put while the
            // pointer sits on the last point placed.
            let last = pts[pts.len() - 1];
            let from = pts.iter().rev().find(|p| {
                let (dx, dy) = (last.0 - p.0, last.1 - p.1);
                dx * dx + dy * dy > 1.0
            });
            if let Some(from) = from {
                out.extend(arrow_head(*from, last, color, width));
            }
        }
        Tool::ParallelChannel => {
            let (a, b, c) = (pts[0], pts[1], pts[2]);
            // The second line is the first one moved vertically to pass through the third point.
            let dx = b.0 - a.0;
            let line_y_at_c = if dx.abs() < 1e-6 {
                a.1
            } else {
                a.1 + (b.1 - a.1) * (c.0 - a.0) / dx
            };
            let offset = c.1 - line_y_at_c;
            let (a, b) = stretch(a, b, style, rect);
            let (a2, b2) = ((a.0, a.1 + offset), (b.0, b.1 + offset));
            if let Some(fill) = fill {
                out.push(Prim::Polygon {
                    points: vec![a, b, b2, a2],
                    fill,
                });
            }
            out.push(seg(a, b, color, 1.0, width, dash));
            out.push(seg(a2, b2, color, 1.0, width, dash));
            if style.middle {
                let (m1, m2) = (
                    ((a.0 + a2.0) / 2.0, (a.1 + a2.1) / 2.0),
                    ((b.0 + b2.0) / 2.0, (b.1 + b2.1) / 2.0),
                );
                out.push(seg(m1, m2, color, 0.6, 1.0, Dash::Dashed));
            }
        }
        Tool::FibRetracement => fib_prims(
            drawing,
            &pts,
            proj,
            |ratio| {
                // 0 at the second point, 1 at the first.
                drawing.points[1].p + (drawing.points[0].p - drawing.points[1].p) * ratio
            },
            &mut out,
        ),
        Tool::FibExtension => fib_prims(
            drawing,
            &pts,
            proj,
            |ratio| {
                // The move from the first to the second point, repeated from the third.
                drawing.points[2].p + (drawing.points[1].p - drawing.points[0].p) * ratio
            },
            &mut out,
        ),
        Tool::Rectangle => {
            let (mut l, mut r) = (pts[0].0.min(pts[1].0), pts[0].0.max(pts[1].0));
            if style.extend_left {
                l = l.min(rect.l);
            }
            if style.extend_right {
                r = r.max(rect.r);
            }
            let (t, b) = (pts[0].1.min(pts[1].1), pts[0].1.max(pts[1].1));
            if dash == Dash::Solid {
                out.push(Prim::Rect {
                    a: (l, t),
                    b: (r, b),
                    fill,
                    stroke: Some((color, width)),
                });
            } else {
                if fill.is_some() {
                    out.push(Prim::Rect {
                        a: (l, t),
                        b: (r, b),
                        fill,
                        stroke: None,
                    });
                }
                for (p, q) in [
                    ((l, t), (r, t)),
                    ((r, t), (r, b)),
                    ((r, b), (l, b)),
                    ((l, b), (l, t)),
                ] {
                    out.push(seg(p, q, color, 1.0, width, dash));
                }
            }
        }
        Tool::Ellipse => out.push(Prim::Ellipse {
            center: ((pts[0].0 + pts[1].0) / 2.0, (pts[0].1 + pts[1].1) / 2.0),
            rx: (pts[1].0 - pts[0].0).abs() / 2.0,
            ry: (pts[1].1 - pts[0].1).abs() / 2.0,
            fill,
            stroke: Some((color, width)),
        }),
        Tool::Triangle => {
            if let Some(fill) = fill {
                out.push(Prim::Polygon {
                    points: pts.clone(),
                    fill,
                });
            }
            for i in 0..3 {
                out.push(seg(pts[i], pts[(i + 1) % 3], color, 1.0, width, dash));
            }
        }
        // A stroke has no ends to cap and nowhere for words: its points are handed over whole.
        Tool::Brush | Tool::Highlighter => {
            out.push(Prim::Polyline {
                points: pts,
                color,
                width,
                alpha: if drawing.tool == Tool::Highlighter {
                    HIGHLIGHT_ALPHA
                } else {
                    1.0
                },
            });
            fade_lines(&mut out, style.opacity);
            return out;
        }
        Tool::Measure => measure_prims(drawing, &pts, proj, &mut out),
        Tool::LongPosition | Tool::ShortPosition => {
            position_prims(drawing, &pts, proj, selected, &mut out);
        }
        Tool::Text => {
            let text = if drawing.text.is_empty() {
                "Text"
            } else {
                drawing.text.as_str()
            };
            // Each line of the words is its own label, one under the other.
            for (row, line) in text.lines().enumerate() {
                out.push(Prim::Label {
                    at: (
                        pts[0].0 + 6.0,
                        pts[0].1 + row as f32 * style.text_size * 1.4,
                    ),
                    text: line.to_owned(),
                    color: style.text_color(),
                    background: None,
                    anchor: Anchor::Left,
                    size: style.text_size,
                    bold: style.bold,
                });
            }
        }
        Tool::PriceLabel => out.push(Prim::Label {
            at: pts[0],
            text: proj.format_price(drawing.points[0].p),
            color: style.text_color.unwrap_or(0x0a0a0a),
            background: Some((color, 1.0)),
            anchor: Anchor::Left,
            size: style.text_size,
            bold: style.bold,
        }),
        Tool::Unknown => {}
        _ => {
            figures::prims(drawing, &pts, proj, &mut out);
            extras::prims(drawing, &pts, proj, &mut out);
        }
    }
    decor::decorate(drawing, &pts, rect, &mut out);
    // A tag with nothing to say (every part of a measure switched off) is not drawn.
    out.retain(|prim| !matches!(prim, Prim::Label { text, .. } if text.is_empty()));
    fade_lines(&mut out, style.opacity);
    // A copy of bars is all fills: the opacity fades those too.
    if matches!(drawing.tool, Tool::BarsPattern | Tool::GhostFeed) && style.opacity < 1.0 {
        for prim in &mut out {
            if let Prim::Rect {
                fill: Some((_, alpha)),
                ..
            }
            | Prim::Polygon {
                fill: (_, alpha), ..
            } = prim
            {
                *alpha *= style.opacity;
            }
        }
    }
    out
}

/// Fades the lines of a drawing to the opacity of its style. Fills and words keep their own.
fn fade_lines(out: &mut [Prim], opacity: f32) {
    if opacity >= 1.0 {
        return;
    }
    for prim in out {
        match prim {
            Prim::Segment { alpha, .. } | Prim::Polyline { alpha, .. } => *alpha *= opacity,
            Prim::Rect { stroke, .. } | Prim::Ellipse { stroke, .. } => {
                if let Some((_, alpha)) = stroke {
                    *alpha *= opacity;
                }
            }
            _ => {}
        }
    }
}

/// How opaque the stroke of a highlighter is.
const HIGHLIGHT_ALPHA: f32 = 0.35;

fn fib_prims(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    price_at: impl Fn(f64) -> f64,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (mut left, mut right) = pts
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), p| {
            (l.min(p.0), r.max(p.0))
        });
    if style.extend_left {
        left = rect.l;
    }
    if style.extend_right {
        right = rect.r;
    }
    let mut previous: Option<(f32, u32)> = None;
    for level in visible_levels(drawing) {
        let ratio = if drawing.reverse {
            1.0 - level.value
        } else {
            level.value
        };
        let price = price_at(ratio);
        let Some(at) = proj.to_screen(Point {
            t: drawing.points[0].t,
            p: price,
        }) else {
            continue;
        };
        let y = at.1;
        if style.fill
            && let Some((previous_y, _)) = previous
        {
            out.push(Prim::Rect {
                a: (left, previous_y),
                b: (right, y),
                fill: Some((level.color, style.fill_opacity)),
                stroke: None,
            });
        }
        out.push(seg(
            (left, y),
            (right, y),
            level.color,
            0.9,
            level.line_width(style.width),
            level.line_dash(style.dash),
        ));
        if style.labels {
            let text = level_label(style, level.value, Some(&proj.format_price(price)));
            // At the left of the levels, or at the right. Lines that run to the edge of the chart
            // leave no room outside, so the words go inside, just above the line.
            let on_left = style.label_side == LabelSide::Left;
            let edge = if on_left { left } else { right };
            let run_out = if on_left {
                style.extend_left
            } else {
                style.extend_right
            };
            let (x, anchor) = match (on_left, run_out) {
                (true, true) => (edge + 4.0, Anchor::Left),
                (true, false) => (edge - 4.0, Anchor::Right),
                (false, true) => (edge - 4.0, Anchor::Right),
                (false, false) => (edge + 4.0, Anchor::Left),
            };
            let y = if run_out { y - 8.0 } else { y };
            out.push(label((x, y), text, level.color, anchor, style));
        }
        previous = Some((y, level.color));
    }
    // The line that joins the points the levels come from.
    for pair in pts.windows(2) {
        out.push(seg(pair[0], pair[1], style.color, 0.6, 1.0, Dash::Dashed));
    }
}

/// The fill of a drawing, when it has one: its color and opacity.
pub(super) fn fill_of(style: &Style) -> Option<(u32, f32)> {
    (style.fill && style.fill_opacity > 0.0).then(|| (style.fill_color(), style.fill_opacity))
}

/// The segment `a` to `b` stretched to the plot's edges on the sides the style extends.
pub(super) fn stretch(a: P, b: P, style: &Style, rect: Rect) -> (P, P) {
    if !(style.extend_left || style.extend_right) {
        return (a, b);
    }
    // Extend towards the left and right of the screen, whichever point is on which side.
    let (first, second, swapped) = if a.0 <= b.0 {
        (a, b, false)
    } else {
        (b, a, true)
    };
    let (p, q) = extended(first, second, style.extend_left, style.extend_right, rect)
        .unwrap_or((first, second));
    if swapped { (q, p) } else { (p, q) }
}

/// The levels of a drawing that show, in increasing order.
pub(super) fn visible_levels(drawing: &Drawing) -> Vec<Level> {
    let mut levels: Vec<Level> = drawing.levels().into_iter().filter(|l| l.visible).collect();
    levels.sort_by(|a, b| a.value.total_cmp(&b.value));
    levels
}

/// A level's value as the chart writes it: up to three decimals, no trailing zeros.
pub fn level_text(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// The caption of a level as the style asks for it: `price` is the price of the level, for a tool
/// that knows one.
pub(super) fn level_label(style: &Style, value: f64, price: Option<&str>) -> String {
    match (style.level_text, price) {
        (LevelText::Auto, Some(price)) => format!("{} ({price})", level_text(value)),
        (LevelText::Percent, _) => format!("{}%", level_text(value * 100.0)),
        (LevelText::Price, Some(price)) => price.to_owned(),
        _ => level_text(value),
    }
}

/// Words a drawing writes, in the size and weight its style asks for.
pub(super) fn label(at: P, text: String, color: u32, anchor: Anchor, style: &Style) -> Prim {
    Prim::Label {
        at,
        text,
        color,
        background: None,
        anchor,
        size: style.text_size,
        bold: style.bold,
    }
}

fn measure_prims(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (drawing.points[0], drawing.points[1]);
    let up = b.p - a.p >= 0.0;
    // The color of the drawing is for a move up, the one of the look for a move down.
    let color = if up {
        style.color
    } else {
        style.measure.down_color
    };
    let width = style.width;
    out.push(Prim::Rect {
        a: pts[0],
        b: pts[1],
        fill: (style.fill && style.fill_opacity > 0.0)
            .then(|| (style.fill_color.unwrap_or(color), style.fill_opacity)),
        stroke: Some((color, width)),
    });
    // A cross through the middle.
    let (mx, my) = ((pts[0].0 + pts[1].0) / 2.0, (pts[0].1 + pts[1].1) / 2.0);
    out.push(seg(
        (mx, pts[0].1),
        (mx, pts[1].1),
        color,
        0.9,
        width,
        style.dash,
    ));
    out.push(seg(
        (pts[0].0, my),
        (pts[1].0, my),
        color,
        0.9,
        width,
        style.dash,
    ));

    if !style.labels {
        return;
    }
    let text = [
        extras::move_text(proj, a, b, &style.measure),
        extras::span_text(proj, a, b, &style.measure),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("   ");
    let top = pts[0].1.min(pts[1].1);
    out.push(Prim::Label {
        at: (mx, top - 14.0),
        text,
        color: style.text_color.unwrap_or(0xffffff),
        background: Some((color, 0.9)),
        anchor: Anchor::Center,
        size: style.text_size,
        bold: style.bold,
    });
}

fn position_prims(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    selected: bool,
    out: &mut Vec<Prim>,
) {
    let long = drawing.tool == Tool::LongPosition;
    let style = &drawing.style;
    let cfg = &style.position;
    let entry = drawing.points[0].p;
    let (stop, target) = (drawing.points[1].p, drawing.points[2].p);
    let (left, right) = (pts[0].0, pts[3].0.max(pts[0].0 + 1.0));
    let (y_entry, y_stop, y_target) = (pts[0].1, pts[1].1, pts[2].1);
    let width = style.width;

    if style.fill && style.fill_opacity > 0.0 {
        out.push(Prim::Rect {
            a: (left, y_entry),
            b: (right, y_target),
            fill: Some((cfg.target_color, style.fill_opacity)),
            stroke: None,
        });
        out.push(Prim::Rect {
            a: (left, y_entry),
            b: (right, y_stop),
            fill: Some((cfg.stop_color, style.fill_opacity)),
            stroke: None,
        });
    }
    out.push(seg(
        (left, y_entry),
        (right, y_entry),
        cfg.entry_color,
        0.9,
        width,
        style.dash,
    ));
    out.push(seg(
        (left, y_target),
        (right, y_target),
        cfg.target_color,
        1.0,
        width,
        Dash::Solid,
    ));
    out.push(seg(
        (left, y_stop),
        (right, y_stop),
        cfg.stop_color,
        1.0,
        width,
        Dash::Solid,
    ));
    if !style.labels {
        return;
    }

    let pct = |price: f64| {
        if entry != 0.0 {
            (price - entry) / entry * 100.0
        } else {
            0.0
        }
    };
    let reward = (target - entry).abs();
    let risk = (entry - stop).abs();
    let ratio = if risk > 0.0 { reward / risk } else { 0.0 };
    // A long profits when the target is above the entry; the drawing is only right if the stop is
    // on the other side. Say so when it is not, instead of showing a nonsense ratio.
    let sensible = if long {
        target > entry && stop < entry
    } else {
        target < entry && stop > entry
    };
    let tick = proj.tick();
    let stats = sensible
        .then(|| {
            cfg.stats(
                proj.real_price(entry),
                proj.real_price(stop),
                proj.real_price(target),
                tick,
            )
        })
        .flatten();
    // The stats are the amounts and the size: shown all the time, or only while it is selected.
    let details = selected || cfg.always_stats;
    let ticks_of = |price: f64| {
        if tick > 0.0 {
            (proj.real_price(price) - proj.real_price(entry)).abs() / tick
        } else {
            0.0
        }
    };

    // The words of a level tag: what the switches ask for, or one short figure when compact.
    let level_text = |name: &str, price: f64, amount: Option<f64>| -> String {
        let amount = amount
            .filter(|_| cfg.show_amounts && details)
            .map(|a| cfg.format_amount(a));
        if cfg.compact {
            return amount
                .or_else(|| cfg.show_percent.then(|| format!("{:+.2}%", pct(price))))
                .unwrap_or_else(|| proj.format_price(price));
        }
        let mut parts = vec![name.to_owned()];
        if cfg.show_price {
            parts.push(proj.format_price(price));
        }
        if cfg.show_percent {
            parts.push(format!("({:+.2}%)", pct(price)));
        }
        if cfg.show_ticks && tick > 0.0 {
            parts.push(format!("{} ticks", ticks_of(price).round()));
        }
        parts.extend(amount);
        parts.join(" ")
    };
    let ink = style.text_color;
    let (size, bold) = (style.text_size, style.bold);
    let middle = (left + right) / 2.0;

    out.push(Prim::Label {
        at: (
            middle,
            y_target + if y_target < y_entry { -12.0 } else { 12.0 },
        ),
        text: level_text("Target", target, stats.map(|s| s.profit)),
        color: ink.unwrap_or(0x0a0a0a),
        background: Some((cfg.target_color, 0.95)),
        anchor: Anchor::Center,
        size,
        bold,
    });
    out.push(Prim::Label {
        at: (middle, y_stop + if y_stop < y_entry { -12.0 } else { 12.0 }),
        text: level_text("Stop", stop, stats.map(|s| -s.loss)),
        color: ink.unwrap_or(0x0a0a0a),
        background: Some((cfg.stop_color, 0.95)),
        anchor: Anchor::Center,
        size,
        bold,
    });

    let name = if long { "Long" } else { "Short" };
    let entry_text = if !sensible {
        "Stop and target are on the wrong sides".to_owned()
    } else {
        let mut parts = vec![name.to_owned()];
        if let Some(stats) = stats.filter(|_| details) {
            if cfg.show_qty {
                let capped = if stats.capped && !cfg.compact {
                    " (leverage)"
                } else {
                    ""
                };
                parts.push(if cfg.compact {
                    cfg.format_qty(stats.qty)
                } else {
                    format!("Qty {}{capped}", cfg.format_qty(stats.qty))
                });
            }
            if cfg.show_ratio {
                parts.push(if cfg.compact {
                    format!("{ratio:.1}R")
                } else {
                    format!("R/R {ratio:.2}")
                });
            }
            if cfg.show_risk && !cfg.compact {
                parts.push(format!("Risk {}", cfg.format_plain(stats.risk_amount)));
            }
        } else if cfg.show_ratio {
            parts.push(if cfg.compact {
                format!("{ratio:.1}R")
            } else {
                format!("R/R {ratio:.2}")
            });
        }
        parts.join(" | ")
    };
    out.push(Prim::Label {
        at: (middle, y_entry),
        text: entry_text,
        color: ink.unwrap_or(0xffffff),
        background: Some((0x2a2a2a, 0.95)),
        anchor: Anchor::Center,
        size,
        bold,
    });
}

/// The grips of a drawing: where its points are on the screen.
pub fn handles(drawing: &Drawing, proj: &dyn Projection) -> Vec<P> {
    match drawing.tool {
        // A brush has too many points for grips: it is moved as a whole.
        tool if tool.is_freehand() => Vec::new(),
        // The far corner of the square is where the pointer would put it on the grid.
        Tool::GannSquareFixed => anchors(drawing, proj)
            .map(|pts| vec![pts[0], extras::fixed_square_corner(pts[0], pts[1])])
            .unwrap_or_default(),
        tool if tool.is_box() => anchors(drawing, proj)
            .map(|pts| box_handles(pts[0], pts[1]))
            .unwrap_or_default(),
        _ => anchors(drawing, proj).unwrap_or_default(),
    }
}

/// The grips of a shape drawn in the box between two opposite corners: those two, the other two
/// corners, then the middle of each side (the side through the first corner's height, the second
/// corner's height, the first corner's time, the second's).
pub fn box_handles(a: P, b: P) -> Vec<P> {
    let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    vec![
        a,
        b,
        (a.0, b.1),
        (b.0, a.1),
        (mx, a.1),
        (mx, b.1),
        (a.0, my),
        (b.0, my),
    ]
}

fn label_box(at: P, text: &str, anchor: Anchor, size: f32) -> (P, P) {
    let width = text.chars().count() as f32 * size * 0.6 + 14.0;
    let half = size * 0.8;
    let (l, r) = match anchor {
        Anchor::Left => (at.0, at.0 + width),
        Anchor::Right => (at.0 - width, at.0),
        Anchor::Center => (at.0 - width / 2.0, at.0 + width / 2.0),
    };
    ((l, at.1 - half), (r, at.1 + half))
}

fn inside_rect(p: P, a: P, b: P) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

/// What part of the drawing a screen position is on, if any. Grips win over the body, so a
/// drawing can be picked up by its ends even where lines cross.
pub fn hit(drawing: &Drawing, proj: &dyn Projection, at: P, with_handles: bool) -> Option<Part> {
    if with_handles && !drawing.locked {
        let grips = handles(drawing, proj);
        let mut best: Option<(usize, f32)> = None;
        for (index, grip) in grips.iter().enumerate() {
            let distance = ((at.0 - grip.0).powi(2) + (at.1 - grip.1).powi(2)).sqrt();
            if distance <= HANDLE_REACH && best.is_none_or(|(_, d)| distance < d) {
                best = Some((index, distance));
            }
        }
        if let Some((index, _)) = best {
            return Some(Part::Handle(index));
        }
    }
    let reach = LINE_REACH + drawing.style.width / 2.0;
    for prim in prims(drawing, proj) {
        let on = match &prim {
            Prim::Segment { a, b, .. } => distance_to_segment(at, *a, *b) <= reach,
            Prim::Rect { a, b, fill, stroke } => {
                let edges = [
                    (*a, (b.0, a.1)),
                    ((b.0, a.1), *b),
                    (*b, (a.0, b.1)),
                    ((a.0, b.1), *a),
                ];
                (stroke.is_some()
                    && edges
                        .iter()
                        .any(|(p, q)| distance_to_segment(at, *p, *q) <= reach))
                    || (fill.is_some_and(|(_, alpha)| alpha > 0.1) && inside_rect(at, *a, *b))
            }
            Prim::Ellipse {
                center,
                rx,
                ry,
                fill,
                ..
            } => {
                if *rx < 1.0 || *ry < 1.0 {
                    false
                } else {
                    let (nx, ny) = ((at.0 - center.0) / rx, (at.1 - center.1) / ry);
                    let radius = (nx * nx + ny * ny).sqrt();
                    let edge = (reach / rx.min(*ry)).max(0.05);
                    (radius - 1.0).abs() <= edge || (fill.is_some() && radius < 1.0)
                }
            }
            Prim::Polygon { points, .. } => points.len() >= 3 && point_in_polygon(at, points),
            Prim::Polyline { points, .. } => points
                .windows(2)
                .any(|pair| distance_to_segment(at, pair[0], pair[1]) <= reach),
            Prim::Label {
                at: position,
                text,
                anchor,
                size,
                ..
            } => {
                let (a, b) = label_box(*position, text, *anchor, *size);
                inside_rect(at, a, b)
            }
            Prim::Board { tl, rows, size, .. } => {
                let (w, h) = board_size(rows, *size);
                inside_rect(at, *tl, (tl.0 + w, tl.1 + h))
            }
            Prim::Handle { .. } => false,
        };
        // The dashed guide lines of a fib or a channel are not part of what is clickable, but
        // they are segments like any other; they sit inside the levels' boxes anyway.
        if on {
            return Some(Part::Body);
        }
    }
    None
}

fn point_in_polygon(p: P, polygon: &[P]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::drawing::model::Style;

    /// A projection where time is 1 pixel per second from 0, and price is 1 pixel per unit
    /// upwards from the bottom of a 1000 by 500 plot. Bars are one minute long.
    pub struct Linear;

    impl Projection for Linear {
        fn plot(&self) -> Rect {
            Rect::new(1_000.0, 500.0)
        }
        fn to_screen(&self, point: Point) -> Option<P> {
            Some(((point.t / 1_000) as f32, 500.0 - point.p as f32))
        }
        fn point_at(&self, x: f32, y: f32, _magnet: bool) -> Option<Point> {
            // Snap the time to whole minutes, like snapping to a bar.
            let seconds = (x / 60.0).round() as i64 * 60;
            Some(Point {
                t: seconds * 1_000,
                p: f64::from(500.0 - y),
            })
        }
        fn index_of(&self, time_ms: i64) -> Option<f64> {
            Some(time_ms as f64 / 60_000.0)
        }
        fn shift_bars(&self, time_ms: i64, bars: f64) -> Option<i64> {
            Some(time_ms + (bars * 60_000.0).round() as i64)
        }
        fn format_price(&self, price: f64) -> String {
            format!("{price:.1}")
        }
        fn price_span(&self) -> f64 {
            500.0
        }
        fn bars_between(&self, from_ms: i64, to_ms: i64) -> Vec<BarView> {
            // A bar a minute, opening at that minute at a price of 100 plus its number.
            let first = (from_ms.max(0) + 59_999) / 60_000;
            let last = (to_ms / 60_000).min(first + 1_999);
            (first..=last)
                .map(|i| BarView {
                    time: i * 60_000,
                    x: (i * 60) as f32,
                    open: 100.0 + i as f64,
                    high: 103.0 + i as f64,
                    low: 98.0 + i as f64,
                    close: 101.0 + i as f64,
                    volume: 10.0,
                })
                .collect()
        }
        fn y_of(&self, price: f64) -> f32 {
            500.0 - price as f32
        }
    }

    pub fn drawing(tool: Tool, points: &[(i64, f64)]) -> Drawing {
        let mut drawing = Drawing::new(
            1,
            tool,
            points
                .iter()
                .map(|&(t, p)| Point { t: t * 1_000, p })
                .collect(),
        );
        drawing.style = Style {
            width: 1.0,
            ..tool.default_style()
        };
        drawing
    }

    #[test]
    fn a_point_is_measured_against_a_segment() {
        assert_eq!(
            distance_to_segment((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)),
            3.0
        );
        assert_eq!(
            distance_to_segment((-4.0, 3.0), (0.0, 0.0), (10.0, 0.0)),
            5.0
        );
        assert_eq!(
            distance_to_segment((1.0, 1.0), (2.0, 2.0), (2.0, 2.0)),
            2f32.sqrt()
        );
    }

    #[test]
    fn a_line_is_extended_to_the_edges_of_the_plot() {
        let rect = Rect::new(100.0, 100.0);
        let (a, b) = extended((40.0, 50.0), (60.0, 50.0), true, true, rect).unwrap();
        assert_eq!((a, b), ((0.0, 50.0), (100.0, 50.0)));
        let (a, b) = extended((40.0, 50.0), (60.0, 50.0), false, true, rect).unwrap();
        assert_eq!(
            (a, b),
            ((40.0, 50.0), (100.0, 50.0)),
            "a ray starts at its point"
        );
        let (a, b) = extended((40.0, 40.0), (60.0, 60.0), true, true, rect).unwrap();
        assert!(
            (a.0 - 0.0).abs() < 1e-3 && (b.0 - 100.0).abs() < 1e-3,
            "{a:?} {b:?}"
        );
        assert!(
            extended((0.0, 200.0), (10.0, 210.0), true, true, rect).is_none(),
            "misses the plot"
        );
    }

    #[test]
    fn a_trend_line_is_one_segment_between_its_points() {
        let d = drawing(Tool::TrendLine, &[(100, 100.0), (400, 300.0)]);
        let shapes = prims(&d, &Linear);
        assert_eq!(shapes.len(), 1);
        assert!(matches!(
            shapes[0],
            Prim::Segment {
                a: (100.0, 400.0),
                b: (400.0, 200.0),
                ..
            }
        ));
    }

    #[test]
    fn the_opacity_fades_the_lines_and_leaves_the_rest() {
        let mut d = drawing(Tool::TrendLine, &[(100, 100.0), (400, 300.0)]);
        d.style.opacity = 0.5;
        let Prim::Segment { alpha, .. } = prims(&d, &Linear)[0] else {
            panic!("a segment");
        };
        assert!((alpha - 0.5).abs() < 1e-6);

        // The outline of a box fades too, its fill has an opacity of its own.
        let mut rect = drawing(Tool::Rectangle, &[(100, 100.0), (400, 300.0)]);
        rect.style.opacity = 0.4;
        let faded = prims(&rect, &Linear).into_iter().any(|prim| {
            matches!(prim, Prim::Rect { stroke: Some((_, alpha)), .. } if alpha <= 0.4 + 1e-6)
        });
        assert!(faded);
    }

    #[test]
    fn a_horizontal_line_spans_the_plot_and_says_its_price() {
        let d = drawing(Tool::HorizontalLine, &[(300, 250.0)]);
        let shapes = prims(&d, &Linear);
        assert!(matches!(
            shapes[0],
            Prim::Segment {
                a: (0.0, 250.0),
                b: (1000.0, 250.0),
                ..
            }
        ));
        assert!(matches!(&shapes[1], Prim::Label { text, .. } if text == "250.0"));
    }

    #[test]
    fn fib_levels_sit_between_the_two_prices() {
        let d = drawing(Tool::FibRetracement, &[(100, 100.0), (500, 200.0)]);
        let shapes = prims(&d, &Linear);
        let labels: Vec<&str> = shapes
            .iter()
            .filter_map(|s| match s {
                Prim::Label { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        // 0 at the second point (200), 1 at the first (100), 0.5 in between.
        assert!(labels.contains(&"0 (200.0)"), "{labels:?}");
        assert!(labels.contains(&"1 (100.0)"), "{labels:?}");
        assert!(labels.contains(&"0.5 (150.0)"), "{labels:?}");
        assert!(labels.contains(&"0.618 (138.2)"), "{labels:?}");
    }

    #[test]
    fn a_fib_extension_repeats_the_first_move_from_the_third_point() {
        // The move is +100 (100 -> 200); from 150 the 1.618 level is 150 + 161.8.
        let d = drawing(
            Tool::FibExtension,
            &[(100, 100.0), (300, 200.0), (500, 150.0)],
        );
        let labels: Vec<String> = prims(&d, &Linear)
            .into_iter()
            .filter_map(|s| match s {
                Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"1.618 (311.8)".to_owned()), "{labels:?}");
        assert!(labels.contains(&"0 (150.0)".to_owned()), "{labels:?}");
    }

    #[test]
    fn a_channel_has_two_parallel_lines_a_third_point_apart() {
        let d = drawing(
            Tool::ParallelChannel,
            &[(0, 100.0), (400, 200.0), (200, 250.0)],
        );
        let segments: Vec<(P, P)> = prims(&d, &Linear)
            .into_iter()
            .filter_map(|s| match s {
                Prim::Segment { a, b, alpha, .. } => (alpha == 1.0).then_some((a, b)),
                _ => None,
            })
            .collect();
        assert_eq!(segments.len(), 2);
        let slope = |(a, b): (P, P)| (b.1 - a.1) / (b.0 - a.0);
        assert!(
            (slope(segments[0]) - slope(segments[1])).abs() < 1e-6,
            "parallel"
        );
        // The second line passes through the third point (200 s, 250).
        let (a, b) = segments[1];
        let y_at_200 = a.1 + (b.1 - a.1) * (200.0 - a.0) / (b.0 - a.0);
        assert!((y_at_200 - 250.0_f32).abs() < 1e-3, "{y_at_200}");
    }

    #[test]
    fn a_measure_reports_the_change_the_bars_and_the_time() {
        // 100 -> 150 over 10 minutes.
        let d = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
        let text = prims(&d, &Linear)
            .into_iter()
            .find_map(|s| match s {
                Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .unwrap();
        assert_eq!(text, "+50.0 (+50.00%)   10 bars, 10m");
        let down = drawing(Tool::Measure, &[(0, 200.0), (120, 150.0)]);
        let text = prims(&down, &Linear)
            .into_iter()
            .find_map(|s| match s {
                Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .unwrap();
        assert!(text.starts_with("-50.0 (-25.00%)"), "{text}");
    }

    #[test]
    fn a_position_shows_its_risk_and_reward() {
        // Long: entry 100, stop 90, target 130.
        let d = drawing(
            Tool::LongPosition,
            &[(100, 100.0), (100, 90.0), (100, 130.0), (400, 100.0)],
        );
        let labels: Vec<String> = prims(&d, &Linear)
            .into_iter()
            .filter_map(|s| match s {
                Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|t| t.contains("R/R 3.00")), "{labels:?}");
        assert!(
            labels
                .iter()
                .any(|t| t.starts_with("Target 130.0 (+30.00%)")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|t| t.starts_with("Stop 90.0 (-10.00%)")),
            "{labels:?}"
        );

        // A long with the stop above the entry is said to be wrong, not given a ratio.
        let wrong = drawing(
            Tool::LongPosition,
            &[(100, 100.0), (100, 120.0), (100, 130.0), (400, 100.0)],
        );
        assert!(prims(&wrong, &Linear).iter().any(|s| matches!(
            s, Prim::Label { text, .. } if text.contains("wrong sides")
        )));
        // A short mirrors it.
        let short = drawing(
            Tool::ShortPosition,
            &[(100, 100.0), (100, 110.0), (100, 80.0), (400, 100.0)],
        );
        assert!(prims(&short, &Linear).iter().any(|s| matches!(
            s, Prim::Label { text, .. } if text.starts_with("Short | Qty") && text.contains("R/R 2.00")
        )));
    }

    /// The words of the labels of a position, from top to bottom of the list.
    fn position_labels(d: &Drawing, selected: bool) -> Vec<String> {
        prims_with(d, &Linear, selected)
            .into_iter()
            .filter_map(|s| match s {
                Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    fn long_position() -> Drawing {
        // Entry 100, stop 90, target 130: with the defaults, 1% of 10 000 risked over 10 is 10
        // units, so the stop loses 100 and the target gains 300.
        drawing(
            Tool::LongPosition,
            &[(100, 100.0), (100, 90.0), (100, 130.0), (400, 100.0)],
        )
    }

    #[test]
    fn a_position_writes_its_size_and_its_money() {
        let labels = position_labels(&long_position(), true);
        assert!(
            labels
                .iter()
                .any(|t| t == "Long | Qty 10 | R/R 3.00 | Risk 100.00"),
            "{labels:?}"
        );
        assert!(labels.iter().any(|t| t.ends_with("+300.00")), "{labels:?}");
        assert!(labels.iter().any(|t| t.ends_with("-100.00")), "{labels:?}");
    }

    #[test]
    fn the_switches_choose_what_a_position_writes() {
        let mut d = long_position();
        d.style.position.show_qty = false;
        d.style.position.show_risk = false;
        d.style.position.show_amounts = false;
        d.style.position.show_price = false;
        let labels = position_labels(&d, true);
        assert!(labels.iter().any(|t| t == "Long | R/R 3.00"), "{labels:?}");
        assert!(labels.iter().any(|t| t == "Target (+30.00%)"), "{labels:?}");

        // Compact: one short figure per tag.
        let mut d = long_position();
        d.style.position.compact = true;
        let labels = position_labels(&d, true);
        assert!(labels.iter().any(|t| t == "Long | 10 | 3.0R"), "{labels:?}");
        assert!(labels.iter().any(|t| t == "+300.00"), "{labels:?}");
        assert!(labels.iter().any(|t| t == "-100.00"), "{labels:?}");

        // A currency follows the amounts.
        let mut d = long_position();
        d.style.position.currency = "USD".to_owned();
        assert!(
            position_labels(&d, true)
                .iter()
                .any(|t| t.ends_with("+300.00 USD"))
        );
    }

    #[test]
    fn stats_can_wait_for_the_position_to_be_selected() {
        let mut d = long_position();
        d.style.position.always_stats = false;
        let idle = position_labels(&d, false);
        assert!(idle.iter().any(|t| t == "Long | R/R 3.00"), "{idle:?}");
        assert!(
            !idle
                .iter()
                .any(|t| t.contains("Qty") || t.ends_with("+300.00"))
        );
        let picked = position_labels(&d, true);
        assert!(picked.iter().any(|t| t.contains("Qty 10")), "{picked:?}");
    }

    #[test]
    fn a_position_takes_its_colors_and_lines_from_its_settings() {
        let mut d = long_position();
        d.style.position.target_color = 0x123456;
        d.style.position.stop_color = 0x654321;
        d.style.width = 3.0;
        d.style.fill_opacity = 0.5;
        let shapes = prims(&d, &Linear);
        let fills: Vec<(u32, f32)> = shapes
            .iter()
            .filter_map(|s| match s {
                Prim::Rect {
                    fill: Some(fill), ..
                } => Some(*fill),
                _ => None,
            })
            .collect();
        assert_eq!(fills, vec![(0x123456, 0.5), (0x654321, 0.5)]);
        assert!(shapes.iter().any(|s| matches!(
            s, Prim::Segment { color: 0x123456, width, .. } if *width == 3.0
        )));

        // No background, no tags.
        d.style.fill = false;
        d.style.labels = false;
        let bare = prims(&d, &Linear);
        assert!(
            !bare
                .iter()
                .any(|s| matches!(s, Prim::Rect { .. } | Prim::Label { .. }))
        );
        assert_eq!(bare.len(), 3, "the three lines are left");
    }

    #[test]
    fn a_level_can_have_its_own_line_and_its_caption_follows_the_style() {
        use crate::drawing::look::{LabelSide, LevelText};

        let mut d = drawing(Tool::FibRetracement, &[(100, 100.0), (400, 200.0)]);
        let mut levels = d.levels();
        levels[1].width = 4.0;
        levels[1].dash = Some(Dash::Dotted);
        let color = levels[1].color;
        d.levels = levels;
        let shapes = prims(&d, &Linear);
        assert!(shapes.iter().any(|s| matches!(
            s,
            Prim::Segment { color: c, width, dash: Dash::Dotted, .. } if *c == color && *width == 4.0
        )));
        // The others keep the line of the drawing.
        assert!(shapes.iter().any(|s| matches!(
            s, Prim::Segment { width, dash: Dash::Solid, .. } if *width == d.style.width
        )));

        let captions = |d: &Drawing| -> Vec<(String, P, Anchor)> {
            prims(d, &Linear)
                .into_iter()
                .filter_map(|s| match s {
                    Prim::Label {
                        text, at, anchor, ..
                    } => Some((text, at, anchor)),
                    _ => None,
                })
                .collect()
        };
        assert!(
            captions(&d)
                .iter()
                .any(|(t, _, _)| t.starts_with("0.236 (")),
            "the ratio and the price, as before"
        );
        d.style.level_text = LevelText::Percent;
        assert!(captions(&d).iter().any(|(t, _, _)| t == "23.6%"));
        d.style.level_text = LevelText::Price;
        assert!(
            captions(&d)
                .iter()
                .all(|(t, _, _)| !t.contains('(') && !t.contains('%'))
        );
        d.style.level_text = LevelText::Ratio;
        assert!(captions(&d).iter().any(|(t, _, _)| t == "0.236"));

        // At the left of the levels by default, at the right when asked.
        assert!(
            captions(&d)
                .iter()
                .all(|(_, at, a)| at.0 < 100.0 && *a == Anchor::Right)
        );
        d.style.label_side = LabelSide::Right;
        assert!(
            captions(&d)
                .iter()
                .all(|(_, at, a)| at.0 > 400.0 && *a == Anchor::Left)
        );
    }

    #[test]
    fn a_measure_follows_its_look() {
        let mut d = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
        d.style.color = 0x123456;
        d.style.measure.percent = false;
        d.style.measure.bars = false;
        let shapes = prims(&d, &Linear);
        let text = shapes.iter().find_map(|s| match s {
            Prim::Label { text, .. } => Some(text.clone()),
            _ => None,
        });
        assert_eq!(text.as_deref(), Some("+50.0   10m"));
        assert!(shapes.iter().any(|s| matches!(
            s,
            Prim::Rect {
                stroke: Some((0x123456, _)),
                ..
            }
        )));

        // A move down takes the other color; with nothing to say there is no tag.
        let mut down = drawing(Tool::Measure, &[(0, 200.0), (120, 150.0)]);
        down.style.measure.down_color = 0xabcdef;
        down.style.measure.price = false;
        down.style.measure.percent = false;
        down.style.measure.bars = false;
        down.style.measure.time = false;
        let shapes = prims(&down, &Linear);
        assert!(shapes.iter().any(|s| matches!(
            s,
            Prim::Rect {
                stroke: Some((0xabcdef, _)),
                ..
            }
        )));
        assert!(!shapes.iter().any(|s| matches!(s, Prim::Label { .. })));

        // The tag takes the text of the style.
        let mut styled = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
        styled.style.text_size = 20.0;
        styled.style.bold = true;
        styled.style.text_color = Some(0x00ff00);
        assert!(prims(&styled, &Linear).iter().any(|s| matches!(
            s,
            Prim::Label { size, bold: true, color: 0x00ff00, .. } if *size == 20.0
        )));
    }

    #[test]
    fn a_marker_grows_with_its_scale() {
        let height = |scale: f32| {
            let mut d = drawing(Tool::ArrowMarkUp, &[(100, 100.0)]);
            d.style.scale = scale;
            let Some(Prim::Polygon { points, .. }) = prims(&d, &Linear).into_iter().next() else {
                panic!("a polygon");
            };
            let ys = points.iter().map(|p| p.1);
            ys.clone().fold(f32::MIN, f32::max) - ys.fold(f32::MAX, f32::min)
        };
        assert!((height(2.0) - 2.0 * height(1.0)).abs() < 1e-3);
        assert!(height(0.5) < height(1.0));
    }

    #[test]
    fn durations_read_in_their_two_largest_units() {
        assert_eq!(duration_text(30_000), "30s");
        assert_eq!(duration_text(45 * 60_000), "45m");
        assert_eq!(duration_text(135 * 60_000), "2h 15m");
        assert_eq!(duration_text(-(3 * 86_400_000 + 4 * 3_600_000)), "3d 4h");
    }

    #[test]
    fn a_pointer_on_a_line_hits_the_body_and_near_an_end_hits_the_grip() {
        let d = drawing(Tool::TrendLine, &[(100, 100.0), (500, 300.0)]);
        // Screen: (100, 400) to (500, 200). The middle is (300, 300).
        assert_eq!(hit(&d, &Linear, (300.0, 302.0), true), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (300.0, 330.0), true), None);
        assert_eq!(
            hit(&d, &Linear, (104.0, 396.0), true),
            Some(Part::Handle(0))
        );
        assert_eq!(
            hit(&d, &Linear, (503.0, 199.0), true),
            Some(Part::Handle(1))
        );
        // Without grips the end is just the body.
        assert_eq!(hit(&d, &Linear, (104.0, 396.0), false), Some(Part::Body));
    }

    #[test]
    fn a_locked_drawing_has_no_grips_but_can_still_be_picked() {
        let mut d = drawing(Tool::TrendLine, &[(100, 100.0), (500, 300.0)]);
        d.locked = true;
        assert_eq!(hit(&d, &Linear, (100.0, 400.0), true), Some(Part::Body));
    }

    #[test]
    fn a_filled_shape_is_picked_from_inside_and_an_empty_one_only_by_its_edge() {
        let mut d = drawing(Tool::Rectangle, &[(100, 100.0), (400, 300.0)]);
        // Screen rect: x 100..400, y 200..400.
        assert_eq!(hit(&d, &Linear, (250.0, 300.0), false), Some(Part::Body));
        d.style.fill = false;
        assert_eq!(hit(&d, &Linear, (250.0, 300.0), false), None);
        assert_eq!(hit(&d, &Linear, (250.0, 201.0), false), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (900.0, 300.0), false), None);
    }

    #[test]
    fn an_ellipse_is_picked_inside_and_on_its_rim() {
        let d = drawing(Tool::Ellipse, &[(100, 100.0), (500, 300.0)]);
        // Center (300, 300), radii 200 and 100.
        assert_eq!(hit(&d, &Linear, (300.0, 300.0), false), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (150.0, 300.0), false), Some(Part::Body));
        assert_eq!(
            hit(&d, &Linear, (120.0, 210.0), false),
            None,
            "the corner of the box is outside"
        );
    }

    #[test]
    fn a_brush_is_picked_along_its_stroke_and_has_no_grips() {
        let d = drawing(Tool::Brush, &[(0, 100.0), (100, 200.0), (200, 100.0)]);
        assert!(handles(&d, &Linear).is_empty());
        assert_eq!(hit(&d, &Linear, (50.0, 350.0), true), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (50.0, 100.0), true), None);
    }

    #[test]
    fn a_drawing_with_no_way_to_place_it_draws_nothing() {
        struct Nowhere;
        impl Projection for Nowhere {
            fn plot(&self) -> Rect {
                Rect::new(10.0, 10.0)
            }
            fn to_screen(&self, _: Point) -> Option<P> {
                None
            }
            fn point_at(&self, _: f32, _: f32, _: bool) -> Option<Point> {
                None
            }
            fn index_of(&self, _: i64) -> Option<f64> {
                None
            }
            fn shift_bars(&self, _: i64, _: f64) -> Option<i64> {
                None
            }
            fn format_price(&self, _: f64) -> String {
                String::new()
            }
            fn price_span(&self) -> f64 {
                1.0
            }
        }
        let d = drawing(Tool::TrendLine, &[(0, 1.0), (1, 2.0)]);
        assert!(prims(&d, &Nowhere).is_empty());
        assert_eq!(hit(&d, &Nowhere, (1.0, 1.0), true), None);
    }
}
