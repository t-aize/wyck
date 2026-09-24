//! The shapes of the tools that are not lines of the plain kind or the Fibonacci, Gann and pattern
//! families of [`super::figures`]: information lines and angles, regression, ranges, cycles,
//! circles and curves, forecasts, ghost bars, volume, notes, markers and tables.
//!
//! Like the rest of the drawing code this is arithmetic on screen positions. The tools that read
//! the chart's data (regression, bars, VWAP, volume) get it through [`Projection::bars_between`].

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::figures;
use super::geometry::{
    Anchor, LABEL_SIZE, P, Prim, Projection, arrow_head, board_size, duration_text, extended,
    fill_of, label, level_text, seg, stretch, visible_levels,
};
use super::model::{Dash, Drawing, Point, Tool};

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
/// The share of the volume the value area of a profile holds.
const VALUE_AREA: f64 = 0.7;
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

// ---- small arithmetic ----

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
fn stat(at: P, text: String, color: u32, anchor: Anchor) -> Prim {
    Prim::Label {
        at,
        text,
        color: 0xffffff,
        background: Some((color, 0.9)),
        anchor,
        size: LABEL_SIZE,
        bold: false,
    }
}

/// Tags one under the other, the first at `at`.
fn stat_rows(at: P, rows: Vec<String>, color: u32, anchor: Anchor, out: &mut Vec<Prim>) {
    for (i, text) in rows.into_iter().enumerate() {
        out.push(stat(
            (at.0, at.1 + i as f32 * LABEL_SIZE * 1.8),
            text,
            color,
            anchor,
        ));
    }
}

/// The change from `a` to `b`: `+0.0012 (+0.12%)`.
fn move_text(proj: &dyn Projection, a: Point, b: Point) -> String {
    let change = b.p - a.p;
    let percent = if a.p != 0.0 {
        change / a.p * 100.0
    } else {
        0.0
    };
    let sign = if change >= 0.0 { "+" } else { "" };
    format!("{sign}{} ({sign}{percent:.2}%)", proj.format_price(change))
}

/// The time from `a` to `b`: `12 bars, 1h 0m`.
fn span_text(proj: &dyn Projection, a: Point, b: Point) -> String {
    let bars = match (proj.index_of(a.t), proj.index_of(b.t)) {
        (Some(i), Some(j)) => (j - i).abs().round() as i64,
        _ => 0,
    };
    format!("{bars} bars, {}", duration_text(b.t - a.t))
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

// ---- lines ----

fn info_line(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    if let Some((from, to)) = extended(a, b, style.extend_left, style.extend_right, proj.plot()) {
        out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    }
    if style.labels {
        let (p, q) = (drawing.points[0], drawing.points[1]);
        let rows = vec![
            move_text(proj, p, q),
            span_text(proj, p, q),
            format!("Angle {:.1}\u{b0}", slope_degrees(a, b)),
        ];
        let at = (b.0 + 10.0, b.1 - LABEL_SIZE * 1.8);
        stat_rows(at, rows, style.color, Anchor::Left, out);
    }
}

fn trend_angle(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    if let Some((from, to)) = extended(a, b, style.extend_left, style.extend_right, proj.plot()) {
        out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    }
    let run = dist(a, b);
    if run < 2.0 {
        return;
    }
    let side = if b.0 >= a.0 { 1.0 } else { -1.0 };
    out.push(seg(
        a,
        (a.0 + side * run, a.1),
        style.color,
        0.6,
        1.0,
        Dash::Dashed,
    ));
    if !style.labels {
        return;
    }
    let phi = slope_degrees(a, b).to_radians();
    let radius = (run * 0.35).clamp(12.0, 60.0);
    let arc = sample(24, |t| {
        let angle = phi * t;
        (
            a.0 + side * radius * angle.cos(),
            a.1 - radius * angle.sin(),
        )
    });
    out.push(polyline(arc, style.color, 1.0, 0.9));
    let half = phi / 2.0;
    let at = (
        a.0 + side * (radius + 16.0) * half.cos(),
        a.1 - (radius + 16.0) * half.sin(),
    );
    out.push(stat(
        at,
        format!("{:.1}\u{b0}", slope_degrees(a, b)),
        style.color,
        Anchor::Center,
    ));
}

// ---- channels ----

fn regression(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (t0, t1) = (
        drawing.points[0].t.min(drawing.points[1].t),
        drawing.points[0].t.max(drawing.points[1].t),
    );
    let bars = proj.bars_between(t0, t1);
    let n = bars.len();
    if n < 2 {
        out.push(seg(pts[0], pts[1], style.color, 0.6, 1.0, Dash::Dashed));
        return;
    }
    let count = n as f64;
    let (mut sum_x, mut sum_xx, mut sum_y, mut sum_xy) = (0.0, 0.0, 0.0, 0.0);
    for (k, bar) in bars.iter().enumerate() {
        let x = k as f64;
        sum_x += x;
        sum_xx += x * x;
        sum_y += bar.close;
        sum_xy += x * bar.close;
    }
    let denominator = count * sum_xx - sum_x * sum_x;
    let slope = if denominator.abs() < 1e-12 {
        0.0
    } else {
        (count * sum_xy - sum_x * sum_y) / denominator
    };
    let intercept = (sum_y - slope * sum_x) / count;
    let variance = bars
        .iter()
        .enumerate()
        .map(|(k, bar)| (bar.close - (intercept + slope * k as f64)).powi(2))
        .sum::<f64>()
        / count;
    let deviation = variance.sqrt();

    let rect = proj.plot();
    let (first, last) = (&bars[0], &bars[n - 1]);
    let line_at = |offset: f64| -> Option<(P, P)> {
        let from = (first.x, proj.y_of(intercept + offset));
        let to = (
            last.x,
            proj.y_of(intercept + slope * (count - 1.0) + offset),
        );
        extended(from, to, style.extend_left, style.extend_right, rect)
    };
    let levels = visible_levels(drawing);
    let widest = levels.iter().map(|l| l.value).fold(0.0, f64::max);
    if widest > 0.0
        && let (Some((a0, a1)), Some((b0, b1)), Some((_, opacity))) = (
            line_at(widest * deviation),
            line_at(-widest * deviation),
            fill_of(style),
        )
    {
        out.push(Prim::Polygon {
            points: vec![a0, a1, b1, b0],
            fill: (style.fill_color(), opacity),
        });
    }
    for level in &levels {
        for sign in [1.0, -1.0] {
            if let Some((from, to)) = line_at(sign * level.value * deviation) {
                out.push(seg(from, to, level.color, 0.9, style.width, style.dash));
                if style.labels && sign > 0.0 {
                    out.push(label(
                        (to.0 + 4.0, to.1),
                        level_text(level.value),
                        level.color,
                        Anchor::Left,
                        style,
                    ));
                }
            }
        }
    }
    if let Some((from, to)) = line_at(0.0) {
        out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    }
}

fn flat_top_bottom(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let (from, to) = stretch(a, b, style, proj.plot());
    let flat = ((from.0, c.1), (to.0, c.1));
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: vec![from, to, flat.1, flat.0],
            fill,
        });
    }
    out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    out.push(seg(
        flat.0,
        flat.1,
        style.color,
        1.0,
        style.width,
        style.dash,
    ));
}

fn disjoint_channel(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (a, b) = left_to_right(pts[0], pts[1]);
    let (c, d) = left_to_right(pts[2], pts[3]);
    let (a, b) = stretch(a, b, style, rect);
    let (c, d) = stretch(c, d, style, rect);
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: vec![a, b, d, c],
            fill,
        });
    }
    out.push(seg(a, b, style.color, 1.0, style.width, style.dash));
    out.push(seg(c, d, style.color, 1.0, style.width, style.dash));
    if style.middle {
        out.push(seg(
            mid(a, c),
            mid(b, d),
            style.color,
            0.6,
            1.0,
            Dash::Dashed,
        ));
    }
}

fn pitchfan(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let through: Vec<(P, u32, String)> = visible_levels(drawing)
        .into_iter()
        .map(|l| {
            let p = add(b, scale(sub(c, b), l.value as f32));
            (p, l.color, level_text(l.value))
        })
        .collect();
    figures::fan(drawing, a, &through, proj.plot(), out);
    out.push(seg(b, c, drawing.style.color, 0.6, 1.0, Dash::Dashed));
}

// ---- Fibonacci ----

fn fib_time_extension(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (pa, pb, pc) = (drawing.points[0], drawing.points[1], drawing.points[2]);
    let bars = match (proj.index_of(pa.t), proj.index_of(pb.t)) {
        (Some(i), Some(j)) => j - i,
        _ => return,
    };
    let mut previous: Option<f32> = None;
    for level in visible_levels(drawing) {
        let x = if bars.abs() < 1e-9 {
            pts[2].0
        } else {
            let Some(t) = proj.shift_bars(pc.t, bars * level.value) else {
                continue;
            };
            let Some(at) = proj.to_screen(Point { t, p: pc.p }) else {
                continue;
            };
            at.0
        };
        if let (Some(before), Some((_, opacity))) = (previous, fill_of(style)) {
            out.push(Prim::Rect {
                a: (before, rect.t),
                b: (x, rect.b),
                fill: Some((level.color, opacity)),
                stroke: None,
            });
        }
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            level.color,
            0.9,
            style.width,
            style.dash,
        ));
        if style.labels {
            out.push(label(
                (x + 4.0, rect.b - 12.0),
                level_text(level.value),
                level.color,
                Anchor::Left,
                style,
            ));
        }
        previous = Some(x);
    }
    out.push(seg(pts[0], pts[1], style.color, 0.6, 1.0, Dash::Dashed));
    out.push(seg(pts[1], pts[2], style.color, 0.6, 1.0, Dash::Dashed));
}

fn fib_circles(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    let center = mid(a, b);
    let base = dist(a, b) / 2.0;
    if base < 1.0 {
        return;
    }
    for level in visible_levels(drawing) {
        let radius = base * level.value as f32;
        if radius < 1.0 {
            continue;
        }
        out.push(Prim::Ellipse {
            center,
            rx: radius,
            ry: radius,
            fill: None,
            stroke: Some((level.color, style.width)),
        });
        if style.labels {
            out.push(label(
                (center.0, center.1 - radius - 8.0),
                level_text(level.value),
                level.color,
                Anchor::Center,
                style,
            ));
        }
    }
    out.push(seg(a, b, style.color, 0.6, 1.0, Dash::Dashed));
}

fn fib_arcs(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    let base = dist(a, b);
    if base < 1.0 {
        return;
    }
    // Half circles about the second point, on the side of the first.
    let toward = sub(a, b).1.atan2(sub(a, b).0);
    for level in visible_levels(drawing) {
        let radius = base * level.value as f32;
        if radius < 1.0 {
            continue;
        }
        let arc = sample(32, |t| {
            let angle = toward - FRAC_PI_2 + PI * t;
            (b.0 + radius * angle.cos(), b.1 + radius * angle.sin())
        });
        out.push(polyline(arc, level.color, style.width, 0.9));
        if style.labels {
            out.push(label(
                (
                    b.0 + radius * toward.cos() + 4.0,
                    b.1 + radius * toward.sin(),
                ),
                level_text(level.value),
                level.color,
                Anchor::Left,
                style,
            ));
        }
    }
    out.push(seg(a, b, style.color, 0.6, 1.0, Dash::Dashed));
}

fn fib_spiral(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (center, through) = (pts[0], pts[1]);
    let base = dist(center, through);
    if base < 2.0 {
        return;
    }
    let start = sub(through, center).1.atan2(sub(through, center).0);
    let turn = if drawing.reverse { -1.0 } else { 1.0 };
    // A golden spiral grows by the golden ratio every quarter turn, and ends at the second point.
    let golden: f32 = 1.618_034;
    let spiral = sample(240, |t| {
        let theta = -4.0 * PI * (1.0 - t);
        let radius = base * golden.powf(theta / FRAC_PI_2);
        let angle = start + turn * theta;
        (
            center.0 + radius * angle.cos(),
            center.1 + radius * angle.sin(),
        )
    });
    out.push(polyline(spiral, style.color, style.width, 1.0));
    out.push(seg(center, through, style.color, 0.6, 1.0, Dash::Dashed));
}

fn fib_wedge(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (center, b, c) = (pts[0], pts[1], pts[2]);
    let base = dist(center, b);
    if base < 2.0 {
        return;
    }
    let (from, toward) = (
        sub(b, center).1.atan2(sub(b, center).0),
        sub(c, center).1.atan2(sub(c, center).0),
    );
    let sweep = wrap(toward - from);
    let levels = visible_levels(drawing);
    let widest = levels.iter().map(|l| l.value).fold(0.0, f64::max) as f32;
    if widest <= 0.0 {
        return;
    }
    let arc_at = |radius: f32| -> Vec<P> {
        sample(32, |t| {
            let angle = from + sweep * t;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
    };
    if let Some((_, opacity)) = fill_of(style) {
        let mut wedge = arc_at(base * widest);
        wedge.push(center);
        out.push(Prim::Polygon {
            points: wedge,
            fill: (style.fill_color(), opacity),
        });
    }
    for level in &levels {
        let radius = base * level.value as f32;
        out.push(polyline(arc_at(radius), level.color, style.width, 0.9));
        if style.labels {
            out.push(label(
                (
                    center.0 + radius * from.cos() + 4.0,
                    center.1 + radius * from.sin(),
                ),
                level_text(level.value),
                level.color,
                Anchor::Left,
                style,
            ));
        }
    }
    for angle in [from, from + sweep] {
        let end = (
            center.0 + base * widest * angle.cos(),
            center.1 + base * widest * angle.sin(),
        );
        out.push(seg(center, end, style.color, 1.0, style.width, style.dash));
    }
}

// ---- cycles ----

/// The x positions of the cycle lines a pair of points makes: one every distance between them,
/// counted in bars, in both directions across the plot, left to right.
fn cycle_xs(drawing: &Drawing, pts: &[P], proj: &dyn Projection) -> Vec<f32> {
    let rect = proj.plot();
    let (pa, pb) = (drawing.points[0], drawing.points[1]);
    let bars = match (proj.index_of(pa.t), proj.index_of(pb.t)) {
        (Some(i), Some(j)) => j - i,
        _ => return Vec::new(),
    };
    let step = pts[1].0 - pts[0].0;
    if bars.abs() < 1e-9 || step.abs() < 3.0 {
        return Vec::new();
    }
    let (k_left, k_right) = ((rect.l - pts[0].0) / step, (rect.r - pts[0].0) / step);
    let first = (k_left.min(k_right).floor() as i64).max(-MAX_CYCLES);
    let last = (k_left.max(k_right).ceil() as i64).min(MAX_CYCLES);
    let mut xs: Vec<f32> = (first..=last)
        .filter_map(|k| {
            if k == 0 {
                return Some(pts[0].0);
            }
            let t = proj.shift_bars(pa.t, bars * k as f64)?;
            proj.to_screen(Point { t, p: pa.p }).map(|p| p.0)
        })
        .collect();
    xs.sort_by(f32::total_cmp);
    xs
}

fn cyclic_lines(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    for x in cycle_xs(drawing, pts, proj) {
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            style.color,
            0.9,
            style.width,
            style.dash,
        ));
    }
}

fn time_cycles(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let base = pts[0].1;
    for pair in cycle_xs(drawing, pts, proj).windows(2) {
        let (left, width) = (pair[0], pair[1] - pair[0]);
        if width < 1.0 {
            continue;
        }
        let height = width / 2.0;
        let arch = sample(24, |t| (left + width * t, base - height * (PI * t).sin()));
        if let Some(fill) = fill_of(style) {
            out.push(Prim::Polygon {
                points: arch.clone(),
                fill,
            });
        }
        out.push(polyline(arch, style.color, style.width, 1.0));
    }
}

fn sine_line(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (a, b) = (pts[0], pts[1]);
    let half = b.0 - a.0;
    if half.abs() < 2.0 {
        out.push(seg(a, b, style.color, 0.6, 1.0, Dash::Dashed));
        return;
    }
    let (center, amplitude) = ((a.1 + b.1) / 2.0, (a.1 - b.1) / 2.0);
    let (left, right) = (
        if style.extend_left {
            rect.l
        } else {
            a.0.min(b.0)
        },
        if style.extend_right {
            rect.r
        } else {
            a.0.max(b.0)
        },
    );
    if right <= left {
        return;
    }
    let steps = (((right - left) / 3.0).ceil() as usize).clamp(2, 1_500);
    let wave = sample(steps, |t| {
        let x = left + (right - left) * t;
        (x, center + amplitude * (PI * (x - a.0) / half).cos())
    });
    out.push(polyline(wave, style.color, style.width, 1.0));
}

// ---- ranges and forecasts ----

/// A line from `a` to `b` with an arrow head at `b`.
fn arrow(a: P, b: P, color: u32, width: f32, out: &mut Vec<Prim>) {
    out.push(seg(a, b, color, 1.0, width, Dash::Solid));
    out.extend(arrow_head(a, b, color, width));
}

fn price_range(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    let (left, right) = (a.0.min(b.0), a.0.max(b.0).max(a.0.min(b.0) + 1.0));
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Rect {
            a: (left, a.1),
            b: (right, b.1),
            fill: Some(fill),
            stroke: None,
        });
    }
    for y in [a.1, b.1] {
        out.push(seg(
            (left, y),
            (right, y),
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
    let x = (left + right) / 2.0;
    arrow((x, a.1), (x, b.1), style.color, 1.0, out);
    if style.labels {
        let text = move_text(proj, drawing.points[0], drawing.points[1]);
        out.push(stat(
            (x, a.1.min(b.1) - 14.0),
            text,
            style.color,
            Anchor::Center,
        ));
    }
}

fn date_range(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (a, b) = (pts[0], pts[1]);
    let (left, right) = (a.0.min(b.0), a.0.max(b.0).max(a.0.min(b.0) + 1.0));
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Rect {
            a: (left, rect.t),
            b: (right, rect.b),
            fill: Some(fill),
            stroke: None,
        });
    }
    for x in [left, right] {
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
    arrow((a.0, a.1), (b.0, a.1), style.color, 1.0, out);
    if style.labels {
        out.push(stat(
            ((a.0 + b.0) / 2.0, a.1 - 14.0),
            span_text(proj, drawing.points[0], drawing.points[1]),
            style.color,
            Anchor::Center,
        ));
    }
}

fn date_price_range(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    out.push(Prim::Rect {
        a,
        b,
        fill: fill_of(style),
        stroke: Some((style.color, style.width)),
    });
    let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    arrow((a.0, my), (b.0, my), style.color, 1.0, out);
    arrow((mx, a.1), (mx, b.1), style.color, 1.0, out);
    if style.labels {
        let (p, q) = (drawing.points[0], drawing.points[1]);
        out.push(stat(
            (mx, a.1.min(b.1) - 14.0),
            move_text(proj, p, q),
            style.color,
            Anchor::Center,
        ));
        out.push(stat(
            (mx, a.1.max(b.1) + 14.0),
            span_text(proj, p, q),
            style.color,
            Anchor::Center,
        ));
    }
}

fn forecast(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    out.push(seg(a, b, style.color, 1.0, style.width, style.dash));
    out.extend(arrow_head(a, b, style.color, style.width));
    out.push(Prim::Ellipse {
        center: a,
        rx: 3.5,
        ry: 3.5,
        fill: Some((style.color, 1.0)),
        stroke: None,
    });
    if style.labels {
        let (p, q) = (drawing.points[0], drawing.points[1]);
        let side = if b.0 >= a.0 { 12.0 } else { -12.0 };
        let anchor = if b.0 >= a.0 {
            Anchor::Left
        } else {
            Anchor::Right
        };
        let rows = vec![
            format!("{} ({})", proj.format_price(q.p), move_text(proj, p, q)),
            span_text(proj, p, q),
        ];
        stat_rows(
            (b.0 + side, b.1 - LABEL_SIZE * 0.9),
            rows,
            style.color,
            anchor,
            out,
        );
    }
}

// ---- bars ----

/// The candle of a bar shifted by `offset` in price, at `x`.
fn candle(
    proj: &dyn Projection,
    x: f32,
    prices: (f64, f64, f64, f64),
    offset: f64,
    width: f32,
    alpha: f32,
    out: &mut Vec<Prim>,
) {
    let (open, high, low, close) = prices;
    let color = if close >= open { UP } else { DOWN };
    out.push(seg(
        (x, proj.y_of(high + offset)),
        (x, proj.y_of(low + offset)),
        color,
        alpha,
        1.0,
        Dash::Solid,
    ));
    out.push(Prim::Rect {
        a: (x - width / 2.0, proj.y_of(open + offset)),
        b: (x + width / 2.0, proj.y_of(close + offset)),
        fill: Some((color, alpha)),
        stroke: None,
    });
}

/// The bars between the first two points of a bars pattern or a ghost feed.
fn source_bars(drawing: &Drawing, proj: &dyn Projection) -> Vec<super::geometry::BarView> {
    let (a, b) = (drawing.points[0].t, drawing.points[1].t);
    let mut bars = proj.bars_between(a.min(b), a.max(b));
    bars.truncate(MAX_GHOST_BARS);
    bars
}

/// A bars pattern (the bars of the range, copied) or a ghost feed (bars made from the moves of
/// the range in another order), starting at the third point.
fn bars_pattern(drawing: &Drawing, proj: &dyn Projection, out: &mut Vec<Prim>, ghost: bool) {
    let style = &drawing.style;
    let bars = source_bars(drawing, proj);
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return;
    };
    let start = drawing.points[2];
    let Some(first_index) = proj.index_of(first.time) else {
        return;
    };
    let spacing = if bars.len() >= 2 {
        (bars[1].x - bars[0].x).abs()
    } else {
        6.0
    };
    let width = (spacing * 0.7).clamp(1.0, 20.0);

    // The range the bars come from.
    let high = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let low = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    out.push(Prim::Rect {
        a: (first.x - spacing / 2.0, proj.y_of(high)),
        b: (last.x + spacing / 2.0, proj.y_of(low)),
        fill: Some((style.color, 0.08)),
        stroke: None,
    });

    let place = |offset_bars: f64| -> Option<f32> {
        let t = proj.shift_bars(start.t, offset_bars)?;
        proj.to_screen(Point { t, p: start.p }).map(|p| p.0)
    };
    if ghost {
        let n = bars.len();
        let mut seed = drawing
            .id
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(n as u64);
        let mut price = start.p;
        for i in 0..n {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let source = &bars[(seed >> 33) as usize % n];
            let (open, close) = (price, price + (source.close - source.open));
            let (high, low) = (
                price + (source.high - source.open),
                price + (source.low - source.open),
            );
            if let Some(x) = place(i as f64) {
                candle(proj, x, (open, high, low, close), 0.0, width, 0.55, out);
            }
            price = close;
        }
    } else {
        let offset = start.p - first.close;
        for bar in &bars {
            let Some(index) = proj.index_of(bar.time) else {
                continue;
            };
            if let Some(x) = place(index - first_index) {
                candle(
                    proj,
                    x,
                    (bar.open, bar.high, bar.low, bar.close),
                    offset,
                    width,
                    0.7,
                    out,
                );
            }
        }
    }
}

fn anchored_vwap(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let bars = proj.bars_between(drawing.points[0].t, i64::MAX);
    if bars.is_empty() {
        return;
    }
    let levels = visible_levels(drawing);
    let (mut volume, mut weighted, mut weighted_squares) = (0.0, 0.0, 0.0);
    let mut line: Vec<P> = Vec::new();
    let mut bands: Vec<(Vec<P>, Vec<P>)> =
        levels.iter().map(|_| (Vec::new(), Vec::new())).collect();
    for bar in &bars {
        let typical = (bar.high + bar.low + bar.close) / 3.0;
        let weight = if bar.volume > 0.0 { bar.volume } else { 1.0 };
        volume += weight;
        weighted += typical * weight;
        weighted_squares += typical * typical * weight;
        let vwap = weighted / volume;
        let deviation = (weighted_squares / volume - vwap * vwap).max(0.0).sqrt();
        // Only what is near the plot is kept: the rest of a long history is out of sight.
        if bar.x < rect.l - 20.0 || bar.x > rect.r + 20.0 {
            continue;
        }
        line.push((bar.x, proj.y_of(vwap)));
        for (band, level) in bands.iter_mut().zip(&levels) {
            band.0
                .push((bar.x, proj.y_of(vwap + level.value * deviation)));
            band.1
                .push((bar.x, proj.y_of(vwap - level.value * deviation)));
        }
    }
    for ((upper, lower), level) in bands.into_iter().zip(&levels) {
        out.push(polyline(upper, level.color, 1.0, 0.8));
        out.push(polyline(lower, level.color, 1.0, 0.8));
    }
    if style.labels
        && let Some(end) = line.last()
    {
        out.push(stat(
            (end.0 + 6.0, end.1),
            "VWAP".to_owned(),
            style.color,
            Anchor::Left,
        ));
    }
    out.push(polyline(line, style.color, style.width, 1.0));
    out.push(seg(
        (pts[0].0, rect.t),
        (pts[0].0, rect.b),
        style.color,
        0.4,
        1.0,
        Dash::Dashed,
    ));
}

fn volume_profile(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
    fixed: bool,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (from, to) = if fixed {
        let (a, b) = (drawing.points[0].t, drawing.points[1].t);
        (a.min(b), a.max(b))
    } else {
        (drawing.points[0].t, i64::MAX)
    };
    let bars = proj.bars_between(from, to);
    if bars.is_empty() {
        return;
    }
    let high = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let low = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    if high <= low {
        return;
    }
    let pixels = (proj.y_of(low) - proj.y_of(high)).abs();
    let rows = ((pixels / ROW_PX).round() as usize).clamp(MIN_ROWS, MAX_ROWS);
    let step = (high - low) / rows as f64;
    let row_of = |price: f64| (((price - low) / step) as usize).min(rows - 1);

    let mut volume = vec![0.0_f64; rows];
    for bar in &bars {
        let weight = if bar.volume > 0.0 { bar.volume } else { 1.0 };
        let (a, b) = (row_of(bar.low), row_of(bar.high));
        let (first, last) = (a.min(b), a.max(b));
        let share = weight / (last - first + 1) as f64;
        for row in &mut volume[first..=last] {
            *row += share;
        }
    }
    let total: f64 = volume.iter().sum();
    let (poc, most) =
        volume.iter().copied().enumerate().fold(
            (0, 0.0),
            |best, (i, v)| if v > best.1 { (i, v) } else { best },
        );
    if most <= 0.0 {
        return;
    }
    // The value area grows from the point of control, taking the busier neighbour each time,
    // until it holds its share of the volume.
    let (mut lower, mut upper, mut held) = (poc, poc, volume[poc]);
    while held < total * VALUE_AREA && (lower > 0 || upper + 1 < rows) {
        let below = if lower > 0 { volume[lower - 1] } else { -1.0 };
        let above = if upper + 1 < rows {
            volume[upper + 1]
        } else {
            -1.0
        };
        if above >= below {
            upper += 1;
            held += volume[upper];
        } else {
            lower -= 1;
            held += volume[lower];
        }
    }

    let left = if fixed {
        pts[0].0.min(pts[1].0)
    } else {
        pts[0].0
    };
    let widest = if fixed {
        ((pts[0].0 - pts[1].0).abs() * 0.9).max(30.0)
    } else {
        ((rect.r - left) * 0.4).clamp(30.0, 300.0)
    };
    let opacity = style.fill_opacity;
    for (i, amount) in volume.iter().enumerate() {
        let w = (amount / most) as f32 * widest;
        if w < 0.5 {
            continue;
        }
        let inside = (lower..=upper).contains(&i);
        let (top, bottom) = (
            proj.y_of(low + (i + 1) as f64 * step),
            proj.y_of(low + i as f64 * step),
        );
        out.push(Prim::Rect {
            a: (left, top),
            b: (left + w, bottom),
            fill: style.fill.then(|| {
                (
                    style.fill_color(),
                    if inside { opacity } else { opacity * 0.5 },
                )
            }),
            stroke: (!style.fill).then_some((style.color, 1.0)),
        });
    }
    let poc_y = proj.y_of(low + (poc as f64 + 0.5) * step);
    let end = if fixed {
        pts[0].0.max(pts[1].0)
    } else {
        rect.r
    };
    out.push(seg((left, poc_y), (end, poc_y), POC, 1.0, 1.0, Dash::Solid));
    if style.labels {
        out.push(label(
            (left + widest + 4.0, poc_y),
            format!("POC {}", proj.format_price(low + (poc as f64 + 0.5) * step)),
            POC,
            Anchor::Left,
            style,
        ));
    }
    let edges: &[f32] = if fixed {
        &[pts[0].0, pts[1].0]
    } else {
        &[pts[0].0]
    };
    for &x in edges {
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            style.color,
            0.4,
            1.0,
            Dash::Dashed,
        ));
    }
}

// ---- shapes ----

fn rotated_rectangle(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let side = sub(b, a);
    let run = length(side);
    if run < 1e-3 {
        out.push(seg(a, b, style.color, 1.0, style.width, style.dash));
        return;
    }
    let normal = (-side.1 / run, side.0 / run);
    let height = sub(c, a).0 * normal.0 + sub(c, a).1 * normal.1;
    let push = scale(normal, height);
    let corners = vec![a, b, add(b, push), add(a, push)];
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: corners.clone(),
            fill,
        });
    }
    for i in 0..4 {
        out.push(seg(
            corners[i],
            corners[(i + 1) % 4],
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
}

fn circle(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let radius = dist(pts[0], pts[1]);
    out.push(Prim::Ellipse {
        center: pts[0],
        rx: radius,
        ry: radius,
        fill: fill_of(style),
        stroke: Some((style.color, style.width)),
    });
}

/// The circle through three points: its center and radius, or `None` when they are on a line
/// (or so nearly that the circle is far larger than any screen).
fn circumcircle(a: P, b: P, c: P) -> Option<(P, f32)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-2 {
        return None;
    }
    let (sa, sb, sc) = (
        a.0 * a.0 + a.1 * a.1,
        b.0 * b.0 + b.1 * b.1,
        c.0 * c.0 + c.1 * c.1,
    );
    let center = (
        (sa * (b.1 - c.1) + sb * (c.1 - a.1) + sc * (a.1 - b.1)) / d,
        (sa * (c.0 - b.0) + sb * (a.0 - c.0) + sc * (b.0 - a.0)) / d,
    );
    let radius = dist(center, a);
    (radius.is_finite() && radius < 50_000.0).then_some((center, radius))
}

fn arc(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let Some((center, radius)) = circumcircle(a, b, c) else {
        out.push(polyline(vec![a, c, b], style.color, style.width, 1.0));
        return;
    };
    let angle = |p: P| (p.1 - center.1).atan2(p.0 - center.0);
    let (from, to, through) = (angle(a), angle(b), angle(c));
    // The arc runs from the first point to the second, on the side of the third.
    let around = |x: f32| x.rem_euclid(TAU);
    let counter = around(to - from);
    let sweep = if around(through - from) <= counter {
        counter
    } else {
        counter - TAU
    };
    let curve = sample(64, |t| {
        let at = from + sweep * t;
        (center.0 + radius * at.cos(), center.1 + radius * at.sin())
    });
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: curve.clone(),
            fill,
        });
    }
    out.push(polyline(curve, style.color, style.width, 1.0));
}

fn curve(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    // A quadratic curve whose control is placed so that the curve passes through the middle point.
    let control = sub(scale(b, 2.0), scale(add(a, c), 0.5));
    let line = sample(48, |t| {
        let u = 1.0 - t;
        (
            u * u * a.0 + 2.0 * u * t * control.0 + t * t * c.0,
            u * u * a.1 + 2.0 * u * t * control.1 + t * t * c.1,
        )
    });
    out.push(polyline(line, style.color, style.width, 1.0));
}

fn double_curve(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    let half = (b.0 - a.0) / 2.0;
    let (c1, c2) = ((a.0 + half, a.1), (a.0 + half, b.1));
    let line = sample(48, |t| {
        let u = 1.0 - t;
        let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        (
            w0 * a.0 + w1 * c1.0 + w2 * c2.0 + w3 * b.0,
            w0 * a.1 + w1 * c1.1 + w2 * c2.1 + w3 * b.1,
        )
    });
    out.push(polyline(line, style.color, style.width, 1.0));
}

// ---- markers ----

fn arrow_marker(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (tail, tip) = (pts[0], pts[1]);
    let run = dist(tail, tip);
    if run < 2.0 {
        return;
    }
    let along = scale(sub(tip, tail), 1.0 / run);
    let normal = (-along.1, along.0);
    let shaft = 2.0 + style.width * 1.5;
    let wing = shaft * 2.4;
    let head = (wing * 1.5).min(run * 0.7);
    let base = sub(tip, scale(along, head));
    let corner = |from: P, side: f32| add(from, scale(normal, side));
    out.push(Prim::Polygon {
        points: vec![
            corner(tail, shaft),
            corner(base, shaft),
            corner(base, wing),
            tip,
            corner(base, -wing),
            corner(base, -shaft),
            corner(tail, -shaft),
        ],
        fill: (style.color, 0.95),
    });
}

/// A block arrow standing on the point, pointing up at it (`up`) or down at it.
fn arrow_mark(drawing: &Drawing, at: P, up: bool, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let k = if up { 1.0 } else { -1.0 };
    let (head, body, wing, shaft) = (12.0, 26.0, 10.0, 4.5);
    out.push(Prim::Polygon {
        points: vec![
            at,
            (at.0 + wing, at.1 + k * head),
            (at.0 + shaft, at.1 + k * head),
            (at.0 + shaft, at.1 + k * body),
            (at.0 - shaft, at.1 + k * body),
            (at.0 - shaft, at.1 + k * head),
            (at.0 - wing, at.1 + k * head),
        ],
        fill: (style.color, 0.95),
    });
}

// ---- notes ----

/// The rows of the words of a drawing, or `fallback` when it has none.
fn rows_of(drawing: &Drawing, fallback: &str) -> Vec<String> {
    let rows: Vec<String> = drawing.text.lines().map(str::to_owned).collect();
    if rows.iter().all(|r| r.trim().is_empty()) {
        vec![fallback.to_owned()]
    } else {
        rows
    }
}

/// A board of `rows` in the look of the drawing, with its top left corner at `tl`.
fn board(drawing: &Drawing, tl: P, rows: Vec<String>) -> Prim {
    let style = &drawing.style;
    Prim::Board {
        tl,
        rows,
        fg: style.text_color.unwrap_or(0xffffff),
        bg: (style.color, 0.92),
        size: style.text_size,
        bold: style.bold,
    }
}

fn dot(at: P, radius: f32, color: u32) -> Prim {
    Prim::Ellipse {
        center: at,
        rx: radius,
        ry: radius,
        fill: Some((color, 1.0)),
        stroke: None,
    }
}

fn note(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let at = pts[0];
    let rows = rows_of(drawing, "Note");
    let (_, height) = board_size(&rows, style.text_size);
    let corner = (at.0 + 10.0, at.1 - 10.0);
    out.push(seg(at, corner, style.color, 1.0, 1.0, Dash::Solid));
    out.push(dot(at, 3.5, style.color));
    out.push(board(drawing, (corner.0, corner.1 - height), rows));
}

fn price_note(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (target, place) = (pts[0], pts[1]);
    let mut rows = vec![proj.format_price(drawing.points[0].p)];
    rows.extend(
        drawing
            .text
            .lines()
            .filter(|r| !r.trim().is_empty())
            .map(str::to_owned),
    );
    let (_, height) = board_size(&rows, style.text_size);
    let tl = (place.0 + 6.0, place.1 - height / 2.0);
    out.push(seg(
        target,
        (tl.0, place.1),
        style.color,
        1.0,
        style.width,
        style.dash,
    ));
    out.push(dot(target, 3.5, style.color));
    out.push(board(drawing, tl, rows));
}

fn callout(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (target, place) = (pts[0], pts[1]);
    let rows = rows_of(drawing, "Callout");
    let (w, h) = board_size(&rows, style.text_size);
    let tl = (place.0 - w / 2.0, place.1 - h / 2.0);
    out.push(seg(
        place,
        target,
        style.color,
        1.0,
        style.width,
        style.dash,
    ));
    out.extend(arrow_head(place, target, style.color, style.width));
    out.push(board(drawing, tl, rows));
}

fn comment(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let at = pts[0];
    let rows = rows_of(drawing, "Comment");
    let (_, height) = board_size(&rows, style.text_size);
    let bottom = at.1 - 10.0;
    out.push(Prim::Polygon {
        points: vec![at, (at.0 + 14.0, bottom), (at.0 + 30.0, bottom)],
        fill: (style.color, 0.92),
    });
    out.push(board(drawing, (at.0 + 8.0, bottom - height), rows));
}

fn pin(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let at = pts[0];
    let head = (at.0, at.1 - 24.0);
    out.push(seg(head, at, style.color, 1.5, 1.5, Dash::Solid));
    out.push(dot(at, 2.5, style.color));
    out.push(Prim::Ellipse {
        center: head,
        rx: 8.0,
        ry: 8.0,
        fill: Some((style.color, 1.0)),
        stroke: Some((0xffffff, 1.0)),
    });
    let rows: Vec<String> = drawing
        .text
        .lines()
        .filter(|r| !r.trim().is_empty())
        .map(str::to_owned)
        .collect();
    if !rows.is_empty() {
        let (_, height) = board_size(&rows, style.text_size);
        out.push(board(drawing, (head.0 + 14.0, head.1 - height / 2.0), rows));
    }
}

fn signpost(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let at = pts[0];
    let top = (at.0, at.1 - 50.0);
    let rows = rows_of(drawing, "Signpost");
    let (_, height) = board_size(&rows, style.text_size);
    out.push(seg(at, top, style.color, 2.0, 2.0, Dash::Solid));
    out.push(dot(at, 2.5, style.color));
    out.push(board(drawing, (top.0, top.1 - height + 4.0), rows));
}

fn flag_mark(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let at = pts[0];
    let top = (at.0, at.1 - 34.0);
    out.push(seg(at, top, style.color, 1.0, 1.5, Dash::Solid));
    out.push(dot(at, 2.5, style.color));
    out.push(Prim::Polygon {
        points: vec![top, (at.0 + 20.0, at.1 - 27.0), (at.0, at.1 - 20.0)],
        fill: (style.color, 1.0),
    });
}

/// The cells of the words of a table: a row per line, cells split by a bar.
pub fn table_cells(text: &str) -> Vec<Vec<String>> {
    let source = if text.trim().is_empty() {
        "Item | Value\nEntry | 0.0\nStop | 0.0"
    } else {
        text
    };
    let mut rows: Vec<Vec<String>> = source
        .lines()
        .map(|line| line.split('|').map(|cell| cell.trim().to_owned()).collect())
        .collect();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1);
    for row in &mut rows {
        row.resize(columns, String::new());
    }
    rows
}

fn table(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let cells = table_cells(&drawing.text);
    let (rows, columns) = (cells.len(), cells.first().map_or(0, Vec::len));
    if rows == 0 || columns == 0 {
        return;
    }
    let size = style.text_size;
    let row_height = size * 1.9;
    let widths: Vec<f32> = (0..columns)
        .map(|c| {
            let chars = cells
                .iter()
                .map(|r| r[c].chars().count())
                .max()
                .unwrap_or(0);
            chars as f32 * size * 0.58 + 18.0
        })
        .collect();
    let (width, height) = (widths.iter().sum::<f32>(), row_height * rows as f32);
    let tl = pts[0];
    out.push(Prim::Rect {
        a: tl,
        b: (tl.0 + width, tl.1 + height),
        fill: Some((0x131722, 0.92)),
        stroke: Some((style.color, 1.0)),
    });
    out.push(Prim::Rect {
        a: tl,
        b: (tl.0 + width, tl.1 + row_height),
        fill: Some((style.color, 0.35)),
        stroke: None,
    });
    let mut x = tl.0;
    for (c, w) in widths.iter().enumerate() {
        if c > 0 {
            out.push(seg(
                (x, tl.1),
                (x, tl.1 + height),
                style.color,
                0.5,
                1.0,
                Dash::Solid,
            ));
        }
        x += w;
    }
    for r in 1..rows {
        let y = tl.1 + row_height * r as f32;
        out.push(seg(
            (tl.0, y),
            (tl.0 + width, y),
            style.color,
            0.5,
            1.0,
            Dash::Solid,
        ));
    }
    let color = style.text_color.unwrap_or(0xffffff);
    let mut x = tl.0;
    for (c, w) in widths.iter().enumerate() {
        for (r, row) in cells.iter().enumerate() {
            if row[c].is_empty() {
                continue;
            }
            out.push(Prim::Label {
                at: (x + 9.0, tl.1 + row_height * (r as f32 + 0.5)),
                text: row[c].clone(),
                color,
                background: None,
                anchor: Anchor::Left,
                size,
                bold: style.bold || r == 0,
            });
        }
        x += w;
    }
}

/// The name of the glyph an icon drawing shows: the one it holds, or the star.
pub fn icon_key(text: &str) -> &'static str {
    ICONS
        .iter()
        .find(|(key, _)| *key == text.trim())
        .map_or("star", |(key, _)| *key)
}

fn icon(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let at = pts[0];
    let half = (style.text_size * 0.9).max(8.0);
    let point = |x: f32, y: f32| (at.0 + x * half, at.1 + y * half);
    let filled = |points: Vec<P>| Prim::Polygon {
        points,
        fill: (style.color, 1.0),
    };
    match icon_key(&drawing.text) {
        "circle" => out.push(dot(at, half, style.color)),
        "square" => out.push(filled(vec![
            point(-0.85, -0.85),
            point(0.85, -0.85),
            point(0.85, 0.85),
            point(-0.85, 0.85),
        ])),
        "diamond" => out.push(filled(vec![
            point(0.0, -1.0),
            point(0.8, 0.0),
            point(0.0, 1.0),
            point(-0.8, 0.0),
        ])),
        "triangle_up" => out.push(filled(vec![
            point(0.0, -1.0),
            point(0.95, 0.8),
            point(-0.95, 0.8),
        ])),
        "triangle_down" => out.push(filled(vec![
            point(0.0, 1.0),
            point(0.95, -0.8),
            point(-0.95, -0.8),
        ])),
        "cross" => {
            out.push(seg(
                point(-0.7, -0.7),
                point(0.7, 0.7),
                style.color,
                1.0,
                3.0,
                Dash::Solid,
            ));
            out.push(seg(
                point(-0.7, 0.7),
                point(0.7, -0.7),
                style.color,
                1.0,
                3.0,
                Dash::Solid,
            ));
        }
        "check" => out.push(polyline(
            vec![point(-0.8, 0.05), point(-0.25, 0.65), point(0.85, -0.7)],
            style.color,
            3.0,
            1.0,
        )),
        "heart" => out.push(filled(sample(40, |t| {
            let a = t * TAU;
            let x = 16.0 * a.sin().powi(3);
            let y =
                13.0 * a.cos() - 5.0 * (2.0 * a).cos() - 2.0 * (3.0 * a).cos() - (4.0 * a).cos();
            point(x / 17.0, -y / 17.0)
        }))),
        "bolt" => out.push(filled(vec![
            point(0.3, -1.0),
            point(-0.6, 0.15),
            point(-0.05, 0.15),
            point(-0.3, 1.0),
            point(0.6, -0.2),
            point(0.05, -0.2),
            point(0.45, -1.0),
        ])),
        _ => {
            // A five pointed star: the tips on a circle, the notches between them on a smaller one.
            let star = (0..10)
                .map(|i| {
                    let angle = -FRAC_PI_2 + i as f32 * PI / 5.0;
                    let radius = if i % 2 == 0 { 1.0 } else { 0.42 };
                    point(radius * angle.cos(), radius * angle.sin())
                })
                .collect();
            out.push(filled(star));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::geometry::tests::{Linear, drawing};
    use super::super::geometry::{Prim, handles, hit, prims as shapes_of};
    use super::*;

    /// `count` points spread over the plot, a few minutes and a few dozen units apart.
    fn spread(count: usize) -> Vec<(i64, f64)> {
        (0..count)
            .map(|i| (600 + 240 * i as i64, 120.0 + 35.0 * ((i * 3 % 5) as f64)))
            .collect()
    }

    fn segments(shapes: &[Prim]) -> Vec<(P, P)> {
        shapes
            .iter()
            .filter_map(|p| match p {
                Prim::Segment { a, b, .. } => Some((*a, *b)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_tool_draws_something_and_offers_a_grip_per_point() {
        for tool in Tool::ALL {
            let count = if tool.is_freehand() {
                6
            } else {
                tool.anchors()
            };
            let d = drawing(tool, &spread(count));
            assert!(d.is_valid(), "{tool:?} is valid with its points");
            let shapes = shapes_of(&d, &Linear);
            assert!(!shapes.is_empty(), "{tool:?} draws nothing");
            let grips = handles(&d, &Linear).len();
            if tool.is_freehand() {
                assert_eq!(grips, 0, "{tool:?}");
            } else if tool.is_box() {
                assert_eq!(grips, 8, "{tool:?}");
            } else {
                assert_eq!(grips, count, "{tool:?}");
            }
        }
    }

    #[test]
    fn every_tool_can_be_picked_on_what_it_draws() {
        for tool in Tool::ALL {
            let count = if tool.is_freehand() {
                6
            } else {
                tool.anchors()
            };
            let d = drawing(tool, &spread(count));
            let picked = handles(&d, &Linear)
                .first()
                .is_some_and(|grip| hit(&d, &Linear, *grip, true).is_some());
            let on_line = shapes_of(&d, &Linear).iter().any(|shape| match shape {
                Prim::Segment { a, b, .. } => hit(&d, &Linear, mid(*a, *b), false).is_some(),
                _ => false,
            });
            assert!(
                picked || on_line || tool.is_freehand(),
                "{tool:?} cannot be picked"
            );
        }
    }

    #[test]
    fn a_regression_runs_through_the_closes_of_its_range() {
        // On the test projection the closes are 101 plus the bar number: a perfect line.
        let d = drawing(Tool::RegressionTrend, &[(600, 100.0), (1_200, 100.0)]);
        let lines = segments(&shapes_of(&d, &Linear));
        let base = lines
            .iter()
            .find(|(a, _)| (a.0 - 600.0).abs() < 1e-3 && (a.1 - 389.0).abs() < 1e-2)
            .expect("a line from the first close");
        assert!((base.1.0 - 1_200.0).abs() < 1e-3);
        assert!((base.1.1 - 379.0).abs() < 1e-2, "ends on the last close");
    }

    #[test]
    fn cycle_lines_repeat_across_the_plot() {
        let d = drawing(Tool::CyclicLines, &[(60, 100.0), (180, 100.0)]);
        let verticals = segments(&shapes_of(&d, &Linear))
            .into_iter()
            .filter(|(a, b)| (a.0 - b.0).abs() < 1e-3)
            .count();
        assert!((8..=10).contains(&verticals), "{verticals} lines");
    }

    #[test]
    fn a_circle_goes_through_three_points() {
        let (center, radius) = circumcircle((0.0, 0.0), (10.0, 0.0), (5.0, 5.0)).unwrap();
        assert!((center.0 - 5.0).abs() < 1e-3 && center.1.abs() < 1e-3);
        assert!((radius - 5.0).abs() < 1e-3);
        assert!(circumcircle((0.0, 0.0), (5.0, 5.0), (10.0, 10.0)).is_none());
    }

    #[test]
    fn an_arc_ends_on_its_two_points_and_passes_the_third() {
        let d = drawing(Tool::Arc, &[(100, 200.0), (300, 200.0), (200, 260.0)]);
        let shapes = shapes_of(&d, &Linear);
        let Some(Prim::Polyline { points, .. }) =
            shapes.iter().find(|s| matches!(s, Prim::Polyline { .. }))
        else {
            panic!("an arc is a polyline");
        };
        let (first, last) = (points[0], points[points.len() - 1]);
        assert!(dist(first, (100.0, 300.0)) < 0.5, "{first:?}");
        assert!(dist(last, (300.0, 300.0)) < 0.5, "{last:?}");
        assert!(
            points.iter().any(|p| dist(*p, (200.0, 240.0)) < 3.0),
            "passes the third point"
        );
    }

    #[test]
    fn a_curve_passes_through_its_middle_point() {
        let d = drawing(Tool::Curve, &[(100, 100.0), (300, 250.0), (500, 100.0)]);
        let shapes = shapes_of(&d, &Linear);
        let Some(Prim::Polyline { points, .. }) = shapes.first() else {
            panic!("a curve is a polyline");
        };
        assert!(points.iter().any(|p| dist(*p, (300.0, 250.0)) < 1.0));
    }

    #[test]
    fn a_fixed_gann_square_is_square_on_the_screen() {
        assert_eq!(
            fixed_square_corner((100.0, 100.0), (150.0, 400.0)),
            (400.0, 400.0)
        );
        assert_eq!(
            fixed_square_corner((100.0, 100.0), (20.0, 60.0)),
            (20.0, 20.0)
        );
    }

    #[test]
    fn the_volume_profile_marks_its_point_of_control() {
        let d = drawing(
            Tool::FixedRangeVolumeProfile,
            &[(600, 100.0), (1_200, 100.0)],
        );
        let shapes = shapes_of(&d, &Linear);
        let rows = shapes
            .iter()
            .filter(|s| matches!(s, Prim::Rect { .. }))
            .count();
        assert!(rows >= MIN_ROWS, "{rows} rows");
        assert!(
            shapes
                .iter()
                .any(|s| matches!(s, Prim::Segment { color, .. } if *color == POC))
        );
    }

    #[test]
    fn a_ghost_feed_is_the_same_every_time_it_is_drawn() {
        let d = drawing(
            Tool::GhostFeed,
            &[(600, 100.0), (900, 100.0), (1_200, 150.0)],
        );
        assert_eq!(shapes_of(&d, &Linear), shapes_of(&d, &Linear));
    }

    #[test]
    fn a_bars_pattern_copies_the_bars_of_its_range() {
        let d = drawing(
            Tool::BarsPattern,
            &[(600, 100.0), (900, 100.0), (1_200, 150.0)],
        );
        let bodies = shapes_of(&d, &Linear)
            .iter()
            .filter(|s| matches!(s, Prim::Rect { fill: Some((_, a)), .. } if *a > 0.5))
            .count();
        assert_eq!(bodies, 6, "the bars of minutes 10 to 15");
    }

    #[test]
    fn a_table_reads_cells_from_bars_and_lines() {
        let cells = table_cells("a | b\nc");
        assert_eq!(cells, vec![vec!["a", "b"], vec!["c", ""]]);
        assert_eq!(table_cells("  ").len(), 3, "an empty table shows a sample");
    }

    #[test]
    fn an_unknown_icon_is_a_star() {
        assert_eq!(icon_key(""), "star");
        assert_eq!(icon_key("nonsense"), "star");
        assert_eq!(icon_key(" heart "), "heart");
    }

    #[test]
    fn a_note_grows_with_its_words() {
        let mut d = drawing(Tool::Note, &[(300, 200.0)]);
        d.text = "one\nsecond line".to_owned();
        let Some(Prim::Board { rows, .. }) = shapes_of(&d, &Linear)
            .into_iter()
            .find(|s| matches!(s, Prim::Board { .. }))
        else {
            panic!("a note is a board");
        };
        assert_eq!(rows, vec!["one".to_owned(), "second line".to_owned()]);
    }
}
