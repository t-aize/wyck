//! Markers, notes, tables and icons.

use super::{ICONS, add, dist, polyline, sample, scale, sub};

use crate::geometry::{Anchor, P, Prim, Projection, arrow_head, board_size, lead, seg};
use crate::model::Drawing;
use crate::model::style::Dash;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use wyck_core::math::count_f32;

pub(super) fn arrow_marker(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some([tail, tip]) = lead(pts) else {
        return;
    };
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
pub(super) fn arrow_mark(drawing: &Drawing, at: P, up: bool, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let k = if up { 1.0 } else { -1.0 };
    let scale = style.scale;
    let (head, body, wing, shaft) = (12.0 * scale, 26.0 * scale, 10.0 * scale, 4.5 * scale);
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

/// The rows of the words of a drawing, or `fallback` when it has none.
pub(super) fn rows_of(drawing: &Drawing, fallback: &str) -> Vec<String> {
    let rows: Vec<String> = drawing.text.lines().map(str::to_owned).collect();
    if rows.iter().all(|r| r.trim().is_empty()) {
        vec![fallback.to_owned()]
    } else {
        rows
    }
}

/// A board of `rows` in the look of the drawing, with its top left corner at `tl`.
pub(super) fn board(drawing: &Drawing, tl: P, rows: Vec<String>) -> Prim {
    let style = &drawing.style;
    Prim::Board {
        tl,
        rows,
        fg: style.text_color.unwrap_or(0xff_ffff),
        bg: (style.color, 0.92),
        size: style.text_size,
        bold: style.bold,
        face: style.face(),
    }
}

pub(super) fn dot(at: P, radius: f32, color: u32) -> Prim {
    Prim::Ellipse {
        center: at,
        rx: radius,
        ry: radius,
        fill: Some((color, 1.0)),
        stroke: None,
    }
}

pub(super) fn note(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some(&at) = pts.first() else {
        return;
    };
    let rows = rows_of(drawing, "Note");
    let (_, height) = board_size(&rows, style.text_size);
    let corner = (at.0 + 10.0, at.1 - 10.0);
    out.push(seg(at, corner, style.color, 1.0, 1.0, Dash::Solid));
    out.push(dot(at, 3.5, style.color));
    out.push(board(drawing, (corner.0, corner.1 - height), rows));
}

pub(super) fn price_note(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (Some([target, place]), Some(first)) = (lead(pts), drawing.points.first()) else {
        return;
    };
    let mut rows = vec![proj.format_price(first.p)];
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

pub(super) fn callout(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some([target, place]) = lead(pts) else {
        return;
    };
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

pub(super) fn comment(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some(&at) = pts.first() else {
        return;
    };
    let rows = rows_of(drawing, "Comment");
    let (_, height) = board_size(&rows, style.text_size);
    let bottom = at.1 - 10.0;
    out.push(Prim::Polygon {
        points: vec![at, (at.0 + 14.0, bottom), (at.0 + 30.0, bottom)],
        fill: (style.color, 0.92),
    });
    out.push(board(drawing, (at.0 + 8.0, bottom - height), rows));
}

pub(super) fn pin(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some(&at) = pts.first() else {
        return;
    };
    let head = (at.0, at.1 - 24.0);
    out.push(seg(head, at, style.color, 1.5, 1.5, Dash::Solid));
    out.push(dot(at, 2.5, style.color));
    out.push(Prim::Ellipse {
        center: head,
        rx: 8.0,
        ry: 8.0,
        fill: Some((style.color, 1.0)),
        stroke: Some((0xff_ffff, 1.0)),
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

pub(super) fn signpost(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some(&at) = pts.first() else {
        return;
    };
    let top = (at.0, at.1 - 50.0);
    let rows = rows_of(drawing, "Signpost");
    let (_, height) = board_size(&rows, style.text_size);
    out.push(seg(at, top, style.color, 2.0, 2.0, Dash::Solid));
    out.push(dot(at, 2.5, style.color));
    out.push(board(drawing, (top.0, top.1 - height + 4.0), rows));
}

pub(super) fn flag_mark(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some(&at) = pts.first() else {
        return;
    };
    let k = style.scale;
    let top = (at.0, at.1 - 34.0 * k);
    out.push(seg(
        at,
        top,
        style.color,
        1.0,
        1.5 * k.max(0.7),
        Dash::Solid,
    ));
    out.push(dot(at, 2.5 * k, style.color));
    out.push(Prim::Polygon {
        points: vec![
            top,
            (at.0 + 20.0 * k, at.1 - 27.0 * k),
            (at.0, at.1 - 20.0 * k),
        ],
        fill: (style.color, 1.0),
    });
}

/// The cells of the words of a table: a row per line, cells split by a bar.
pub(crate) fn table_cells(text: &str) -> Vec<Vec<String>> {
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

pub(super) fn table(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
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
                .map(|r| r.get(c).map_or(0, |cell| cell.chars().count()))
                .max()
                .unwrap_or(0);
            count_f32(chars) * size * 0.58 + 18.0
        })
        .collect();
    let (width, height) = (widths.iter().sum::<f32>(), row_height * count_f32(rows));
    let Some(&tl) = pts.first() else {
        return;
    };
    out.push(Prim::Rect {
        a: tl,
        b: (tl.0 + width, tl.1 + height),
        fill: Some((0x13_1722, 0.92)),
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
        let y = tl.1 + row_height * count_f32(r);
        out.push(seg(
            (tl.0, y),
            (tl.0 + width, y),
            style.color,
            0.5,
            1.0,
            Dash::Solid,
        ));
    }
    let color = style.text_color.unwrap_or(0xff_ffff);
    let mut x = tl.0;
    for (c, w) in widths.iter().enumerate() {
        for (r, row) in cells.iter().enumerate() {
            let Some(cell) = row.get(c).filter(|cell| !cell.is_empty()) else {
                continue;
            };
            out.push(Prim::Label {
                at: (x + 9.0, tl.1 + row_height * (count_f32(r) + 0.5)),
                text: cell.clone(),
                color,
                background: None,
                anchor: Anchor::Left,
                size,
                bold: style.bold || r == 0,
                face: style.face(),
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

pub(super) fn icon(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some(&at) = pts.first() else {
        return;
    };
    let half = (style.text_size * 0.9 * style.scale).max(8.0);
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
                    let angle = -FRAC_PI_2 + count_f32(i) * PI / 5.0;
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

    use crate::geometry::tests::{Linear, drawing};
    use crate::geometry::{Prim, prims as shapes_of};

    use super::*;

    use crate::extras::*;

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
