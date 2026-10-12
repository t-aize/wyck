#![cfg(test)]

mod lines;
mod picking;
mod positions;

use super::hit::*;

use super::*;
use crate::domain::drawings::model::style::{Dash, Style};
use crate::domain::drawings::model::tool::Tool;
use crate::domain::drawings::model::{Drawing, Point};

/// A projection where time is 1 pixel per second from 0, and price is 1 pixel per unit
/// upwards from the bottom of a 1000 by 500 plot. Bars are one minute long.
pub struct Linear;

impl Projection for Linear {
    fn plot(&self) -> Rect {
        Rect::new(1_000.0, 500.0)
    }
    fn to_screen(&self, point: Point) -> Option<P> {
        Some(((point.t / 1_000) as f32, 500.0 - point.p as f32))
    }
    fn point_at(&self, x: f32, y: f32, _magnet: bool) -> Option<Point> {
        // Snap the time to whole minutes, like snapping to a bar.
        let seconds = (x / 60.0).round() as i64 * 60;
        Some(Point {
            t: seconds * 1_000,
            p: f64::from(500.0 - y),
        })
    }
    fn index_of(&self, time_ms: i64) -> Option<f64> {
        Some(time_ms as f64 / 60_000.0)
    }
    fn shift_bars(&self, time_ms: i64, bars: f64) -> Option<i64> {
        Some(time_ms + (bars * 60_000.0).round() as i64)
    }
    fn format_price(&self, price: f64) -> String {
        format!("{price:.1}")
    }
    fn price_span(&self) -> f64 {
        500.0
    }
    fn bars_between(&self, from_ms: i64, to_ms: i64) -> Vec<BarView> {
        // A bar a minute, opening at that minute at a price of 100 plus its number.
        let first = (from_ms.max(0) + 59_999) / 60_000;
        let last = (to_ms / 60_000).min(first + 1_999);
        (first..=last)
            .map(|i| BarView {
                time: i * 60_000,
                x: (i * 60) as f32,
                open: 100.0 + i as f64,
                high: 103.0 + i as f64,
                low: 98.0 + i as f64,
                close: 101.0 + i as f64,
                volume: 10.0,
            })
            .collect()
    }
    fn y_of(&self, price: f64) -> f32 {
        500.0 - price as f32
    }
    fn pip(&self) -> f64 {
        0.01
    }
}

pub fn drawing(tool: Tool, points: &[(i64, f64)]) -> Drawing {
    let mut drawing = Drawing::new(
        1,
        tool,
        points
            .iter()
            .map(|&(t, p)| Point { t: t * 1_000, p })
            .collect(),
    );
    drawing.style = Style {
        width: 1.0,
        ..tool.default_style()
    };
    drawing
}
