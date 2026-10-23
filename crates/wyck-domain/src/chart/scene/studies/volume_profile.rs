//! The volume profile indicator: the histogram of volume by price, drawn beside the bars.

use super::dash_pattern;
use crate::base::math::{f32_of, f64_to_usize, i64_to_f64};
use crate::chart::data::Series;
use crate::chart::scene::cmd::{Cmd, rgb_alpha};
use crate::chart::scene::ctx::Ctx;
use crate::indicators::config::StudyConfig;
use crate::indicators::profile;
use crate::indicators::profile::Slice;

/// The volume profile of the bars on screen, as horizontal rows along one side of the plot.
pub(super) fn volume_profile(
    cx: &Ctx<'_>,
    config: &StudyConfig,
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    let slices: Vec<Slice> = match cx.series {
        Series::Bars(bars) => bars
            .get(first.min(bars.len())..last.min(bars.len()))
            .unwrap_or_default()
            .iter()
            .map(|b| Slice {
                open: i64_to_f64(b.open),
                high: i64_to_f64(b.high),
                low: i64_to_f64(b.low),
                close: i64_to_f64(b.close),
                volume: i64_to_f64(b.volume),
            })
            .collect(),
        Series::Ticks(ticks) => {
            let slice = ticks
                .get(first.min(ticks.len())..last.min(ticks.len()))
                .unwrap_or_default();
            slice
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let previous = i.checked_sub(1).and_then(|p| slice.get(p));
                    let before = i64_to_f64(previous.map_or(t.price, |prev| prev.price));
                    Slice {
                        open: before,
                        high: before.max(i64_to_f64(t.price)),
                        low: before.min(i64_to_f64(t.price)),
                        close: i64_to_f64(t.price),
                        volume: 1.0,
                    }
                })
                .collect()
        }
    };
    let rows = f64_to_usize(config.input("rows"));
    let Some(profile) = profile::build(&slices, rows, config.input("value_area") / 100.0) else {
        return;
    };
    let max_w = f32_of(cx.plot_w) * (f32_of(config.input("width")) / 100.0);
    let right_side = config.input("side") == 0.0;
    let highlight = config.input("highlight") != 0.0;
    let (up_style, down_style, poc_style) = (
        config.plot_style("up"),
        config.plot_style("down"),
        config.plot_style("poc"),
    );
    let edge = if right_side {
        cx.ox + f32_of(cx.plot_w)
    } else {
        cx.ox
    };
    for (index, row) in profile.rows.iter().enumerate() {
        let total = row.total();
        if total <= 0.0 {
            continue;
        }
        let in_area = (profile.value_area.0..=profile.value_area.1).contains(&index);
        let alpha = if in_area && highlight {
            f32_of(config.input("area_opacity")) / 100.0
        } else if highlight {
            f32_of(config.input("other_opacity")) / 100.0
        } else {
            f32_of(config.input("normal_opacity")) / 100.0
        };
        let (y_top, y_bottom) = (cx.y(row.hi), cx.y(row.lo));
        let height = (y_bottom - y_top).abs() - 1.0;
        if height <= 0.0 {
            continue;
        }
        let top = y_top.min(y_bottom) + 0.5;
        let width = f32_of(total / profile.max_total) * max_w;
        let up_w = f32_of(row.up / total) * width;
        let parts = [
            (up_w, up_style.color, up_style.visible, up_style.opacity),
            (
                width - up_w,
                down_style.color,
                down_style.visible,
                down_style.opacity,
            ),
        ];
        let mut offset = 0.0;
        for (w, color, visible, opacity) in parts {
            if visible && w > 0.0 {
                let x = if right_side {
                    edge - offset - w
                } else {
                    edge + offset
                };
                out.push(Cmd::Rect {
                    x,
                    y: top,
                    w,
                    h: height,
                    fill: rgb_alpha(color, alpha * opacity),
                    border: None,
                    radius: 0.0,
                });
            }
            offset += w;
        }
    }
    if poc_style.visible
        && let Some(row) = profile.rows.get(profile.poc)
    {
        let y = cx.snap(cx.y(f64::midpoint(row.lo, row.hi))) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(cx.ox, y), (cx.ox + f32_of(cx.plot_w), y)],
            width: poc_style.width,
            color: rgb_alpha(poc_style.color, poc_style.opacity),
            dash: dash_pattern(poc_style.dash),
        });
    }
}
