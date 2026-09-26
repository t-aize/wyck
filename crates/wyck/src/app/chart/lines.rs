//! Horizontal lines the chart shows for things outside it: working orders, open positions with
//! their stop loss and take profit, and price alerts.
//!
//! The chart knows nothing of trading or alerts. It is handed [`ChartLine`]s, draws each as a line
//! across the prices with a label on the left and a tag on the axis, lets the user drag the ones
//! that may move and close the ones that may close, and reports what the user did as
//! [`super::ChartEvent::LineMoved`] and [`super::ChartEvent::LineClosed`]. What that means (amend
//! an order, move a stop, delete an alert) is decided by whoever made the lines.

use wyck_openapi::market::PRICE_SCALE;

use super::drawing::model::Dash;
use super::scene::PriceMark;

pub use wyck_chart::lines::LineId;

#[derive(Debug, Clone, PartialEq)]
pub struct ChartLine {
    pub id: LineId,
    /// The price in the symbol's own units (1.08412).
    pub price: f64,
    /// `0xRRGGBB`.
    pub color: u32,
    /// What the label on the line says: `Buy 0.10`, `SL`, `Alert`.
    pub label: String,
    /// A second part of the label, drawn in its own color: the profit of a position.
    pub detail: Option<(String, u32)>,
    pub dash: Dash,
    /// Whether the line can be dragged to a new price.
    pub draggable: bool,
    /// Whether the label has a close button.
    pub closable: bool,
}

impl ChartLine {
    /// The price in the chart's raw units.
    pub fn raw_price(&self) -> f64 {
        self.price * PRICE_SCALE as f64
    }

    /// The line as the scene draws it.
    pub fn mark(&self) -> PriceMark {
        PriceMark {
            price: self.raw_price(),
            color: self.color,
            dash: self.dash,
            width: if matches!(self.id, LineId::Position(_)) {
                1.5
            } else {
                1.0
            },
            from_x: None,
            axis_tag: true,
        }
    }
}

/// A raw price as a real one.
pub fn to_real(raw: f64) -> f64 {
    raw / PRICE_SCALE as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_drawn_at_its_raw_price() {
        let line = ChartLine {
            id: LineId::Alert(1),
            price: 1.08412,
            color: 0xffb900,
            label: "Alert".into(),
            detail: None,
            dash: Dash::Dashed,
            draggable: true,
            closable: true,
        };
        assert!((line.mark().price - 108_412.0).abs() < 1e-6);
        assert!((to_real(line.raw_price()) - 1.08412).abs() < 1e-12);
    }
}
