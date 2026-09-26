//! Drawing the footprint: each bar as a candle with, next to it, what traded at every price.
//!
//! The bars are cut into rows of a height picked for the zoom (or the user's), and the flow of
//! each bar is analysed once per frame ([`model::analyze`]). Then, bar by bar and from the back
//! to the front: the value area behind, the heat of each cell, the point of control, the
//! candle, the numbers, the outlines of the imbalances and the stacks of them.
//!
//! Bars narrower than [`DETAIL_MIN_PX`] cannot hold numbers and are plain candles; bars the flow
//! has not loaded yet are candles too, so a bar is never missing.

use super::color::{Hsla, Rgba, rgb, transparent_black};
use wyck_openapi_model::market::Bar;

use super::super::flow::Flow;
use super::super::footprint::{self as model, Analysis, CellMode, HeatScope};
use super::cmd::{Align, Cmd, FONT, hsla, with_alpha};
use super::{Ctx, quote_unit, series};

/// Bars narrower than this are drawn as plain candles.
const DETAIL_MIN_PX: f64 = 24.0;
/// The height of the strip under the prices that holds each bar's delta and volume.
pub const SUMMARY_H: f32 = 34.0;
/// The color of the point of control and the value area.
const POC: u32 = 0xf5b942;
/// How many bars a stack or a point of control is followed to the right before giving up on it
/// being touched.
const FOLLOW_BARS: usize = 2_000;
/// The width of the candle beside the cells.
const BODY_W: f32 = 3.0;

/// How much room the strip under the prices takes, in pixels (none when it is off).
pub fn reserved(settings: &super::ChartSettings) -> f32 {
    if settings.kind == super::ChartKind::Footprint && settings.footprint.summary {
        SUMMARY_H
    } else {
        0.0
    }
}

/// The strength of the color of a cell holding `value` when the busiest holds `max`.
fn heat(value: u64, max: u64) -> f32 {
    if value == 0 || max == 0 {
        return 0.0;
    }
    let share = (value as f32 / max as f32).clamp(0.0, 1.0);
    0.07 + 0.5 * share.powf(0.75)
}

/// Draws the footprint of the bars `first..last`.
pub(super) fn draw(cx: &Ctx<'_>, bars: &[Bar], first: usize, last: usize, out: &mut Vec<Cmd>) {
    let last = last.min(bars.len());
    let bar_px = cx.f.view.bar_px;
    let flow: &Flow = match cx.f.flow {
        Some(flow) if bar_px >= DETAIL_MIN_PX && model::supports(cx.f.timeframe) => flow,
        _ => return series::solid_candles(cx, bars, first, last, out),
    };
    let s = &cx.f.settings.footprint;
    let p = cx.f.palette;
    let (up, down) = (p.up, p.down);

    // The height of a row: what a number needs, or what the user chose.
    let unit = quote_unit(cx.f.digits).max(1.0);
    let middle = (cx.map.lo + cx.map.hi) / 2.0;
    let unit_px = f64::from((cx.y(middle) - cx.y(middle + unit)).abs());
    let want_px = if s.numbers {
        12.0
    } else {
        2.0 * model::MIN_ROW_PX
    };
    let size = model::row_size(unit as i64, s.row_steps, unit_px, want_px);
    let half = unit / 2.0;

    let analyses: Vec<Option<Analysis>> = (first..last)
        .map(|i| {
            flow.bar(bars[i].time_ms)
                .and_then(|b| model::analyze(&b.levels, size, s))
        })
        .collect();
    // The busiest cell of the screen, for the heat that compares bars with each other.
    let cell_max = |a: &Analysis| match s.mode {
        CellMode::BidAsk => u64::from(a.max_side),
        CellMode::Delta => a.max_delta,
        CellMode::Volume => a.max_volume,
    };
    let visible_max = analyses
        .iter()
        .flatten()
        .map(cell_max)
        .max()
        .unwrap_or(1)
        .max(1);

    let (top, bottom) = (cx.oy + cx.band.top as f32, cx.oy + cx.band.bottom() as f32);
    let plot_right = cx.ox + cx.plot_w as f32;
    // Where a row is, from its top to its bottom: a price sits in the middle of its cell.
    let row_y = |index: i64| -> (f32, f32) {
        let low = (index * size) as f64 - half;
        let (a, b) = (cx.y(low + size as f64), cx.y(low));
        (a.min(b), a.max(b))
    };
    // The rows from one to another, from the top of the higher to the bottom of the lower.
    let span_y = |low_row: i64, high_row: i64| -> (f32, f32) {
        let (a0, a1) = row_y(low_row);
        let (b0, b1) = row_y(high_row);
        (a0.min(b0), a1.max(b1))
    };
    let on_screen = |(y0, y1): (f32, f32)| y1 >= top && y0 <= bottom;
    let bidask = s.mode == CellMode::BidAsk;
    let w = (bar_px * 0.94).min(bar_px - 2.0).max(8.0) as f32;
    let text_y = |center: f32, size: f32| center - size * 1.3 / 2.0;

    for (k, i) in (first..last).enumerate() {
        let bar = &bars[i];
        let x = cx.x(i);
        let (x0, x1) = (x - w / 2.0, x + w / 2.0);
        // The candle is in the middle when there are two columns, at the left when there is one.
        let body_x = if bidask { x } else { x0 + 2.0 };
        let (left, right) = if bidask {
            ((x0, x - BODY_W / 2.0 - 1.0), (x + BODY_W / 2.0 + 1.0, x1))
        } else {
            ((x0 + 5.0, x1), (x0 + 5.0, x1))
        };
        let Some(a) = &analyses[k] else {
            candle_strip(cx, bar, body_x, out);
            continue;
        };
        let norm = match s.heat_scope {
            HeatScope::Bar => cell_max(a).max(1),
            HeatScope::Visible => visible_max,
        };

        if s.value_area {
            let (y_top, y_bottom) =
                span_y(a.rows[a.value_area.0].index, a.rows[a.value_area.1].index);
            if on_screen((y_top, y_bottom)) {
                out.push(cx.rect(
                    x0,
                    y_top,
                    x1 - x0,
                    y_bottom - y_top,
                    with_alpha(rgb(POC), 0.07),
                ));
                out.push(cx.rect(
                    x0 - 2.0,
                    y_top,
                    1.5,
                    y_bottom - y_top,
                    with_alpha(rgb(POC), 0.7),
                ));
            }
        }

        // The size of the numbers: what the rows allow, and what the columns hold.
        let row_px = {
            let (y0, y1) = row_y(a.rows[0].index);
            y1 - y0
        };
        let longest = match s.mode {
            CellMode::BidAsk => model::compact(u64::from(a.max_side)).len(),
            CellMode::Delta => model::signed(a.max_delta as i64).len(),
            CellMode::Volume => model::compact(a.max_volume).len(),
        };
        let column = left.1 - left.0;
        let font = (row_px - 2.0)
            .min(FONT)
            .min((column - 4.0) / (longest as f32 * 0.6));
        let numbers = s.numbers && font >= 7.0;

        let mut texts: Vec<Cmd> = Vec::new();
        let mut outlines: Vec<Cmd> = Vec::new();
        for (r, row) in a.rows.iter().enumerate() {
            let (y0, y1) = row_y(row.index);
            if !on_screen((y0, y1)) {
                continue;
            }
            let gap = if y1 - y0 >= 6.0 { 1.0 } else { 0.0 };
            let (cy0, ch) = (y0 + gap / 2.0, (y1 - y0 - gap).max(0.5));
            let center = (y0 + y1) / 2.0;
            let buy_wins = a.buy_imbalance[r];
            let sell_wins = a.sell_imbalance[r];
            match s.mode {
                CellMode::BidAsk => {
                    if s.heat {
                        for (side, count, color) in [(left, row.sell, down), (right, row.buy, up)] {
                            let alpha = heat(u64::from(count), norm);
                            if alpha > 0.0 {
                                out.push(cx.rect(
                                    side.0,
                                    cy0,
                                    side.1 - side.0,
                                    ch,
                                    with_alpha(color, alpha),
                                ));
                            }
                        }
                    }
                    if numbers {
                        for (side, count, wins, color, align) in [
                            (left, row.sell, sell_wins, down, Align::Right),
                            (right, row.buy, buy_wins, up, Align::Left),
                        ] {
                            if count == 0 {
                                continue;
                            }
                            texts.push(Cmd::Text {
                                text: model::compact(u64::from(count)),
                                x: if align == Align::Right {
                                    side.1 - 3.0
                                } else {
                                    side.0 + 3.0
                                },
                                y: text_y(center, font),
                                size: font,
                                color: hsla(if wins { color } else { p.text_strong }),
                                align,
                                bold: wins,
                            });
                        }
                    }
                    for (side, wins, color) in [(left, sell_wins, down), (right, buy_wins, up)] {
                        if wins {
                            outlines.push(outline(side, cy0, ch, color));
                        }
                    }
                }
                CellMode::Delta | CellMode::Volume => {
                    let (value, color, alpha) = if s.mode == CellMode::Delta {
                        let d = row.delta();
                        (
                            model::signed(d),
                            if d >= 0 { up } else { down },
                            heat(d.unsigned_abs(), norm),
                        )
                    } else {
                        (
                            model::compact(row.volume()),
                            p.line,
                            heat(row.volume(), norm),
                        )
                    };
                    if s.heat && alpha > 0.0 {
                        out.push(cx.rect(
                            left.0,
                            cy0,
                            left.1 - left.0,
                            ch,
                            with_alpha(color, alpha),
                        ));
                    }
                    if numbers {
                        texts.push(Cmd::Text {
                            text: value,
                            x: (left.0 + left.1) / 2.0,
                            y: text_y(center, font),
                            size: font,
                            color: hsla(p.text_strong),
                            align: Align::Center,
                            bold: buy_wins || sell_wins,
                        });
                    }
                    for (wins, color) in [(buy_wins, up), (sell_wins, down)] {
                        if wins {
                            outlines.push(outline(left, cy0, ch, color));
                        }
                    }
                }
            }
        }

        if s.poc {
            let poc = &a.rows[a.poc];
            let (y0, y1) = row_y(poc.index);
            if on_screen((y0, y1)) {
                out.push(Cmd::Rect {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: (y1 - y0).max(1.0),
                    fill: transparent_black(),
                    border: Some((1.5, hsla_rgb(POC, 0.95))),
                    radius: 0.0,
                });
            }
        }
        candle_strip(cx, bar, body_x, out);
        out.extend(outlines);
        out.extend(texts);

        // Stacks of imbalances, and the point of control, carried to the right until the price
        // comes back to them.
        let follow = |low: i64, high: i64| -> f32 {
            let after = bars.get(i + 1..).unwrap_or_default();
            let touched = model::first_touch(
                after.iter().take(FOLLOW_BARS).map(|b| (b.low, b.high)),
                low,
                high,
            );
            touched.map_or(plot_right, |offset| cx.x(i + 1 + offset))
        };
        for stack in &a.stacks {
            let (y_top, y_bottom) = span_y(a.rows[stack.first].index, a.rows[stack.last].index);
            if !on_screen((y_top, y_bottom)) {
                continue;
            }
            let color = if stack.buy { up } else { down };
            let edge = if stack.buy { x1 } else { x0 - 3.0 };
            out.push(cx.rect(edge, y_top, 3.0, y_bottom - y_top, with_alpha(color, 0.9)));
            if s.project_stacks {
                let low = a.rows[stack.first].index * size;
                let high = (a.rows[stack.last].index + 1) * size - 1;
                let end = follow(low, high);
                if end > x1 {
                    out.push(cx.rect(
                        x1,
                        y_top,
                        end - x1,
                        y_bottom - y_top,
                        with_alpha(color, 0.1),
                    ));
                    out.push(cx.hline(y_top, x1, end, with_alpha(color, 0.45)));
                    out.push(cx.hline(y_bottom, x1, end, with_alpha(color, 0.45)));
                }
            }
        }
        if s.extend_poc && s.poc {
            let poc = &a.rows[a.poc];
            let (y0, y1) = row_y(poc.index);
            let end = follow(poc.index * size, (poc.index + 1) * size - 1);
            if end > x1 && on_screen((y0, y1)) {
                let y = (y0 + y1) / 2.0;
                out.push(Cmd::Stroke {
                    points: vec![(x1, cx.snap(y) + 0.5), (end, cx.snap(y) + 0.5)],
                    width: 1.0,
                    color: hsla_rgb(POC, 0.8),
                    dash: Some([4.0, 3.0]),
                });
            }
        }
    }

    if s.summary {
        summary(cx, &analyses, first, w, out);
    }
}

fn hsla_rgb(color: u32, alpha: f32) -> Hsla {
    with_alpha(rgb(color), alpha)
}

/// The border around a half cell that won an imbalance.
fn outline((x0, x1): (f32, f32), y: f32, h: f32, color: Rgba) -> Cmd {
    Cmd::Rect {
        x: x0,
        y,
        w: x1 - x0,
        h,
        fill: transparent_black(),
        border: Some((1.0, with_alpha(color, 0.95))),
        radius: 0.0,
    }
}

/// The candle of a bar, thin, beside or between its cells.
fn candle_strip(cx: &Ctx<'_>, bar: &Bar, x: f32, out: &mut Vec<Cmd>) {
    let color = hsla(cx.up_color(bar.close >= bar.open));
    let (y_high, y_low) = (cx.y(bar.high as f64), cx.y(bar.low as f64));
    let wick = 1.0 / cx.f.scale;
    out.push(cx.vline(x, y_high.min(y_low), y_low.max(y_high) + wick, color));
    let (y_open, y_close) = (cx.y(bar.open as f64), cx.y(bar.close as f64));
    let top = cx.snap(y_open.min(y_close));
    let height = (cx.snap(y_open.max(y_close)) - top).max(1.0 / cx.f.scale);
    out.push(cx.rect(x - BODY_W / 2.0, top, BODY_W, height, color));
}

/// The strip under the prices: the delta and the volume of each bar.
fn summary(
    cx: &Ctx<'_>,
    analyses: &[Option<Analysis>],
    first: usize,
    width: f32,
    out: &mut Vec<Cmd>,
) {
    let p = cx.f.palette;
    let (left, right) = (cx.ox, cx.ox + cx.plot_w as f32);
    let y = cx.oy + cx.band.bottom() as f32 - SUMMARY_H;
    out.push(cx.rect(left, y, right - left, SUMMARY_H, with_alpha(p.bg, 0.94)));
    out.push(cx.hline(y, left, right, hsla(p.border)));
    let (row_1, row_2) = (y + 4.0, y + 4.0 + FONT * 1.3 + 3.0);
    let label = |text: &str, y: f32| Cmd::Text {
        text: text.to_owned(),
        x: left + 6.0,
        y,
        size: FONT,
        color: hsla(p.text),
        align: Align::Left,
        bold: false,
    };
    for (k, analysis) in analyses.iter().enumerate() {
        let Some(a) = analysis else { continue };
        let x = cx.x(first + k);
        if x < left + 46.0 || x > right {
            continue;
        }
        let delta = a.delta();
        let mut push = |text: String, y: f32, color: Rgba, bold: bool| {
            // Kept to the width of the bar, and dropped when it does not fit.
            if text.len() as f32 * FONT * 0.6 <= width + 8.0 {
                out.push(Cmd::Text {
                    text,
                    x,
                    y,
                    size: FONT,
                    color: hsla(color),
                    align: Align::Center,
                    bold,
                });
            }
        };
        push(
            model::signed(delta),
            row_1,
            if delta >= 0 { p.up } else { p.down },
            true,
        );
        push(model::compact(a.volume()), row_2, p.text_strong, false);
    }
    out.push(label("Delta", row_1));
    out.push(label("Vol", row_2));
}
