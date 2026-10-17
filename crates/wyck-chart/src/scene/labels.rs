//! The axes, drawn over the bands: the labels and tags of the price axis on the right, the times
//! and the time zone under the plot.

use super::bands::{PaneScale, Plan};
use super::cmd::{Align, Cmd, P, font, hsla, line_height};
use super::ctx::Ctx;
use super::geometry::{Band, Geometry, zone_w};
use super::{axes, price, studies};
use wyck_core::math::f32_of;
use wyck_drawings::look::Face;
use wyck_indicators::spec::ValueFormat;

/// Draws the axes into `out`. The tags of the price axes are made first and spread so none hides
/// another, and the labels of the scales keep clear of them. The pointer's tags go over everything
/// and move nothing.
pub(super) fn draw(
    cx: &Ctx<'_>,
    plan: &Plan,
    geometry: &Geometry,
    panes: &[PaneScale],
    pointer: Option<(P, usize)>,
    out: &mut Vec<Cmd>,
) {
    let frame = cx.f;
    let mut tags = Vec::new();
    studies::value_tags(cx, panes, &mut tags);
    let mut pointer_tags = Vec::new();
    axes::draw_axis_tags(cx, pointer, geometry, panes, &mut tags, &mut pointer_tags);
    axes::spread_tags(&mut tags, geometry, cx.oy);

    out.push(axes::axes_background(frame, geometry));
    for band in geometry.bands.iter().skip(1) {
        out.push(cx.hline(
            cx.oy + f32_of(band.top),
            cx.ox,
            cx.ox + f32_of(frame.w),
            hsla(frame.palette.border),
        ));
    }
    let taken: Vec<(f32, f32)> = tags
        .iter()
        .chain(&pointer_tags)
        .filter_map(|cmd| match cmd {
            Cmd::Tag {
                y,
                height,
                fixed_width: Some(_),
                ..
            } => Some((*y, y + height)),
            _ => None,
        })
        .collect();
    price_labels(cx, plan, panes, &taken, out);
    time_labels(cx, plan, geometry, out);
    zone(cx, geometry, out);
    out.extend(tags);
    out.extend(pointer_tags);
}

/// The values of the scales of the prices and of every pane, written on the axis where no tag
/// (`taken`, as spans of y) is in the way.
fn price_labels(
    cx: &Ctx<'_>,
    plan: &Plan,
    panes: &[PaneScale],
    taken: &[(f32, f32)],
    out: &mut Vec<Cmd>,
) {
    let axis_x = cx.ox + f32_of(cx.plot_w) + 8.0;
    let label = |text: String, y: f32| Cmd::Text {
        text,
        x: axis_x,
        y: y - line_height() / 2.0,
        size: font(),
        color: hsla(cx.f.palette.text),
        align: Align::Left,
        bold: false,
        face: Face::default(),
    };
    let fits = |band: &Band, y: f32| {
        y >= cx.oy + f32_of(band.top) + 6.0
            && y <= cx.oy + f32_of(band.bottom()) - 6.0
            && taken
                .iter()
                .all(|(top, bottom)| y + 6.0 < *top || y - 6.0 > *bottom)
    };
    for value in &plan.price_ticks {
        let y = cx.y(*value);
        if fits(&cx.band, y) {
            out.push(label(
                cx.map.label(*value, ValueFormat::Price, cx.f.digits),
                y,
            ));
        }
    }
    for pane in panes {
        for value in &pane.ticks {
            let y = cx.y_on(&pane.map, *value);
            if fits(&pane.band, y) {
                out.push(label(
                    price::format_value(*value, pane.format, cx.f.digits),
                    y,
                ));
            }
        }
    }
}

/// The labels of the time axis, stopping short of the time zone at its right end.
fn time_labels(cx: &Ctx<'_>, plan: &Plan, geometry: &Geometry, out: &mut Vec<Cmd>) {
    let palette = cx.f.palette;
    let zone_left = cx.ox + f32_of(cx.plot_w) - zone_w();
    for label in &plan.time_labels {
        if cx.x(label.index) > zone_left - 16.0 {
            continue;
        }
        out.push(Cmd::Text {
            text: label.text.clone(),
            x: cx.x(label.index),
            y: cx.oy + f32_of(geometry.plot_h()) + 8.0,
            size: font(),
            color: if label.major {
                hsla(palette.text_strong)
            } else {
                hsla(palette.text)
            },
            align: Align::Center,
            bold: label.major,
            face: Face::default(),
        });
    }
}

/// The zone the times are in, at the right end of the time axis. The scale buttons are in the
/// corner under the price axis (drawn over the chart, not here).
fn zone(cx: &Ctx<'_>, geometry: &Geometry, out: &mut Vec<Cmd>) {
    let corner_time = cx.f.raw.last_time().unwrap_or(cx.f.now_ms);
    out.push(Cmd::Text {
        text: cx.f.settings.zone.label(corner_time),
        x: cx.ox + f32_of(cx.plot_w) - 8.0,
        y: cx.oy + f32_of(geometry.plot_h()) + 8.0,
        size: font(),
        color: hsla(cx.f.palette.text),
        align: Align::Right,
        bold: false,
        face: Face::default(),
    });
}
