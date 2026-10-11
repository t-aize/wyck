//! The axes, the price lines and the tags on them.

use super::color::{Hsla, Rgba};
use super::{
    Align, Band, Cmd, Ctx, Frame, Geometry, P, PriceMap, axis_w, font, hsla, rgb_alpha, with_alpha,
};
use crate::domain::chart::axis;
use crate::domain::chart::data::Series;
use crate::domain::chart::timeframe::Timeframe;
use crate::domain::drawings::model::Dash;
use crate::domain::indicators::ValueFormat;

/// The solid strips of the two axes and the lines that set them off.
pub(super) fn axes_background(frame: &Frame<'_>, geometry: &Geometry) -> Cmd {
    let (ox, oy) = frame.origin;
    let plot_w = geometry.plot_w() as f32;
    let plot_h = geometry.plot_h() as f32;
    let (w, h) = (frame.w as f32, frame.h as f32);
    let p = frame.palette;
    let one = 1.0 / frame.scale;
    let rect = |x: f32, y: f32, rw: f32, rh: f32, c: Hsla| Cmd::Rect {
        x,
        y,
        w: rw.max(0.0),
        h: rh.max(0.0),
        fill: c,
        border: None,
        radius: 0.0,
    };
    Cmd::Clip {
        x: ox,
        y: oy,
        w,
        h,
        inner: vec![
            rect(ox + plot_w, oy, w - plot_w, h, hsla(p.bg)),
            rect(ox, oy + plot_h, plot_w, h - plot_h, hsla(p.bg)),
            rect(ox + plot_w, oy, one, h, hsla(p.border)),
            rect(ox, oy + plot_h, w, one, hsla(p.border)),
        ],
    }
}

/// The dash pattern of a line style, for a line `width` wide.
pub fn dash_pattern(dash: Dash, width: f32) -> Option<[f32; 2]> {
    match dash {
        Dash::Solid => None,
        Dash::Dashed => Some([6.0 + width * 1.5, 4.0 + width]),
        Dash::Dotted => Some([width.max(1.0), 3.0 + width]),
    }
}

/// Whether the newest price moved up (or is unchanged) from the one before it.
pub(super) fn last_is_up(series: &Series) -> bool {
    match series {
        Series::Bars(bars) => bars.last().is_none_or(|b| b.close >= b.open),
        Series::Ticks(ticks) => match ticks.as_slice() {
            [.., a, b] => b.price >= a.price,
            _ => true,
        },
    }
}

/// The close of the last bar of the day before the newest bar's, in raw units, when the prices
/// are bars.
pub(super) fn previous_close(
    series: &Series,
    zone: crate::domain::chart::zone::Zone,
) -> Option<i64> {
    let Series::Bars(bars) = series else {
        return None;
    };
    let day = zone.day(bars.last()?.time_ms);
    bars.iter()
        .rev()
        .find(|bar| zone.day(bar.time_ms) != day)
        .map(|bar| bar.close)
}

/// The indexes in `first..last` of the points that start a new day, when the chart's points are
/// periods shorter than a day.
pub(super) fn day_breaks(frame: &Frame<'_>, first: usize, last: usize) -> Vec<usize> {
    let intraday = frame
        .timeframe
        .bar_ms()
        .is_some_and(|ms| ms < 86_400_000 && frame.settings.kind.keeps_time());
    if !intraday {
        return Vec::new();
    }
    let series = frame.series();
    let zone = frame.settings.zone;
    (first.max(1)..last)
        .filter(|&i| match (series.time_at(i - 1), series.time_at(i)) {
            (Some(before), Some(now)) => zone.day(before) != zone.day(now),
            _ => false,
        })
        .collect()
}

pub(super) fn draw_price_lines(cx: &Ctx<'_>, out: &mut Vec<Cmd>) {
    let p = cx.f.palette;
    let lines = cx.f.settings.price_lines;
    let (x0, x1) = (cx.ox, cx.ox + cx.plot_w as f32);
    if lines.previous_close
        && let Some(close) = previous_close(cx.f.raw, cx.f.settings.zone)
    {
        let y = cx.snap(cx.y(close as f64)) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(x0, y), (x1, y)],
            width: 1.0,
            color: with_alpha(p.text, 0.55),
            dash: Some([6.0, 4.0]),
        });
    }
    if let Some(ask) = cx.f.ask.filter(|_| lines.ask_line) {
        out.push(Cmd::Stroke {
            points: vec![
                (x0, cx.snap(cx.y(ask as f64)) + 0.5),
                (x1, cx.snap(cx.y(ask as f64)) + 0.5),
            ],
            width: 1.0,
            color: with_alpha(p.text, 0.35),
            dash: Some([2.0, 3.0]),
        });
    }
    if let Some(price) = cx.f.raw.last_price().filter(|_| lines.last_line) {
        let c = with_alpha(cx.up_color(last_is_up(cx.f.raw)), 0.8);
        let y = cx.snap(cx.y(price as f64)) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(x0, y), (x1, y)],
            width: 1.0,
            color: c,
            dash: Some([1.0, 2.0]),
        });
    }
}

/// `05:12`, `1:05:12` or `2d 03:04`: the time left before the bar closes.
pub fn countdown(ms: i64) -> String {
    let seconds = ms.max(0) / 1_000;
    let (days, hours, minutes, secs) = (
        seconds / 86_400,
        seconds % 86_400 / 3_600,
        seconds % 3_600 / 60,
        seconds % 60,
    );
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}")
    } else if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

/// The tags on the axes: the newest price with the time left in its bar, the marks, and the
/// crosshair's value and time.
/// Draws the tags on the price axis (the order and position lines, the last price and the time
/// left in its bar) into `out`, and the pointer's price and time into `pointer_out`.
pub(super) fn draw_axis_tags(
    cx: &Ctx<'_>,
    pointer: Option<(P, usize)>,
    geometry: &Geometry,
    panes: &[(Band, PriceMap, ValueFormat, Vec<f64>)],
    out: &mut Vec<Cmd>,
    pointer_out: &mut Vec<Cmd>,
) {
    let p = cx.f.palette;
    let axis_x = cx.ox + cx.plot_w as f32 + 1.0;
    let main = geometry.main();
    let clamp_y = |band: &Band, y: f32| {
        let top = cx.oy + band.top as f32 + 9.0;
        let bottom = cx.oy + band.bottom() as f32 - 9.0;
        if top <= bottom {
            y.clamp(top, bottom)
        } else {
            (top + bottom) / 2.0
        }
    };
    let tag = |text: String, y: f32, bg: Hsla, fg: Hsla| Cmd::Tag {
        text,
        x: axis_x,
        y: y - 9.0,
        height: 18.0,
        pad: 7.0,
        bg,
        fg,
        align: Align::Left,
        fixed_width: Some(axis_w() - 2.0),
        within: None,
        size: font(),
        bold: false,
        face: Default::default(),
    };
    for mark in cx.f.marks.iter().filter(|m| m.axis_tag) {
        let y = cx.y(mark.price);
        if main.contains(f64::from(y - cx.oy)) {
            out.push(tag(
                cx.map.label(mark.price, ValueFormat::Price, cx.f.digits),
                y,
                rgb_alpha(mark.color, 1.0),
                hsla(p.bg),
            ));
        }
    }
    if let Some(price) = cx.f.raw.last_price() {
        let y = clamp_y(&main, cx.y(price as f64));
        let bg = hsla(cx.up_color(last_is_up(cx.f.raw)));
        let lines = cx.f.settings.price_lines;
        if lines.last_tag {
            out.push(tag(
                cx.map.label(price as f64, ValueFormat::Price, cx.f.digits),
                y,
                bg,
                hsla(p.bg),
            ));
        }
        // The time left in the bar, under the price, for bars that are periods of time.
        if let (Some(bar_ms), Some(last)) = (cx.f.timeframe.bar_ms(), cx.f.raw.last_time())
            && lines.countdown
            && cx.f.settings.kind.keeps_time()
            && matches!(cx.f.raw, Series::Bars(_))
        {
            let left = last + bar_ms - cx.f.now_ms;
            if left > 0 && left <= bar_ms {
                out.push(Cmd::Tag {
                    text: countdown(left),
                    x: axis_x,
                    // Under the tag of the price, or where it would be without one.
                    y: if lines.last_tag { y + 9.0 } else { y - 8.0 },
                    height: 16.0,
                    pad: 7.0,
                    bg: shade(cx.up_color(last_is_up(cx.f.raw)), 0.72),
                    fg: hsla(p.bg),
                    align: Align::Left,
                    fixed_width: Some(axis_w() - 2.0),
                    within: None,
                    size: font(),
                    bold: false,
                    face: Default::default(),
                });
            }
        }
    }
    let Some(((hx, hy), band_index)) = pointer else {
        return;
    };
    let band = geometry.bands[band_index];
    let y = clamp_y(&band, cx.oy + hy);
    let text = if !band.contains(f64::from(hy)) {
        None
    } else if band_index == 0 {
        Some(
            cx.map
                .label(cx.map.price(f64::from(hy)), ValueFormat::Price, cx.f.digits),
        )
    } else {
        panes.get(band_index - 1).map(|(_, map, format, _)| {
            super::price::format_value(map.price(f64::from(hy)), *format, cx.f.digits)
        })
    };
    if let Some(text) = text {
        pointer_out.push(tag(text, y, hsla(p.tag), hsla(p.text_strong)));
    }
    if cx.len > 0 {
        let index =
            cx.f.view
                .index_at(f64::from(hx), cx.len, cx.plot_w)
                .round()
                .clamp(0.0, (cx.len - 1) as f64) as usize;
        if let Some(time) = cx.series.time_at(index) {
            let text = axis::full_time(
                time,
                cx.f.settings.zone,
                matches!(cx.f.timeframe, Timeframe::Seconds(_) | Timeframe::Ticks),
                matches!(cx.f.timeframe, Timeframe::Ticks),
            );
            pointer_out.push(Cmd::Tag {
                text,
                x: cx.x(index),
                y: cx.oy + geometry.plot_h() as f32 + 4.0,
                height: 20.0,
                pad: 8.0,
                bg: hsla(p.tag),
                fg: hsla(p.text_strong),
                align: Align::Center,
                fixed_width: None,
                within: Some((cx.ox, cx.ox + cx.plot_w as f32)),
                size: font(),
                bold: false,
                face: Default::default(),
            });
        }
    }
}

/// Keep enough room for a label while using the height available on each axis.
pub(super) fn axis_tick_target(height: f64) -> usize {
    (height / 28.0).floor().clamp(2.0, 32.0) as usize
}

/// Move axis tags apart within their own bands. Later tags have priority. Drop a lower priority
/// tag when its band cannot fit it without covering another one.
pub(super) fn spread_tags(tags: &mut Vec<Cmd>, geometry: &Geometry, oy: f32) {
    let mut placed: Vec<(f32, f32)> = Vec::new();
    for i in (0..tags.len()).rev() {
        let Cmd::Tag {
            y,
            height,
            fixed_width: Some(_),
            ..
        } = &tags[i]
        else {
            continue;
        };
        let h = *height;
        let wanted = *y;
        let Some(band) = geometry.band_at(f64::from(wanted + h / 2.0 - oy)) else {
            tags.remove(i);
            continue;
        };
        let band = geometry.bands[band];
        let top = oy + band.top as f32;
        let bottom = oy + band.bottom() as f32;
        let fits = |at: f32| {
            at >= top && at + h <= bottom && placed.iter().all(|(t, b)| at + h <= *t || at >= *b)
        };
        let chosen = if fits(wanted) {
            Some(wanted)
        } else {
            (1..400).find_map(|step| {
                let d = step as f32;
                [wanted - d, wanted + d].into_iter().find(|at| fits(*at))
            })
        };
        if let Some(chosen) = chosen {
            if let Cmd::Tag { y, .. } = &mut tags[i] {
                *y = chosen;
            }
            placed.push((chosen, chosen + h));
        } else {
            tags.remove(i);
        }
    }
}

/// A color darkened towards black by `amount` of its brightness kept, opaque.
pub(super) fn shade(color: Rgba, keep: f32) -> Hsla {
    hsla(Rgba {
        r: color.r * keep,
        g: color.g * keep,
        b: color.b * keep,
        a: 1.0,
    })
}
