//! What a bar looks like at the current zoom, and the pixel boxes to draw it with.
//!
//! A chart that shows fifty bars and one that shows fifty thousand cannot draw a bar the same way.
//! [`detail_for`] picks one of three levels from the bar spacing:
//!
//! - [`Detail::Candles`]: a body and a wick, the normal look.
//! - [`Detail::Lines`]: the bars are too narrow for a body, so each is a one pixel line from its
//!   low to its high, colored by direction.
//! - [`Detail::Columns`]: several bars share a pixel. The bars that fall on the same pixel are
//!   merged into one column (lowest low, highest high, direction of the first open to the last
//!   close), so the cost is bounded by the width of the screen, not by the number of bars.
//!
//! Boxes are snapped to whole pixels here, so wicks and edges stay crisp and a candle does not
//! shimmer as the chart is dragged. Everything is in plain numbers, so it is tested without a
//! window; the drawing code only fills the rectangles.

use wyck_engine::domain::Candle;

use super::viewport::Viewport;

/// Spacing from which bars are drawn with a body.
pub const CANDLE_MIN_SPACING: f64 = 3.0;

/// Spacing from which each bar still gets its own line; below it, bars are merged per pixel.
pub const LINE_MIN_SPACING: f64 = 1.0;

/// How a bar is drawn at the current zoom. See the [module docs](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// A body and a wick.
    Candles,
    /// A one pixel line from low to high.
    Lines,
    /// Bars merged into one column per pixel.
    Columns,
}

/// The level of detail for `spacing` pixels per bar.
#[must_use]
pub fn detail_for(spacing: f64) -> Detail {
    if spacing >= CANDLE_MIN_SPACING {
        Detail::Candles
    } else if spacing >= LINE_MIN_SPACING {
        Detail::Lines
    } else {
        Detail::Columns
    }
}

/// A rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

/// The two boxes of a candle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandleShape {
    /// The wick: one pixel wide, from the high to the low.
    pub wick: Rect,
    /// The body: from the open to the close, at least one pixel tall.
    pub body: Rect,
}

/// The widest body for `spacing`: about three quarters of it, odd so the wick sits in the middle,
/// and always leaving a gap to the next bar.
fn body_width(spacing: f64) -> f64 {
    let room = (spacing.floor() - 1.0).max(1.0);
    let mut width = (spacing * 0.75).floor().clamp(1.0, room);
    if (width as i64) % 2 == 0 {
        width = (width - 1.0).max(1.0);
    }
    width
}

/// The boxes of a candle centred at `x`, with the four prices already converted to pixel rows
/// (`y` grows downwards, so `y_high` is the smallest).
#[must_use]
pub fn candle_shape(
    x: f64,
    spacing: f64,
    y_open: f64,
    y_close: f64,
    y_high: f64,
    y_low: f64,
) -> CandleShape {
    let width = body_width(spacing);
    let centre = x.floor();
    let left = centre - (width - 1.0) / 2.0;

    let wick_top = y_high.min(y_low).round();
    let wick_bottom = y_high.max(y_low).round();
    let body_top = y_open.min(y_close).round();
    let body_bottom = y_open.max(y_close).round();

    CandleShape {
        wick: Rect {
            x: centre,
            y: wick_top,
            w: 1.0,
            h: (wick_bottom - wick_top).max(1.0),
        },
        body: Rect {
            x: left,
            y: body_top,
            w: width,
            h: (body_bottom - body_top).max(1.0),
        },
    }
}

/// The line of a bar at [`Detail::Lines`]: one pixel wide, from high to low.
#[must_use]
pub fn line_shape(x: f64, y_high: f64, y_low: f64) -> Rect {
    let top = y_high.min(y_low).round();
    Rect {
        x: x.floor(),
        y: top,
        w: 1.0,
        h: (y_high.max(y_low).round() - top).max(1.0),
    }
}

/// Bars merged into the pixel column they share, at [`Detail::Columns`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    /// The pixel column.
    pub x: f64,
    /// The lowest low of the bars in it.
    pub low: f64,
    /// The highest high of the bars in it.
    pub high: f64,
    /// Whether the last close is at or above the first open.
    pub up: bool,
}

/// The columns for the bars in `range` of `bars`, left to right. Bars are merged while they fall
/// on the same pixel, so the result has at most one column per pixel of width.
#[must_use]
pub fn columns(bars: &[Candle], range: std::ops::Range<usize>, viewport: &Viewport) -> Vec<Column> {
    let count = bars.len();
    let mut out: Vec<Column> = Vec::new();
    let mut first_open = 0.0;
    for index in range {
        let bar = &bars[index];
        let x = viewport.x_of(index as f64, count).floor();
        match out.last_mut() {
            Some(column) if column.x == x => {
                column.low = column.low.min(bar.low);
                column.high = column.high.max(bar.high);
                column.up = bar.close >= first_open;
            }
            _ => {
                first_open = bar.open;
                out.push(Column {
                    x,
                    low: bar.low,
                    high: bar.high,
                    up: bar.close >= bar.open,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            time: 0,
            open,
            high,
            low,
            close,
            volume: 0.0,
        }
    }

    #[test]
    fn the_detail_follows_the_spacing() {
        assert_eq!(detail_for(8.0), Detail::Candles);
        assert_eq!(detail_for(3.0), Detail::Candles);
        assert_eq!(detail_for(2.0), Detail::Lines);
        assert_eq!(detail_for(1.0), Detail::Lines);
        assert_eq!(detail_for(0.4), Detail::Columns);
    }

    #[test]
    fn a_body_is_odd_and_leaves_a_gap() {
        for spacing in [3.0, 4.0, 5.5, 8.0, 10.0, 13.0, 40.0, 80.0] {
            let w = body_width(spacing);
            assert!(w >= 1.0 && (w as i64) % 2 == 1, "{spacing}: {w}");
            assert!(w <= (spacing.floor() - 1.0).max(1.0), "{spacing}: {w}");
        }
    }

    #[test]
    fn the_wick_is_centred_on_the_body() {
        let shape = candle_shape(100.4, 8.0, 50.0, 70.0, 40.0, 80.0);
        assert_eq!(shape.wick.w, 1.0);
        assert_eq!(shape.wick.x, 100.0);
        assert_eq!(shape.body.x + (shape.body.w - 1.0) / 2.0, shape.wick.x);
        assert_eq!((shape.wick.y, shape.wick.h), (40.0, 40.0));
        assert_eq!((shape.body.y, shape.body.h), (50.0, 20.0));
    }

    #[test]
    fn a_flat_candle_is_still_visible() {
        let shape = candle_shape(10.0, 8.0, 50.0, 50.0, 50.0, 50.0);
        assert_eq!((shape.body.h, shape.wick.h), (1.0, 1.0));
    }

    #[test]
    fn open_above_close_gives_the_same_body() {
        let up = candle_shape(10.0, 8.0, 70.0, 50.0, 40.0, 80.0);
        let down = candle_shape(10.0, 8.0, 50.0, 70.0, 40.0, 80.0);
        assert_eq!(up, down);
    }

    #[test]
    fn a_line_spans_high_to_low() {
        let r = line_shape(12.7, 30.2, 60.6);
        assert_eq!((r.x, r.y, r.w, r.h), (12.0, 30.0, 1.0, 31.0));
    }

    #[test]
    fn narrow_bars_share_a_column() {
        let bars: Vec<Candle> = (0..100)
            .map(|i| bar(i as f64, i as f64 + 1.0, i as f64 - 1.0, i as f64 + 0.5))
            .collect();
        let view = Viewport {
            bar_spacing: 0.25,
            right_offset: 0.0,
            width: 200.0,
        };
        let cols = columns(&bars, view.visible(bars.len()), &view);
        assert!(
            cols.len() <= 26,
            "{} columns for 100 bars at 0.25px",
            cols.len()
        );
        assert!(cols.windows(2).all(|w| w[0].x < w[1].x));
        let lows = cols.iter().map(|c| c.low).fold(f64::MAX, f64::min);
        let highs = cols.iter().map(|c| c.high).fold(f64::MIN, f64::max);
        assert_eq!((lows, highs), (-1.0, 100.0), "no extreme is lost");
    }

    #[test]
    fn a_column_takes_its_direction_from_first_open_to_last_close() {
        let bars = [bar(10.0, 12.0, 9.0, 9.5), bar(9.5, 13.0, 9.0, 11.0)];
        let view = Viewport {
            bar_spacing: 0.1,
            right_offset: 0.0,
            width: 100.0,
        };
        let cols = columns(&bars, 0..2, &view);
        assert_eq!(cols.len(), 1);
        assert!(cols[0].up, "closed at 11 after opening at 10");
    }
}
