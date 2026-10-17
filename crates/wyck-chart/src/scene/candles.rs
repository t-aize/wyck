//! Candles, OHLC bars and bricks: the bars drawn one by one. Other chart types fall back to
//! plain candles when they cannot show their details (a footprint on bars too narrow to read).

use super::cmd::{Cmd, hsla, with_alpha};
use super::color::transparent_black;
use super::ctx::{Ctx, columns};
use wyck_core::math::{f32_of, i64_to_f64};
use wyck_market::Bar;

/// Bars narrower than this are drawn as one line each, not as a body with wicks.
pub(super) const CANDLE_MIN_PX: f64 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CandleStyle {
    Solid,
    Hollow,
    Ohlc,
    /// Bodies without wicks (bricks and lines).
    Body,
    /// Bricks with the wicks of the extremes the price saw while they formed.
    BodyWicks,
}

impl CandleStyle {
    /// Whether the body is a brick, drawn from the brick settings.
    pub(super) fn is_brick(self) -> bool {
        matches!(self, Self::Body | Self::BodyWicks)
    }
}

/// Plain candles, for what a chart type falls back to when it cannot show its details.
pub(super) fn solid_candles(
    cx: &Ctx<'_>,
    bars: &[Bar],
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    candles(cx, bars, first, last, CandleStyle::Solid, out);
}

pub(super) fn candles(
    cx: &Ctx<'_>,
    bars: &[Bar],
    first: usize,
    last: usize,
    style: CandleStyle,
    out: &mut Vec<Cmd>,
) {
    let bar_px = cx.f.view.bar_px;
    let last = last.min(bars.len());
    let wide = bar_px >= CANDLE_MIN_PX;
    let wick = 1.0 / cx.f.scale;
    if !wide {
        // One line per pixel column, from the lowest low to the highest high.
        columns(cx, first, last, |x, range| {
            let group = bars.get(range).unwrap_or_default();
            let (Some(oldest), Some(newest)) = (group.first(), group.last()) else {
                return;
            };
            let high = i64_to_f64(group.iter().map(|b| b.high).max().unwrap_or(0));
            let low = i64_to_f64(group.iter().map(|b| b.low).min().unwrap_or(0));
            let up = newest.close >= oldest.open;
            let c = hsla(cx.up_color(up));
            out.push(cx.vline(x, cx.y(high), cx.y(low).max(cx.y(high) + wick), c));
        });
        return;
    }
    let look = &cx.f.settings.transform;
    let body_w = if style.is_brick() {
        f32_of((bar_px * f64::from(look.brick_width)).max(3.0))
    } else {
        f32_of((bar_px * 0.72).max(3.0))
    };
    for (i, bar) in (first..last).zip(bars.iter().skip(first)) {
        let x = cx.x(i);
        // Hollow candles are colored by the change from the previous close, the others by the
        // change over the bar itself.
        let up = match style {
            CandleStyle::Hollow => {
                let previous = i
                    .checked_sub(1)
                    .and_then(|p| bars.get(p))
                    .map(|prev| prev.close);
                bar.close >= previous.unwrap_or(bar.open)
            }
            _ => bar.close >= bar.open,
        };
        let c = hsla(cx.up_color(up));
        let (y_high, y_low) = (cx.y(i64_to_f64(bar.high)), cx.y(i64_to_f64(bar.low)));
        let (y_open, y_close) = (cx.y(i64_to_f64(bar.open)), cx.y(i64_to_f64(bar.close)));
        if style != CandleStyle::Body {
            out.push(cx.vline(x, y_high.min(y_low), y_low.max(y_high) + wick, c));
        }
        if style == CandleStyle::Ohlc {
            let tick = (f32_of(bar_px) * 0.38).max(2.0);
            out.push(cx.hline(y_open, x - tick, x, c));
            out.push(cx.hline(y_close, x, x + tick, c));
            continue;
        }
        let (top, bottom) = (y_open.min(y_close), y_open.max(y_close));
        let x0 = cx.snap(x - body_w / 2.0);
        let x1 = cx.snap(x + body_w / 2.0).max(x0 + 1.0 / cx.f.scale);
        let top = cx.snap(top);
        let height = (cx.snap(bottom) - top).max(1.0 / cx.f.scale);
        if style == CandleStyle::Hollow && bar.close > bar.open {
            out.push(Cmd::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: height,
                fill: transparent_black(),
                border: Some(((1.0 / cx.f.scale).max(1.0), c)),
                radius: 0.0,
            });
        } else if style.is_brick() {
            out.push(Cmd::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: height,
                fill: with_alpha(cx.up_color(up), look.brick_opacity),
                border: look.brick_border.then_some((1.0, c)),
                radius: 0.0,
            });
        } else {
            out.push(Cmd::rect(x0, top, x1 - x0, height, c));
        }
    }
}
