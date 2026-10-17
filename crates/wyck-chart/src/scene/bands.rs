//! The bands of a frame: the prices, then one pane for each indicator that has a scale of its own.
//! Each band is clipped to its rectangle; the vertical line of the crosshair crosses all of them.

use super::cmd::{Align, Cmd, P, hsla, rgb_alpha, with_alpha};
use super::ctx::Ctx;
use super::geometry::{Band, Geometry, PaneScale};
use super::price::PriceMap;
use super::{axes, drawings, quote_unit, series, studies};
use crate::axis::{self, TimeLabel};
use wyck_core::math::{count_f64, f32_of};
use wyck_drawings::look::Face;
use wyck_indicators::spec::ValueFormat;

/// The size of the text of the watermark.
fn watermark_size() -> f32 {
    wyck_core::text_scale::scaled(46.0)
}

/// What a frame works out once and every band reads: the points on screen, the labels of the time
/// axis, the ticks of the prices and which grid lines show.
pub(super) struct Plan {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) time_labels: Vec<TimeLabel>,
    pub(super) price_ticks: Vec<f64>,
    grid_horizontal: bool,
    grid_vertical: bool,
}

impl Plan {
    pub(super) fn new(cx: &Ctx<'_>) -> Self {
        let frame = cx.f;
        let (first, last) = frame.view.visible(cx.len, cx.plot_w);
        // Bricks and columns are not periods: they are labeled by date, further apart.
        let spacing = if frame.settings.kind.keeps_time() || !frame.display.is_derived() {
            96.0
        } else {
            120.0
        };
        let time_labels = axis::time_labels(
            first,
            last,
            |i| cx.series.time_at(i),
            |i| frame.view.x_of(count_f64(i), cx.len, cx.plot_w),
            cx.plot_w,
            spacing,
            frame.settings.zone,
        );
        let price_ticks = cx
            .map
            .ticks(axes::axis_tick_target(cx.band.h), quote_unit(frame.digits));
        Self {
            first,
            last,
            time_labels,
            price_ticks,
            grid_horizontal: frame.settings.grid && frame.settings.grid_horizontal,
            grid_vertical: frame.settings.grid && frame.settings.grid_vertical,
        }
    }
}

/// The prices band: the grid, the indicators drawn on the prices, the drawings, the price lines
/// and the prices themselves.
pub(super) fn prices(
    cx: &Ctx<'_>,
    plan: &Plan,
    geometry: &Geometry,
    pointer: Option<(P, usize)>,
) -> Cmd {
    let frame = cx.f;
    let mut plot: Vec<Cmd> = Vec::new();
    plot.extend(watermark(cx));
    grid(cx, plan, cx.band, &cx.map, &plan.price_ticks, &mut plot);
    if frame.settings.price_lines.day_breaks {
        for index in axes::day_breaks(frame, plan.first, plan.last) {
            plot.push(cx.vline(
                cx.x(index),
                cx.oy,
                cx.oy + f32_of(geometry.plot_h()),
                with_alpha(frame.palette.text, 0.28),
            ));
        }
    }
    studies::overlays_under(cx, plan.first, plan.last, &mut plot);
    studies::overlays(cx, plan.first, &mut plot);
    drawings::draw_script_drawings(cx, &mut plot);
    marks(cx, &mut plot);
    if let Some(view) = &frame.drawings {
        drawings::draw_drawings(cx, view, &mut plot);
    }
    axes::draw_price_lines(cx, &mut plot);
    // Price marks remain readable even when a scripted or manual drawing fills their area.
    series::draw(cx, frame.kind(), plan.first, plan.last, &mut plot);
    if let Some(((_, hy), 0)) = pointer {
        plot.push(crosshair_line(cx, hy));
    }
    clipped(cx, cx.band, plot)
}

/// The panes under the prices, into `out`: one for each visible indicator that has a scale of its
/// own. Returns their scales, which the price axis writes.
pub(super) fn panes(
    cx: &Ctx<'_>,
    plan: &Plan,
    geometry: &Geometry,
    pointer: Option<(P, usize)>,
    out: &mut Vec<Cmd>,
) -> Vec<PaneScale> {
    let frame = cx.f;
    let mut scales = Vec::new();
    for (i, pane) in frame.settings.panes().iter().enumerate() {
        let band_index = i + 1;
        let (Some(&band), Some(config)) = (
            geometry.bands.get(band_index),
            frame.settings.studies.get(pane.study),
        ) else {
            continue;
        };
        let Some(Some(output)) = frame.display.studies.get(pane.study) else {
            continue;
        };
        let map = studies::pane_map(
            config,
            output,
            config.spec().range,
            band,
            plan.first,
            plan.last,
        );
        let format = config.value_format();
        let min_step = match format {
            ValueFormat::Price => quote_unit(frame.digits),
            ValueFormat::Plain(decimals) | ValueFormat::Percent(decimals) => {
                10f64.powi(-i32::try_from(decimals).unwrap_or(i32::MAX))
            }
            ValueFormat::Count => 1.0,
        };
        let ticks = axis::price_ticks(map.lo, map.hi, axes::axis_tick_target(band.h), min_step);
        let mut inner = Vec::new();
        grid(cx, plan, band, &map, &ticks, &mut inner);
        studies::pane(cx, config, output, &map, plan.first, plan.last, &mut inner);
        if let Some(((_, hy), at)) = pointer
            && at == band_index
        {
            inner.push(crosshair_line(cx, hy));
        }
        out.push(clipped(cx, band, inner));
        scales.push(PaneScale {
            study: pane.study,
            band_index,
            band,
            map,
            format,
            ticks,
        });
    }
    scales
}

/// The vertical line of the crosshair, which crosses every band.
pub(super) fn crosshair_column(
    cx: &Ctx<'_>,
    geometry: &Geometry,
    pointer: Option<(P, usize)>,
) -> Option<Cmd> {
    let ((hx, _), _) = pointer?;
    let x = if cx.len > 0 {
        let index =
            cx.f.view
                .index_at(f64::from(hx), cx.len, cx.plot_w)
                .round()
                .clamp(0.0, count_f64(cx.len - 1));
        cx.xf(index)
    } else {
        cx.ox + hx
    };
    let plot_h = f32_of(geometry.plot_h());
    Some(Cmd::Clip {
        x: cx.ox,
        y: cx.oy,
        w: f32_of(cx.plot_w),
        h: plot_h,
        inner: vec![Cmd::Stroke {
            points: vec![
                (cx.snap(x) + 0.5, cx.oy),
                (cx.snap(x) + 0.5, cx.oy + plot_h),
            ],
            width: 1.0,
            color: hsla(cx.f.palette.crosshair),
            dash: cx.f.settings.crosshair.dash(),
        }],
    })
}

/// The horizontal line of the crosshair, at `hy` from the top of the chart.
fn crosshair_line(cx: &Ctx<'_>, hy: f32) -> Cmd {
    let y = cx.snap(cx.oy + hy) + 0.5;
    Cmd::Stroke {
        points: vec![(cx.ox, y), (cx.ox + f32_of(cx.plot_w), y)],
        width: 1.0,
        color: hsla(cx.f.palette.crosshair),
        dash: cx.f.settings.crosshair.dash(),
    }
}

/// `inner` cut to the rectangle of `band`.
fn clipped(cx: &Ctx<'_>, band: Band, inner: Vec<Cmd>) -> Cmd {
    Cmd::Clip {
        x: cx.ox,
        y: cx.oy + f32_of(band.top),
        w: f32_of(cx.plot_w),
        h: f32_of(band.h),
        inner,
    }
}

/// The grid of one band: a line at each tick of its scale, and at each label of the time axis.
fn grid(cx: &Ctx<'_>, plan: &Plan, band: Band, map: &PriceMap, ticks: &[f64], out: &mut Vec<Cmd>) {
    let color = hsla(cx.f.palette.grid);
    if plan.grid_horizontal {
        for value in ticks {
            out.push(cx.hline(
                cx.y_on(map, *value),
                cx.ox,
                cx.ox + f32_of(cx.plot_w),
                color,
            ));
        }
    }
    if plan.grid_vertical {
        for label in &plan.time_labels {
            out.push(cx.vline(
                cx.x(label.index),
                cx.oy + f32_of(band.top),
                cx.oy + f32_of(band.bottom()),
                color,
            ));
        }
    }
}

/// The text written faint behind the prices, when the chart has a watermark.
fn watermark(cx: &Ctx<'_>) -> Option<Cmd> {
    let text = cx.f.watermark.as_ref()?;
    let size = watermark_size();
    Some(Cmd::Text {
        text: text.clone(),
        x: cx.ox + f32_of(cx.plot_w) / 2.0,
        y: cx.oy + f32_of(cx.band.top + cx.band.h / 2.0) - size * 0.65,
        size,
        color: with_alpha(cx.f.palette.text_strong, 0.07),
        align: Align::Center,
        bold: true,
        face: Face::default(),
    })
}

/// The horizontal lines the chart is told to show: an order, a position, an alert.
fn marks(cx: &Ctx<'_>, out: &mut Vec<Cmd>) {
    for mark in cx.f.marks {
        let y = cx.snap(cx.y(mark.price)) + 0.5;
        let x0 = cx.ox + mark.from_x.unwrap_or(0.0).max(0.0);
        out.push(Cmd::Stroke {
            points: vec![(x0, y), (cx.ox + f32_of(cx.plot_w), y)],
            width: mark.width,
            color: rgb_alpha(mark.color, 0.9),
            dash: axes::dash_pattern(mark.dash, mark.width),
        });
    }
}
