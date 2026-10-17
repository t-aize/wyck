//! The axes, the price lines and the tags on them.

use super::bands::PaneScale;
use super::color::{Hsla, Rgba};
use super::{Align, Band, Cmd, Ctx, Frame, Geometry, P, axis_w, font, hsla, rgb_alpha, with_alpha};
use crate::axis;
use crate::data::Series;
use wyck_core::math::{count_f64, f32_of, f64_to_usize, i64_to_f64};
use wyck_drawings::look::Face;
use wyck_drawings::model::style::Dash;
use wyck_indicators::spec::ValueFormat;
use wyck_market::Timeframe;

/// The solid strips of the two axes and the lines that set them off.
pub(super) fn axes_background(frame: &Frame<'_>, geometry: &Geometry) -> Cmd {
    let (ox, oy) = frame.origin;
    let plot_w = f32_of(geometry.plot_w());
    let plot_h = f32_of(geometry.plot_h());
    let (w, h) = (f32_of(frame.w), f32_of(frame.h));
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
pub(crate) fn dash_pattern(dash: Dash, width: f32) -> Option<[f32; 2]> {
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
pub(super) fn previous_close(series: &Series, zone: crate::zone::Zone) -> Option<i64> {
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
    let (x0, x1) = (cx.ox, cx.ox + f32_of(cx.plot_w));
    if lines.previous_close
        && let Some(close) = previous_close(cx.f.raw, cx.f.settings.zone)
    {
        let y = cx.snap(cx.y(i64_to_f64(close))) + 0.5;
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
                (x0, cx.snap(cx.y(i64_to_f64(ask))) + 0.5),
                (x1, cx.snap(cx.y(i64_to_f64(ask))) + 0.5),
            ],
            width: 1.0,
            color: with_alpha(p.text, 0.35),
            dash: Some([2.0, 3.0]),
        });
    }
    if let Some(price) = cx.f.raw.last_price().filter(|_| lines.last_line) {
        let c = with_alpha(cx.up_color(last_is_up(cx.f.raw)), 0.8);
        let y = cx.snap(cx.y(i64_to_f64(price))) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(x0, y), (x1, y)],
            width: 1.0,
            color: c,
            dash: Some([1.0, 2.0]),
        });
    }
}

/// `05:12`, `1:05:12` or `2d 03:04`: the time left before the bar closes.
pub(crate) fn countdown(ms: i64) -> String {
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

/// Draws the tags on the price axis (the order and position lines, the last price and the time
/// left in its bar) into `out`, and the pointer's price and time into `pointer_out`.
pub(super) fn draw_axis_tags(
    cx: &Ctx<'_>,
    pointer: Option<(P, usize)>,
    geometry: &Geometry,
    panes: &[PaneScale],
    out: &mut Vec<Cmd>,
    pointer_out: &mut Vec<Cmd>,
) {
    let p = cx.f.palette;
    let axis_x = cx.ox + f32_of(cx.plot_w) + 1.0;
    let main = geometry.main();
    let clamp_y = |band: &Band, y: f32| {
        let top = cx.oy + f32_of(band.top) + 9.0;
        let bottom = cx.oy + f32_of(band.bottom()) - 9.0;
        if top <= bottom {
            y.clamp(top, bottom)
        } else {
            f32::midpoint(top, bottom)
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
        face: Face::default(),
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
        let y = clamp_y(&main, cx.y(i64_to_f64(price)));
        let bg = hsla(cx.up_color(last_is_up(cx.f.raw)));
        let lines = cx.f.settings.price_lines;
        if lines.last_tag {
            out.push(tag(
                cx.map
                    .label(i64_to_f64(price), ValueFormat::Price, cx.f.digits),
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
                    face: Face::default(),
                });
            }
        }
    }
    let Some(((hx, hy), band_index)) = pointer else {
        return;
    };
    let Some(&band) = geometry.bands.get(band_index) else {
        return;
    };
    let y = clamp_y(&band, cx.oy + hy);
    let text = if !band.contains(f64::from(hy)) {
        None
    } else if band_index == 0 {
        Some(
            cx.map
                .label(cx.map.price(f64::from(hy)), ValueFormat::Price, cx.f.digits),
        )
    } else {
        panes
            .iter()
            .find(|pane| pane.band_index == band_index)
            .map(|pane| {
                super::price::format_value(pane.map.price(f64::from(hy)), pane.format, cx.f.digits)
            })
    };
    if let Some(text) = text {
        pointer_out.push(tag(text, y, hsla(p.tag), hsla(p.text_strong)));
    }
    if cx.len > 0 {
        let index = f64_to_usize(
            cx.f.view
                .index_at(f64::from(hx), cx.len, cx.plot_w)
                .round()
                .clamp(0.0, count_f64(cx.len - 1)),
        );
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
                y: cx.oy + f32_of(geometry.plot_h()) + 4.0,
                height: 20.0,
                pad: 8.0,
                bg: hsla(p.tag),
                fg: hsla(p.text_strong),
                align: Align::Center,
                fixed_width: None,
                within: Some((cx.ox, cx.ox + f32_of(cx.plot_w))),
                size: font(),
                bold: false,
                face: Face::default(),
            });
        }
    }
}

/// Keep enough room for a label while using the height available on each axis.
pub(super) fn axis_tick_target(height: f64) -> usize {
    f64_to_usize((height / 28.0).floor().clamp(2.0, 32.0))
}

/// Move axis tags apart within their own bands. Later tags have priority. Drop a lower priority
/// tag when its band cannot fit it without covering another one.
pub(super) fn spread_tags(tags: &mut Vec<Cmd>, geometry: &Geometry, oy: f32) {
    let mut placed: Vec<(f32, f32)> = Vec::new();
    for i in (0..tags.len()).rev() {
        let Some(Cmd::Tag {
            y,
            height,
            fixed_width: Some(_),
            ..
        }) = tags.get(i)
        else {
            continue;
        };
        let h = *height;
        let wanted = *y;
        let Some(band) = geometry.band_at(f64::from(wanted + h / 2.0 - oy)) else {
            tags.remove(i);
            continue;
        };
        let Some(&band) = geometry.bands.get(band) else {
            tags.remove(i);
            continue;
        };
        let top = oy + f32_of(band.top);
        let bottom = oy + f32_of(band.bottom());
        let fits = |at: f32| {
            at >= top && at + h <= bottom && placed.iter().all(|(t, b)| at + h <= *t || at >= *b)
        };
        let chosen = if fits(wanted) {
            Some(wanted)
        } else {
            (1..400).find_map(|step| {
                let d = f32_of(f64::from(step));
                [wanted - d, wanted + d].into_iter().find(|at| fits(*at))
            })
        };
        if let Some(chosen) = chosen {
            if let Some(Cmd::Tag { y, .. }) = tags.get_mut(i) {
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

#[cfg(test)]
mod tests {
    use super::previous_close;
    use super::*;

    use crate::scene::color::rgb;

    use crate::scene::*;

    use crate::zone::Zone;

    use wyck_market::Bar;

    #[test]
    fn the_time_left_in_a_bar_reads_like_a_clock() {
        assert_eq!(countdown(65_000), "01:05");
        assert_eq!(countdown(3_725_000), "1:02:05");
        assert_eq!(countdown(2 * 86_400_000 + 3_600_000), "2d 01:00");
        assert_eq!(countdown(-5), "00:00");
    }

    #[test]
    fn tags_on_the_axis_are_moved_apart_the_later_keeping_their_place() {
        let tag = |y: f32| Cmd::Tag {
            text: String::new(),
            x: 0.0,
            y,
            height: 18.0,
            pad: 0.0,
            bg: hsla(rgb(0)),
            fg: hsla(rgb(0)),
            align: Align::Left,
            fixed_width: Some(10.0),
            within: None,
            size: font(),
            bold: false,
            face: Face::default(),
        };
        let mut tags = vec![tag(100.0), tag(105.0), tag(104.0)];
        spread_tags(&mut tags, &Geometry::single(1_000.0, 528.0), 0.0);
        let ys: Vec<f32> = tags
            .iter()
            .map(|t| match t {
                Cmd::Tag { y, .. } => *y,
                _ => 0.0,
            })
            .collect();
        assert_eq!(ys[2], 104.0, "the last one stays");
        let mut sorted = ys.clone();
        sorted.sort_by(f32::total_cmp);
        assert!(sorted.windows(2).all(|w| w[1] - w[0] >= 18.0), "{ys:?}");
    }

    #[test]
    fn price_axis_uses_the_available_vertical_space() {
        let target = axis_tick_target(300.0);
        let ticks = axis::price_ticks(30_580.0, 30_660.0, target, 1.0);
        assert!(ticks.len() >= 8, "{ticks:?}");
    }

    #[test]
    fn the_previous_close_is_the_last_close_of_the_day_before() {
        let day = 86_400_000;
        let bar = |time_ms, close| Bar {
            time_ms,
            open: close,
            high: close,
            low: close,
            close,
            volume: 1,
        };
        let series = Series::Bars(vec![
            bar(day, 10),
            bar(day + 3_600_000, 11),
            bar(2 * day, 20),
            bar(2 * day + 3_600_000, 21),
        ]);
        assert_eq!(previous_close(&series, Zone::Utc), Some(11));
        assert_eq!(
            previous_close(&Series::Bars(vec![bar(day, 10)]), Zone::Utc),
            None
        );
        assert_eq!(previous_close(&Series::Ticks(Vec::new()), Zone::Utc), None);
    }
}
