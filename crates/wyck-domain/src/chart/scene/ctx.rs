//! The drawing context of the prices band: the frame, the scale and the pixel snapping.

use super::color::{Hsla, Rgba};
use super::{Band, Cmd, Frame, PriceMap};
use crate::base::math::{count_f64, f32_of};
use crate::chart::data::Series;

/// The drawing context of the prices band.
pub(super) struct Ctx<'a> {
    pub(super) f: &'a Frame<'a>,
    pub(super) series: &'a Series,
    pub(super) map: PriceMap,
    pub(super) len: usize,
    pub(super) plot_w: f64,
    pub(super) band: Band,
    pub(super) ox: f32,
    pub(super) oy: f32,
}

impl Ctx<'_> {
    pub(super) fn snap(&self, v: f32) -> f32 {
        (v * self.f.scale).round() / self.f.scale
    }

    /// Absolute x of point `index` (fractional, possibly past the data).
    pub(super) fn xf(&self, index: f64) -> f32 {
        self.ox + f32_of(self.f.view.x_of(index, self.len, self.plot_w))
    }

    pub(super) fn x(&self, index: usize) -> f32 {
        self.xf(count_f64(index))
    }

    /// Absolute y of a value on `map`, kept to a range a GPU can draw.
    pub(super) fn y_on(&self, map: &PriceMap, value: f64) -> f32 {
        self.oy + f32_of(map.y(value).clamp(-4_000.0, 20_000.0))
    }

    pub(super) fn y(&self, price: f64) -> f32 {
        self.y_on(&self.map, price)
    }

    /// A one device pixel wide vertical line from `y0` to `y1`.
    pub(super) fn vline(&self, x: f32, y0: f32, y1: f32, color: Hsla) -> Cmd {
        let w = 1.0 / self.f.scale;
        let (a, b) = (y0.min(y1), y0.max(y1));
        Cmd::rect(
            self.snap(x),
            self.snap(a),
            w,
            self.snap(b) - self.snap(a),
            color,
        )
    }

    pub(super) fn hline(&self, y: f32, x0: f32, x1: f32, c: Hsla) -> Cmd {
        let h = 1.0 / self.f.scale;
        Cmd::rect(
            self.snap(x0),
            self.snap(y),
            self.snap(x1) - self.snap(x0),
            h,
            c,
        )
    }

    pub(super) fn up_color(&self, up: bool) -> Rgba {
        if up {
            self.f.palette.up
        } else {
            self.f.palette.down
        }
    }
}

/// Calls `each` for every group of points that share a pixel column, with the column's center x.
/// Wider than a pixel, every point is its own group.
#[expect(
    clippy::float_cmp,
    reason = "both sides are `floor()` results, whole numbers, so the comparison is exact"
)]
pub(super) fn columns(
    cx: &Ctx<'_>,
    first: usize,
    last: usize,
    mut each: impl FnMut(f32, std::ops::Range<usize>),
) {
    let mut start = first;
    while start < last {
        let column = cx.x(start).floor();
        let mut end = start + 1;
        while end < last && cx.x(end).floor() == column {
            end += 1;
        }
        let x = if end - start == 1 {
            cx.x(start)
        } else {
            column + 0.5
        };
        each(x, start..end);
        start = end;
    }
}
