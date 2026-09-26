//! How a chart maps times and prices to screen positions, for the drawings.

use wyck_openapi_model::market::{PRICE_SCALE, format_price};

use super::data::Series;
use super::drawing::geometry::{BarView, P, Projection, Rect};
use super::drawing::model::Point;
use super::scene::PriceMap;
use super::view::View;

/// How close (in pixels) the pointer must be to a bar's open, high, low or close for the magnet
/// to take it.
const MAGNET_REACH: f64 = 18.0;

/// Most bars handed to a drawing tool that reads the data (regression, VWAP, volume).
const MAX_TOOL_BARS: usize = 50_000;

pub struct ChartProjection<'a> {
    pub series: &'a Series,
    pub view: &'a View,
    pub map: PriceMap,
    pub plot_w: f64,
    pub plot_h: f64,
    /// The time between two points, for placing times beyond the data.
    pub step_ms: f64,
    pub digits: u32,
    pub pip_position: Option<i64>,
}

impl ChartProjection<'_> {
    fn len(&self) -> usize {
        self.series.len()
    }

    /// The nearest open, high, low or close of the bar at `index`, when one is within reach of
    /// `price` on the screen.
    fn magnet_price(&self, index: usize, price: f64) -> f64 {
        let Series::Bars(bars) = self.series else {
            return price;
        };
        let Some(bar) = bars.get(index) else {
            return price;
        };
        let y = self.map.y(price);
        [bar.open, bar.high, bar.low, bar.close]
            .into_iter()
            .map(|value| value as f64)
            .map(|value| ((self.map.y(value) - y).abs(), value))
            .filter(|(distance, _)| *distance <= MAGNET_REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(price, |(_, value)| value)
    }
}

impl Projection for ChartProjection<'_> {
    fn plot(&self) -> Rect {
        Rect::new(self.plot_w as f32, self.plot_h as f32)
    }

    fn to_screen(&self, point: Point) -> Option<P> {
        let index = self.series.index_of_time(point.t, self.step_ms)?;
        let x = self.view.x_of(index, self.len(), self.plot_w);
        let y = self.map.y(point.p);
        (x.is_finite() && y.is_finite()).then_some((x as f32, y as f32))
    }

    fn point_at(&self, x: f32, y: f32, magnet: bool) -> Option<Point> {
        let len = self.len();
        if len == 0 {
            return None;
        }
        let index = self.view.index_at(f64::from(x), len, self.plot_w).round();
        let t = self.series.time_of_index(index, self.step_ms)?;
        let mut price = self.map.price(f64::from(y));
        if magnet && index >= 0.0 {
            price = self.magnet_price(index as usize, price);
        }
        price.is_finite().then_some(Point { t, p: price })
    }

    fn index_of(&self, time_ms: i64) -> Option<f64> {
        self.series.index_of_time(time_ms, self.step_ms)
    }

    fn shift_bars(&self, time_ms: i64, bars: f64) -> Option<i64> {
        let index = self.series.index_of_time(time_ms, self.step_ms)?;
        self.series.time_of_index(index + bars, self.step_ms)
    }

    fn format_price(&self, price: f64) -> String {
        format_price(price.round() as i64, self.digits)
    }

    fn price_span(&self) -> f64 {
        self.map.hi - self.map.lo
    }

    fn bars_between(&self, from_ms: i64, to_ms: i64) -> Vec<BarView> {
        let Series::Bars(bars) = self.series else {
            return Vec::new();
        };
        let start = bars.partition_point(|bar| bar.time_ms < from_ms);
        let end = bars.partition_point(|bar| bar.time_ms <= to_ms);
        let len = bars.len();
        bars[start..end.max(start)]
            .iter()
            .take(MAX_TOOL_BARS)
            .enumerate()
            .map(|(offset, bar)| BarView {
                time: bar.time_ms,
                x: self.view.x_of((start + offset) as f64, len, self.plot_w) as f32,
                open: bar.open as f64,
                high: bar.high as f64,
                low: bar.low as f64,
                close: bar.close as f64,
                volume: bar.volume as f64,
            })
            .collect()
    }

    fn y_of(&self, price: f64) -> f32 {
        self.map.y(price) as f32
    }

    fn real_price(&self, raw: f64) -> f64 {
        raw / PRICE_SCALE as f64
    }

    fn tick(&self) -> f64 {
        super::scene::quote_unit(self.digits) / PRICE_SCALE as f64
    }

    fn pip(&self) -> f64 {
        self.pip_position
            .filter(|position| (0..=12).contains(position))
            .map_or(0.0, |position| 10f64.powi(-(position as i32)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::Display;
    use crate::scene::{Geometry, main_map};
    use crate::settings::ChartSettings;
    use wyck_openapi_model::market::Bar;

    fn bars() -> Series {
        Series::Bars(
            (0..100)
                .map(|i| Bar {
                    time_ms: 1_767_571_200_000 + i * 300_000,
                    open: 100_000 + i,
                    high: 100_050 + i,
                    low: 99_950 + i,
                    close: 100_020 + i,
                    volume: 1,
                })
                .collect(),
        )
    }

    fn with<R>(f: impl FnOnce(&ChartProjection<'_>) -> R) -> R {
        let series = bars();
        let view = View::new(8.0);
        let settings = ChartSettings::default();
        let display = Display::build(&series, &settings, 1);
        let geometry = Geometry::single(1_000.0, 600.0);
        let map = main_map(&series, &display, &settings, &view, &geometry, 5).unwrap();
        let projection = ChartProjection {
            series: &series,
            view: &view,
            map,
            plot_w: geometry.plot_w(),
            plot_h: geometry.plot_h(),
            step_ms: 300_000.0,
            digits: 5,
            pip_position: Some(4),
        };
        f(&projection)
    }

    #[test]
    fn a_point_goes_to_the_screen_and_back() {
        with(|proj| {
            let point = Point {
                t: 1_767_571_200_000 + 50 * 300_000,
                p: 100_040.0,
            };
            let (x, y) = proj.to_screen(point).unwrap();
            let back = proj.point_at(x, y, false).unwrap();
            assert_eq!(back.t, point.t);
            assert!((back.p - point.p).abs() < 1.0, "{} vs {}", back.p, point.p);
        });
    }

    #[test]
    fn a_click_between_bars_snaps_to_the_nearest_bar_time() {
        with(|proj| {
            let bar_time = 1_767_571_200_000 + 60 * 300_000;
            let (x, y) = proj
                .to_screen(Point {
                    t: bar_time,
                    p: 100_000.0,
                })
                .unwrap();
            // A few pixels to either side is still the same bar (8 px per bar).
            for dx in [-3.0, 0.0, 3.0] {
                assert_eq!(proj.point_at(x + dx, y, false).unwrap().t, bar_time);
            }
        });
    }

    #[test]
    fn times_past_the_last_bar_are_placed_by_the_bar_length() {
        with(|proj| {
            let last = 1_767_571_200_000 + 99 * 300_000;
            let (x_last, y) = proj
                .to_screen(Point {
                    t: last,
                    p: 100_000.0,
                })
                .unwrap();
            let (x_later, _) = proj
                .to_screen(Point {
                    t: last + 3 * 300_000,
                    p: 100_000.0,
                })
                .unwrap();
            assert!((x_later - x_last - 24.0).abs() < 0.1, "three bars of 8 px");
            // And a click there gives that future time.
            let at = proj.point_at(x_later, y, false).unwrap();
            assert_eq!(at.t, last + 3 * 300_000);
        });
    }

    #[test]
    fn the_magnet_takes_the_nearest_ohlc_only_when_close() {
        with(|proj| {
            let bar_time = 1_767_571_200_000 + 40 * 300_000;
            let bar_high = 100_050.0 + 40.0;
            let (x, y_high) = proj
                .to_screen(Point {
                    t: bar_time,
                    p: bar_high,
                })
                .unwrap();
            // A few pixels below the high: snaps to it.
            let snapped = proj.point_at(x, y_high + 5.0, true).unwrap();
            assert_eq!(snapped.p, bar_high);
            let free = proj.point_at(x, y_high + 5.0, false).unwrap();
            assert_ne!(free.p, bar_high);
            // Far from any of the four: left alone.
            let far = proj.point_at(x, y_high - 150.0, true).unwrap();
            assert!(
                far.p != bar_high
                    && (far.p - proj.map.price(f64::from(y_high - 150.0))).abs() < 1e-6
            );
        });
    }

    #[test]
    fn shifting_by_bars_and_counting_them_agree() {
        with(|proj| {
            let start = 1_767_571_200_000 + 10 * 300_000;
            let later = proj.shift_bars(start, 12.0).unwrap();
            assert_eq!(later, start + 12 * 300_000);
            assert_eq!(
                proj.index_of(later).unwrap() - proj.index_of(start).unwrap(),
                12.0
            );
        });
    }

    #[test]
    fn an_empty_chart_places_nothing() {
        let series = Series::Bars(Vec::new());
        let view = View::new(8.0);
        let projection = ChartProjection {
            series: &series,
            view: &view,
            map: PriceMap::linear(0.0, 1.0, 0.0, 100.0),
            plot_w: 500.0,
            plot_h: 300.0,
            step_ms: 1_000.0,
            digits: 5,
            pip_position: Some(4),
        };
        assert!(projection.to_screen(Point { t: 0, p: 0.5 }).is_none());
        assert!(projection.point_at(10.0, 10.0, false).is_none());
    }

    #[test]
    fn pips_use_the_symbol_position_instead_of_the_quote_tick() {
        with(|proj| {
            assert!((proj.tick() - 0.00001).abs() < 1e-12);
            assert!((proj.pip() - 0.0001).abs() < 1e-12);
        });
    }
}
