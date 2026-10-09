//! ATR settings and price levels used before an order is sent.

use serde::{Deserialize, Serialize};
use wyck_openapi::market::{Bar, PRICE_SCALE};

use super::math;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Smoothing {
    #[default]
    Rma,
    Sma,
    Ema,
    Wma,
}

impl Smoothing {
    pub const ALL: [Self; 4] = [Self::Rma, Self::Sma, Self::Ema, Self::Wma];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rma => "RMA",
            Self::Sma => "SMA",
            Self::Ema => "EMA",
            Self::Wma => "WMA",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AtrStop {
    pub length: usize,
    pub smoothing: Smoothing,
    pub multiplier: f64,
    /// None follows the chart timeframe; otherwise this is a timeframe code.
    pub timeframe: Option<String>,
    pub current_bar: bool,
}

impl Default for AtrStop {
    fn default() -> Self {
        Self {
            length: 14,
            smoothing: Smoothing::Rma,
            multiplier: 2.0,
            timeframe: None,
            current_bar: false,
        }
    }
}

impl AtrStop {
    pub fn normalized(mut self) -> Self {
        self.length = self.length.clamp(1, 1_000);
        if !self.multiplier.is_finite() || self.multiplier <= 0.0 {
            self.multiplier = 2.0;
        }
        self.multiplier = self.multiplier.min(1_000.0);
        self
    }

    pub fn value(&self, bars: &[Bar], last_is_open: bool) -> Option<f64> {
        let usable = if self.current_bar {
            bars
        } else {
            if last_is_open {
                &bars[..bars.len() - 1]
            } else {
                bars
            }
        };
        if usable.len() < self.length {
            return None;
        }
        let price = |raw| raw as f64 / PRICE_SCALE as f64;
        let high: Vec<_> = usable.iter().map(|b| price(b.high)).collect();
        let low: Vec<_> = usable.iter().map(|b| price(b.low)).collect();
        let close: Vec<_> = usable.iter().map(|b| price(b.close)).collect();
        let tr = math::true_range(&high, &low, &close);
        let series = match self.smoothing {
            Smoothing::Rma => math::rma(&tr, self.length),
            Smoothing::Sma => math::sma(&tr, self.length),
            Smoothing::Ema => math::ema(&tr, self.length),
            Smoothing::Wma => math::wma(&tr, self.length),
        };
        series.last().copied().filter(|v| v.is_finite() && *v > 0.0)
    }

    pub fn stop(&self, entry: f64, buy: bool, atr: f64) -> Option<f64> {
        let distance = atr * self.multiplier;
        (entry.is_finite() && distance.is_finite() && distance > 0.0)
            .then_some(entry + if buy { -distance } else { distance })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_smoothing_methods_and_bar_choices() {
        let bars: Vec<_> = (0..20)
            .map(|i| Bar {
                time_ms: i * 60_000,
                open: 100_000,
                high: 100_000 + i * 100,
                low: 99_000,
                close: 100_000,
                volume: 1,
            })
            .collect();
        for smoothing in Smoothing::ALL {
            let settings = AtrStop {
                length: 3,
                smoothing,
                ..AtrStop::default()
            };
            let closed = settings.value(&bars, true).unwrap();
            let live = AtrStop {
                current_bar: true,
                ..settings.clone()
            }
            .value(&bars, true)
            .unwrap();
            assert!(live > closed);
            assert!(settings.stop(10.0, true, closed).unwrap() < 10.0);
            assert!(settings.stop(10.0, false, closed).unwrap() > 10.0);
        }
    }
}
