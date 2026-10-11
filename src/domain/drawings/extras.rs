//! The shapes of the tools that are not lines of the plain kind or the Fibonacci, Gann and pattern
//! families of [`super::figures`]: information lines and angles, regression, ranges, cycles,
//! circles and curves, forecasts, ghost bars, volume, notes, markers and tables.
//!
//! Like the rest of the drawing code this is arithmetic on screen positions. The tools that read
//! the chart's data (regression, bars, VWAP, volume) get it through [`Projection::bars_between`].

use std::f32::consts::{PI, TAU};

use super::figures;
use super::geometry::{Anchor, P, Prim, Projection, duration_text};
use super::look::MeasureLook;
use super::model::{Drawing, Point, Style, Tool};

/// The colors of a bar that closed above and below its open, as the position tools have them.
const UP: u32 = 0x00d492;
const DOWN: u32 = 0xff6467;
/// The color of the point of control of a volume profile.
const POC: u32 = 0xffb900;
/// Most bars a bars pattern or a ghost feed copies.
const MAX_GHOST_BARS: usize = 400;
/// The height of a row of a volume profile, in pixels, and the fewest and most rows.
const ROW_PX: f32 = 6.0;
const MIN_ROWS: usize = 8;
const MAX_ROWS: usize = 120;
/// Most cycle lines drawn at once.
const MAX_CYCLES: i64 = 600;

/// The glyphs of an icon drawing: the name it is saved under, and what the settings call it.
pub const ICONS: [(&str, &str); 10] = [
    ("star", "Star"),
    ("circle", "Circle"),
    ("square", "Square"),
    ("diamond", "Diamond"),
    ("triangle_up", "Triangle up"),
    ("triangle_down", "Triangle down"),
    ("cross", "Cross"),
    ("check", "Check"),
    ("heart", "Heart"),
    ("bolt", "Bolt"),
];

/// The shapes of a drawing whose tool is one of the ones in this file. `pts` are its points on
/// the screen.
pub fn prims(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    match drawing.tool {
        Tool::InfoLine => info_line(drawing, pts, proj, out),
        Tool::TrendAngle => trend_angle(drawing, pts, proj, out),
        Tool::RegressionTrend => regression(drawing, pts, proj, out),
        Tool::FlatTopBottom => flat_top_bottom(drawing, pts, proj, out),
        Tool::DisjointChannel => disjoint_channel(drawing, pts, proj, out),
        Tool::Pitchfan => pitchfan(drawing, pts, proj, out),
        Tool::FibTimeExtension => fib_time_extension(drawing, pts, proj, out),
        Tool::FibCircles => fib_circles(drawing, pts, out),
        Tool::FibSpiral => fib_spiral(drawing, pts, out),
        Tool::FibArcs => fib_arcs(drawing, pts, out),
        Tool::FibWedge => fib_wedge(drawing, pts, out),
        Tool::GannSquareFixed => {
            let corner = fixed_square_corner(pts[0], pts[1]);
            figures::gann_square(drawing, &[pts[0], corner], out);
        }
        Tool::CyclicLines => cyclic_lines(drawing, pts, proj, out),
        Tool::TimeCycles => time_cycles(drawing, pts, proj, out),
        Tool::SineLine => sine_line(drawing, pts, proj, out),
        Tool::PriceRange => price_range(drawing, pts, proj, out),
        Tool::DateRange => date_range(drawing, pts, proj, out),
        Tool::DatePriceRange => date_price_range(drawing, pts, proj, out),
        Tool::Forecast => forecast(drawing, pts, proj, out),
        Tool::BarsPattern => bars_pattern(drawing, proj, out, false),
        Tool::GhostFeed => bars_pattern(drawing, proj, out, true),
        Tool::AnchoredVwap => anchored_vwap(drawing, pts, proj, out),
        Tool::FixedRangeVolumeProfile => volume_profile(drawing, pts, proj, out, true),
        Tool::AnchoredVolumeProfile => volume_profile(drawing, pts, proj, out, false),
        Tool::RotatedRectangle => rotated_rectangle(drawing, pts, out),
        Tool::Circle => circle(drawing, pts, out),
        Tool::Arc => arc(drawing, pts, out),
        Tool::Curve => curve(drawing, pts, out),
        Tool::DoubleCurve => double_curve(drawing, pts, out),
        Tool::ArrowMarker => arrow_marker(drawing, pts, out),
        Tool::ArrowMarkUp => arrow_mark(drawing, pts[0], true, out),
        Tool::ArrowMarkDown => arrow_mark(drawing, pts[0], false, out),
        Tool::Note => note(drawing, pts, out),
        Tool::PriceNote => price_note(drawing, pts, proj, out),
        Tool::Callout => callout(drawing, pts, out),
        Tool::Comment => comment(drawing, pts, out),
        Tool::Pin => pin(drawing, pts, out),
        Tool::Signpost => signpost(drawing, pts, out),
        Tool::FlagMark => flag_mark(drawing, pts, out),
        Tool::Table => table(drawing, pts, out),
        Tool::Icon => icon(drawing, pts, out),
        _ => {}
    }
}

fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn scale(a: P, k: f32) -> P {
    (a.0 * k, a.1 * k)
}

fn mid(a: P, b: P) -> P {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

fn length(a: P) -> f32 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}

fn dist(a: P, b: P) -> f32 {
    length(sub(b, a))
}

/// The angle of the line from `a` to `b` to the horizontal, in degrees, up being positive and the
/// direction the line runs in not mattering: between -90 and 90.
fn slope_degrees(a: P, b: P) -> f32 {
    (-(b.1 - a.1)).atan2((b.0 - a.0).abs()).to_degrees()
}

/// `angle` brought into the range from -PI to PI.
fn wrap(angle: f32) -> f32 {
    let mut a = angle % TAU;
    if a > PI {
        a -= TAU;
    } else if a < -PI {
        a += TAU;
    }
    a
}

/// The two ends of a segment ordered left to right.
fn left_to_right(a: P, b: P) -> (P, P) {
    if a.0 <= b.0 { (a, b) } else { (b, a) }
}

/// A tag with a stat on it, in the color of the drawing.
fn stat(style: &Style, at: P, text: String, color: u32, anchor: Anchor) -> Prim {
    Prim::Label {
        at,
        text,
        color: style.text_color.unwrap_or(0xffffff),
        background: Some((color, 0.9)),
        anchor,
        size: style.text_size,
        bold: style.bold,
        face: style.face(),
    }
}

/// Tags one under the other, the first at `at`.
fn stat_rows(
    style: &Style,
    at: P,
    rows: Vec<String>,
    color: u32,
    anchor: Anchor,
    out: &mut Vec<Prim>,
) {
    for (i, text) in rows.into_iter().filter(|r| !r.is_empty()).enumerate() {
        out.push(stat(
            style,
            (at.0, at.1 + i as f32 * style.text_size * 1.8),
            text,
            color,
            anchor,
        ));
    }
}

/// The change from `a` to `b`: `+0.0012 (+0.12%)`, or the part of it the look asks for. Empty when
/// it asks for none.
pub(super) fn move_text(proj: &dyn Projection, a: Point, b: Point, look: &MeasureLook) -> String {
    let change = b.p - a.p;
    let percent = if a.p != 0.0 {
        change / a.p * 100.0
    } else {
        0.0
    };
    let sign = if change >= 0.0 { "+" } else { "" };
    let price = format!("{sign}{}", proj.format_price(change));
    let percent = format!("{sign}{percent:.2}%");
    match (look.price, look.percent) {
        (true, true) => format!("{price} ({percent})"),
        (true, false) => price,
        (false, true) => percent,
        (false, false) => String::new(),
    }
}

/// The time from `a` to `b`: `12 bars, 1h 0m`, or the part of it the look asks for. Empty when it
/// asks for none.
pub(super) fn span_text(proj: &dyn Projection, a: Point, b: Point, look: &MeasureLook) -> String {
    let bars = match (proj.index_of(a.t), proj.index_of(b.t)) {
        (Some(i), Some(j)) => (j - i).abs().round() as i64,
        _ => 0,
    };
    let time = duration_text(b.t - a.t);
    match (look.bars, look.time) {
        (true, true) => format!("{bars} bars, {time}"),
        (true, false) => format!("{bars} bars"),
        (false, true) => time,
        (false, false) => String::new(),
    }
}

/// A line of points `at(t)` for `t` from 0 to 1, in `steps` steps.
fn sample(steps: usize, at: impl Fn(f32) -> P) -> Vec<P> {
    (0..=steps).map(|i| at(i as f32 / steps as f32)).collect()
}

fn polyline(points: Vec<P>, color: u32, width: f32, alpha: f32) -> Prim {
    Prim::Polyline {
        points,
        color,
        width,
        alpha,
    }
}

/// The corner opposite `a` of the square a fixed Gann square makes towards `b`: as far as the
/// pointer goes on its longer side, so the square is square on the screen.
pub fn fixed_square_corner(a: P, b: P) -> P {
    let side = (b.0 - a.0).abs().max((b.1 - a.1).abs());
    let sx = if b.0 >= a.0 { 1.0 } else { -1.0 };
    let sy = if b.1 >= a.1 { 1.0 } else { -1.0 };
    (a.0 + sx * side, a.1 + sy * side)
}

mod bars;
mod curves;
mod fib;
mod lines;
mod notes;
mod ranges;

pub use notes::{icon_key, table_cells};

use bars::{anchored_vwap, bars_pattern, volume_profile};
use curves::{
    arc, circle, curve, cyclic_lines, double_curve, rotated_rectangle, sine_line, time_cycles,
};
use fib::{fib_arcs, fib_circles, fib_spiral, fib_time_extension, fib_wedge};
use lines::{disjoint_channel, flat_top_bottom, info_line, pitchfan, regression, trend_angle};
use notes::{
    arrow_mark, arrow_marker, callout, comment, flag_mark, icon, note, pin, price_note, signpost,
    table,
};
use ranges::{date_price_range, date_range, forecast, price_range};

#[cfg(test)]
#[path = "../../../tests/unit/domain/drawings/extras.rs"]
mod tests;
