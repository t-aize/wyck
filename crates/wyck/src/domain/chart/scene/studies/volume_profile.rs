//! The volume profile indicator: the histogram of volume by price, drawn beside the bars.

use super::super::cmd::{Cmd, rgb_alpha};
use super::super::ctx::Ctx;
use super::dash_pattern;
use crate::domain::chart::data::Series;
use wyck_indicators::config::StudyConfig;
use wyck_indicators::profile;
use wyck_indicators::profile::Slice;

/// The volume profile of the bars on screen, as horizontal rows along one side of the plot.
pub(super) fn volume_profile(
    cx: &Ctx<'_>,
    config: &StudyConfig,
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    let slices: Vec<Slice> = match cx.series {
        Series::Bars(bars) => bars[first.min(bars.len())..last.min(bars.len())]
            .iter()
            .map(|b| Slice {
                open: b.open as f64,
                high: b.high as f64,
                low: b.low as f64,
                close: b.close as f64,
                volume: b.volume as f64,
            })
            .collect(),
        Series::Ticks(ticks) => {
            let slice = &ticks[first.min(ticks.len())..last.min(ticks.len())];
            slice
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let before = if i > 0 { slice[i - 1].price } else { t.price } as f64;
                    Slice {
                        open: before,
                        high: before.max(t.price as f64),
                        low: before.min(t.price as f64),
                        close: t.price as f64,
                        volume: 1.0,
                    }
                })
                .collect()
        }
    };
    let rows = config.input("rows") as usize;
    let Some(profile) = profile::build(&slices, rows, config.input("value_area") / 100.0) else {
        return;
    };
    let max_w = cx.plot_w as f32 * (config.input("width") as f32 / 100.0);
    let right_side = config.input("side") == 0.0;
    let highlight = config.input("highlight") != 0.0;
    let (up_style, down_style, poc_style) = (
        config.plot_style("up"),
        config.plot_style("down"),
        config.plot_style("poc"),
    );
    let edge = if right_side {
        cx.ox + cx.plot_w as f32
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
            config.input("area_opacity") as f32 / 100.0
        } else if highlight {
            config.input("other_opacity") as f32 / 100.0
        } else {
            config.input("normal_opacity") as f32 / 100.0
        };
        let (y_top, y_bottom) = (cx.y(row.hi), cx.y(row.lo));
        let height = (y_bottom - y_top).abs() - 1.0;
        if height <= 0.0 {
            continue;
        }
        let top = y_top.min(y_bottom) + 0.5;
        let width = (total / profile.max_total) as f32 * max_w;
        let up_w = (row.up / total) as f32 * width;
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
    if poc_style.visible {
        let row = profile.rows[profile.poc];
        let y = cx.snap(cx.y((row.lo + row.hi) / 2.0)) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(cx.ox, y), (cx.ox + cx.plot_w as f32, y)],
            width: poc_style.width,
            color: rgb_alpha(poc_style.color, poc_style.opacity),
            dash: dash_pattern(poc_style.dash),
        });
    }
}
