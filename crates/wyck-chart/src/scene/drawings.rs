//! The drawings and the script drawings of a chart, as commands.

use super::axes::dash_pattern;
use super::color::transparent_black;
use super::ctx::Ctx;
use super::{Align, Cmd, DrawingView, P, hsla, rgb_alpha};
use crate::projection::ChartProjection;
use wyck_core::math::count_f32;
use wyck_drawings::geometry as shapes;
use wyck_drawings::geometry::{Anchor, Prim};
use wyck_drawings::model::style::Dash;

/// Draws the drawings of the symbol into `out`, and returns what must go over the prices: the grips
/// of the selected and of the hovered drawing (a candle over a grip makes it hard to use). The tags
/// of a position stay under the prices, and only show on the drawing the pointer is working with,
/// so that a position seen on another chart, or ten of them, never hide the candles.
pub(super) fn draw_drawings(cx: &Ctx<'_>, view: &DrawingView<'_>, out: &mut Vec<Cmd>) -> Vec<Cmd> {
    let mut over = Vec::new();
    let projection = ChartProjection {
        series: cx.series,
        view: cx.f.view,
        map: cx.map,
        plot_w: cx.plot_w,
        plot_h: cx.band.h,
        step_ms: cx.f.step_ms(),
        digits: cx.f.digits,
        pip_position: cx.f.pip_position,
    };
    for drawing in view
        .list
        .iter()
        .filter(|d| (view.visible)(d))
        .chain(view.creating)
    {
        let hovered = view.hovered == Some(drawing.id);
        for prim in shapes::prims_with(drawing, &projection, hovered) {
            push_prim(cx, prim, out);
        }
        // The selected drawings show their grips (the main one is the one they can be grabbed
        // by), and so does the drawing under the pointer, which a press on a grip takes.
        let selected = view.selected == Some(drawing.id) || view.also.contains(&drawing.id);
        let grips = selected || hovered;
        if grips && !drawing.locked {
            for at in shapes::handles(drawing, &projection) {
                push_prim(cx, Prim::Handle { at }, &mut over);
            }
        }
    }
    over
}

/// Draws generated objects using the same geometry as the manual tools, without edit handles.
pub(super) fn draw_script_drawings(cx: &Ctx<'_>, out: &mut Vec<Cmd>) {
    let projection = ChartProjection {
        series: cx.series,
        view: cx.f.view,
        map: cx.map,
        plot_w: cx.plot_w,
        plot_h: cx.band.h,
        step_ms: cx.f.step_ms(),
        digits: cx.f.digits,
        pip_position: cx.f.pip_position,
    };
    let timeframe = cx.f.timeframe.code();
    for (config, output) in cx.f.settings.studies.iter().zip(&cx.f.display.studies) {
        if !config.visible {
            continue;
        }
        let Some(output) = output else {
            continue;
        };
        for script_drawing in &output.drawings {
            let drawing = &script_drawing.drawing;
            if drawing.shows_on(&timeframe) {
                for prim in shapes::prims_with(drawing, &projection, false) {
                    push_prim(cx, prim, out);
                }
            }
        }
    }
}

pub(crate) fn ellipse_points(center: P, rx: f32, ry: f32, steps: usize) -> Vec<P> {
    (0..=steps)
        .map(|i| {
            let angle = count_f32(i) / count_f32(steps) * std::f32::consts::TAU;
            (center.0 + rx * angle.cos(), center.1 + ry * angle.sin())
        })
        .collect()
}

/// Turns one shape of a drawing (in plot coordinates) into drawing commands.
pub(super) fn push_prim(cx: &Ctx<'_>, prim: Prim, out: &mut Vec<Cmd>) {
    let (ox, oy) = (cx.ox, cx.oy);
    let at = |p: P| (ox + p.0, oy + p.1);
    let thin = 1.0 / cx.f.scale;
    match prim {
        Prim::Segment {
            a,
            b,
            color,
            alpha,
            width,
            dash,
        } => {
            let (a, b) = (at(a), at(b));
            let paint = rgb_alpha(color, alpha);
            let solid = dash == Dash::Solid;
            if solid && width <= 1.0 && (a.1 - b.1).abs() < 0.01 {
                out.push(cx.hline(a.1, a.0.min(b.0), a.0.max(b.0), paint));
            } else if solid && width <= 1.0 && (a.0 - b.0).abs() < 0.01 {
                out.push(cx.vline(a.0, a.1, b.1, paint));
            } else {
                out.push(Cmd::Stroke {
                    points: vec![a, b],
                    width,
                    color: paint,
                    dash: dash_pattern(dash, width),
                });
            }
        }
        Prim::Rect {
            a,
            b,
            fill: inside,
            stroke: edge,
        } => {
            let (a, b) = (at(a), at(b));
            let (left, right) = (a.0.min(b.0), a.0.max(b.0));
            let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
            out.push(Cmd::Rect {
                x: left,
                y: top,
                w: right - left,
                h: bottom - top,
                fill: inside.map_or_else(transparent_black, |(c, al)| rgb_alpha(c, al)),
                border: edge.map(|(c, width)| (width.max(thin), rgb_alpha(c, 1.0))),
                radius: 0.0,
            });
        }
        Prim::Ellipse {
            center,
            rx,
            ry,
            fill: inside,
            stroke: edge,
        } => {
            let ring = ellipse_points(at(center), rx, ry, 64);
            if let Some((c, al)) = inside {
                out.push(Cmd::Fill {
                    points: ring.clone(),
                    color: rgb_alpha(c, al),
                });
            }
            if let Some((c, width)) = edge {
                out.push(Cmd::Stroke {
                    points: ring,
                    width,
                    color: rgb_alpha(c, 1.0),
                    dash: None,
                });
            }
        }
        Prim::Polygon {
            points,
            fill: (c, al),
        } => {
            out.push(Cmd::Fill {
                points: points.into_iter().map(at).collect(),
                color: rgb_alpha(c, al),
            });
        }
        Prim::Polyline {
            points,
            color,
            width,
            alpha,
        } => {
            out.push(Cmd::Stroke {
                points: points.into_iter().map(at).collect(),
                width,
                color: rgb_alpha(color, alpha),
                dash: None,
            });
        }
        Prim::Board {
            tl,
            rows,
            fg,
            bg: (bg, bg_alpha),
            size,
            bold,
            face,
        } => {
            let (w, h) = shapes::board_size(&rows, size);
            let (row, pad_x, pad_y) = shapes::board_metrics();
            let (x, y) = at(tl);
            out.push(Cmd::Rect {
                x,
                y,
                w,
                h,
                fill: rgb_alpha(bg, bg_alpha),
                border: None,
                radius: 4.0,
            });
            for (i, text) in rows.into_iter().enumerate() {
                out.push(Cmd::Text {
                    text,
                    x: x + pad_x,
                    y: y + pad_y + count_f32(i) * size * row + (size * row - size * 1.3) / 2.0,
                    size,
                    color: rgb_alpha(fg, 1.0),
                    align: Align::Left,
                    bold,
                    face,
                });
            }
        }
        Prim::Label {
            at: position,
            text,
            color,
            background,
            anchor,
            size,
            bold,
            face,
        } => {
            let (x, y) = at(position);
            let align = match anchor {
                Anchor::Left => Align::Left,
                Anchor::Right => Align::Right,
                Anchor::Center => Align::Center,
            };
            out.push(match background {
                Some((bg, al)) => Cmd::Tag {
                    text,
                    x,
                    y: y - size * 0.8,
                    height: size * 1.6,
                    pad: 6.0,
                    bg: rgb_alpha(bg, al),
                    fg: rgb_alpha(color, 1.0),
                    align,
                    fixed_width: None,
                    within: None,
                    size,
                    bold,
                    face,
                },
                None => Cmd::Text {
                    text,
                    x,
                    y: y - size * 1.3 / 2.0,
                    size,
                    color: rgb_alpha(color, 1.0),
                    align,
                    bold,
                    face,
                },
            });
        }
        Prim::FittedLabel {
            bounds,
            text,
            color,
            background,
            align,
            valign,
            size,
            bold,
            face,
        } => {
            out.push(Cmd::FittedText {
                text,
                x: ox + bounds.l,
                y: oy + bounds.t,
                w: (bounds.r - bounds.l).max(0.0),
                h: (bounds.b - bounds.t).max(0.0),
                size,
                color: rgb_alpha(color, 1.0),
                background: background.map(|(color, alpha)| rgb_alpha(color, alpha)),
                align: match align {
                    wyck_drawings::look::HAlign::Start => Align::Left,
                    wyck_drawings::look::HAlign::Center => Align::Center,
                    wyck_drawings::look::HAlign::End => Align::Right,
                },
                valign,
                bold,
                face,
            });
        }
        Prim::Handle { at: position } => {
            let (x, y) = at(position);
            let r = 4.5;
            out.push(Cmd::Rect {
                x: x - r,
                y: y - r,
                w: r * 2.0,
                h: r * 2.0,
                fill: hsla(cx.f.palette.bg),
                border: Some((1.5, hsla(cx.f.palette.accent))),
                radius: r,
            });
        }
    }
}
