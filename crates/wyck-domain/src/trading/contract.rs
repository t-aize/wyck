//! How a symbol trades: its lot, the volumes the broker accepts, where its pip is.
//!
//! # Units
//!
//! - A **volume** is what the server counts: hundredths of a unit of the base asset.
//! - A **lot** is `lot_size` of those (100 000 units of a currency pair, for example).
//! - **Prices** here are real (1.08412), like the prices of positions and orders on the wire.
//! - **Money** is in the account's deposit currency, as real numbers.

use crate::base::math::{f64_to_i64, i64_to_f64};
use crate::market::PRICE_SCALE;

/// How a symbol trades: its lot, the volumes it accepts and where its pip is.
///
/// ```
/// use wyck_domain::trading::contract::Contract;
/// use wyck_domain::trading::risk::lots_for_risk;
///
/// // A forex pair: 100 000 units a lot, steps of 0.01 lot, pips at the fourth decimal.
/// let eurusd = Contract::default();
/// assert_eq!(eurusd.volume_near(0.5).volume, 5_000_000);
/// assert!((eurusd.pips(0.0020) - 20.0).abs() < 1e-9);
///
/// // Risk 100 of the deposit currency over a 20 pip stop, quote currency = deposit currency.
/// let lots = lots_for_risk(100.0, 0.0020, 1.0, &eurusd).unwrap();
/// assert_eq!(eurusd.volume_near(lots).volume, 5_000_000);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contract {
    /// Decimals of the symbol's prices.
    pub digits: u32,
    /// The decimal a pip sits at: `4` makes a pip `0.0001`.
    pub pip_position: i64,
    /// Hundredths of a unit per lot.
    pub lot_size: i64,
    /// The smallest volume the broker accepts.
    pub min_volume: i64,
    /// The largest volume the broker accepts.
    pub max_volume: i64,
    /// The volumes accepted are multiples of this one.
    pub step_volume: i64,
}

impl Default for Contract {
    fn default() -> Self {
        // A forex pair until the broker says otherwise.
        Self {
            digits: 5,
            pip_position: 4,
            lot_size: 10_000_000,
            min_volume: 100_000,
            max_volume: 10_000_000_000,
            step_volume: 100_000,
        }
    }
}

impl Contract {
    /// The contract of a symbol as the server describes it, with a forex pair's values for
    /// anything missing or zero.
    pub fn from_symbol(symbol: &crate::market::Symbol) -> Self {
        let default = Self::default();
        let positive =
            |value: Option<i64>, fallback: i64| value.filter(|v| *v > 0).unwrap_or(fallback);
        let lot_size = positive(symbol.lot_size, default.lot_size);
        Self {
            digits: u32::try_from(symbol.digits).unwrap_or(5),
            pip_position: symbol.pip_position,
            lot_size,
            min_volume: positive(symbol.min_volume, lot_size / 100),
            max_volume: positive(symbol.max_volume, lot_size * 1_000),
            step_volume: positive(symbol.step_volume, lot_size / 100),
        }
    }

    /// The volume of `lots`, rounded to a step the broker accepts and kept inside its limits.
    pub(crate) fn volume_of_lots(&self, lots: f64) -> i64 {
        if !lots.is_finite() || lots <= 0.0 {
            return self.min_volume;
        }
        let raw = lots * i64_to_f64(self.lot_size);
        let step = i64_to_f64(self.step_volume.max(1));
        let stepped = f64_to_i64((raw / step).round() * step);
        stepped.clamp(self.min_volume, self.max_volume.max(self.min_volume))
    }

    pub fn lots_of_volume(&self, volume: i64) -> f64 {
        i64_to_f64(volume) / i64_to_f64(self.lot_size.max(1))
    }

    /// The size of one pip in price.
    pub fn pip(&self) -> f64 {
        crate::market::pip_size(self.pip_position)
    }

    /// A price distance in pips.
    pub fn pips(&self, distance: f64) -> f64 {
        distance / self.pip()
    }

    /// A price rounded to the symbol's decimals.
    pub fn round_price(&self, price: f64) -> f64 {
        let scale = 10f64.powi(i32::try_from(self.digits).unwrap_or(5));
        (price * scale).round() / scale
    }

    /// A price written with the symbol's decimals.
    pub fn format_price(&self, price: f64) -> String {
        format!("{price:.*}", self.digits as usize)
    }

    /// The volume of `lots` rounded to the nearest step and kept inside the broker's limits.
    pub fn volume_near(&self, lots: f64) -> Stepped {
        let volume = self.volume_of_lots(lots);
        let raw = lots * i64_to_f64(self.lot_size);
        let limit = if raw < i64_to_f64(self.min_volume) - 0.5 {
            Some(Limit::Min)
        } else if raw > i64_to_f64(self.max_volume.max(self.min_volume)) + 0.5 {
            Some(Limit::Max)
        } else {
            None
        };
        Stepped { volume, limit }
    }

    /// The volume of `lots` stepped down, so a volume sized from a risk never risks more, and
    /// kept inside the broker's limits.
    pub fn volume_at_most(&self, lots: f64) -> Stepped {
        let raw = if lots.is_finite() { lots.max(0.0) } else { 0.0 } * i64_to_f64(self.lot_size);
        let step = i64_to_f64(self.step_volume.max(1));
        // A hair of tolerance, so 0.3 lots is not stepped down to 0.29 by the float error.
        let stepped = f64_to_i64((raw / step + 1e-9).floor() * step);
        let max = self.max_volume.max(self.min_volume);
        if stepped < self.min_volume {
            Stepped {
                volume: self.min_volume,
                limit: Some(Limit::Min),
            }
        } else if stepped > max {
            Stepped {
                volume: max,
                limit: Some(Limit::Max),
            }
        } else {
            Stepped {
                volume: stepped,
                limit: None,
            }
        }
    }
}

/// A volume chosen by the ticket, and whether the broker's limits changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stepped {
    pub volume: i64,
    /// The limit that changed it, if one did.
    pub limit: Option<Limit>,
}

/// A limit of the broker the wanted volume ran into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// Raised to the least volume the broker takes.
    Min,
    /// Lowered to the most it takes.
    Max,
}

/// Lots written plainly: `0.01`, `1.5`, `10`.
pub fn format_lots(lots: f64) -> String {
    crate::base::format::trim(lots, 2)
}

/// A price distance in the server's relative units (a stop loss or take profit of a market
/// order is given as a distance from where it fills).
///
/// The server takes only multiples of the smallest step of the symbol's price, so the distance
/// is rounded to `digits` decimals: on a three digit symbol the units go by hundreds. A distance
/// is never rounded down to nothing.
pub fn relative_distance(distance: f64, digits: u32) -> i64 {
    let step = 10i64.pow(PRICE_DIGITS.saturating_sub(digits.min(PRICE_DIGITS)));
    let units = f64_to_i64((distance.abs() * i64_to_f64(PRICE_SCALE) / i64_to_f64(step)).round());
    units.max(1) * step
}

/// The decimals of the server's price scale (`PRICE_SCALE` is ten to this).
const PRICE_DIGITS: u32 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn lots_become_volumes_the_broker_accepts() {
        let c = Contract::default();
        assert_eq!(c.volume_of_lots(1.0), 10_000_000);
        assert_eq!(c.volume_of_lots(0.013), 100_000, "rounded to the 0.01 step");
        assert_eq!(c.volume_of_lots(0.0), c.min_volume);
        assert_eq!(c.volume_of_lots(f64::NAN), c.min_volume);
        assert_eq!(c.volume_of_lots(1e9), c.max_volume);
        assert!(close(c.lots_of_volume(2_500_000), 0.25));
    }

    #[test]
    fn pips_follow_the_pip_position() {
        let fx = Contract::default();
        assert!(close(fx.pip(), 0.0001));
        assert!((fx.pips(0.0025) - 25.0).abs() < 1e-6);
        let jpy = Contract {
            digits: 3,
            pip_position: 2,
            ..Contract::default()
        };
        assert!((jpy.pips(0.25) - 25.0).abs() < 1e-6);
        assert_eq!(jpy.format_price(151.2), "151.200");
        assert!(close(fx.round_price(1.084_123_7), 1.08412));
    }

    #[test]
    fn a_distance_is_counted_in_the_servers_units() {
        assert_eq!(relative_distance(0.0025, 5), 250);
        assert_eq!(relative_distance(-0.0025, 5), 250);
        // A three digit symbol takes multiples of a hundred units (a thousandth).
        assert_eq!(relative_distance(0.1234, 3), 12_300);
        assert_eq!(relative_distance(0.12351, 3), 12_400);
        // A two digit symbol: multiples of a thousand units (a hundredth).
        assert_eq!(relative_distance(0.2549, 2), 25_000);
        // Never nothing, and more digits than the scale has change nothing.
        assert_eq!(relative_distance(0.00001, 2), 1_000);
        assert_eq!(relative_distance(0.00012, 7), 12);
    }
}
