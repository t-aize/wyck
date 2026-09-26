//! What a long or short position drawing is sized and told with: the account it is for, how much
//! it risks, and which numbers it writes on the chart.
//!
//! The size follows the way trading tools read it: the risk (a share of the account, or an
//! amount) divided by the distance to the stop gives a quantity, the leverage puts a ceiling on
//! that quantity, and the smaller of the two is rounded down to the lot step. Profit and loss are
//! that quantity times the price distance times the point value.
//!
//! Money is in the currency of the account. The point value says how much one unit is worth per
//! 1.0 of price: 1.0 when the instrument is quoted in the account currency (an index in dollars,
//! EURUSD on a dollar account), something else when it is not. There is no conversion rate here,
//! so the figures are a plan, not a quote.

use serde::{Deserialize, Serialize};

/// Everything a position drawing keeps besides its points and its lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionSettings {
    /// The balance the position is sized for.
    #[serde(default = "default_account")]
    pub account: f64,
    /// How much is risked: a percentage of the account, or an amount (see `risk_percent`).
    #[serde(default = "default_risk")]
    pub risk: f64,
    #[serde(default = "yes")]
    pub risk_percent: bool,
    /// The step a quantity is rounded down to.
    #[serde(default = "one")]
    pub lot_size: f64,
    /// The most the account can carry: the quantity is capped at account times leverage over the
    /// entry price.
    #[serde(default = "default_leverage")]
    pub leverage: f64,
    /// What one unit gains per 1.0 of price, in the account currency.
    #[serde(default = "one")]
    pub point_value: f64,
    /// How many decimals a quantity is written with.
    #[serde(default = "default_precision")]
    pub qty_precision: u8,
    /// The currency written after amounts. Empty writes none.
    #[serde(default)]
    pub currency: String,
    #[serde(default = "default_target_color")]
    pub target_color: u32,
    #[serde(default = "default_stop_color")]
    pub stop_color: u32,
    #[serde(default = "default_entry_color")]
    pub entry_color: u32,
    /// The stats written on the chart, one switch each.
    #[serde(default = "yes")]
    pub show_qty: bool,
    #[serde(default = "yes")]
    pub show_risk: bool,
    #[serde(default = "yes")]
    pub show_amounts: bool,
    #[serde(default = "yes")]
    pub show_ratio: bool,
    #[serde(default = "yes")]
    pub show_percent: bool,
    #[serde(default)]
    pub show_ticks: bool,
    #[serde(default)]
    pub show_pips: bool,
    #[serde(default = "yes")]
    pub show_price: bool,
    /// Shorter tags, for a crowded chart.
    #[serde(default = "yes")]
    pub compact: bool,
    /// Whether the tags show all the time. Off, they show only while the drawing is selected.
    #[serde(default)]
    pub always_stats: bool,
}

fn yes() -> bool {
    true
}

fn one() -> f64 {
    1.0
}

fn default_account() -> f64 {
    10_000.0
}

fn default_risk() -> f64 {
    1.0
}

fn default_leverage() -> f64 {
    100.0
}

fn default_precision() -> u8 {
    2
}

fn default_target_color() -> u32 {
    0x00d492
}

fn default_stop_color() -> u32 {
    0xff6467
}

fn default_entry_color() -> u32 {
    0xffffff
}

impl Default for PositionSettings {
    fn default() -> Self {
        Self {
            account: default_account(),
            risk: default_risk(),
            risk_percent: true,
            lot_size: 1.0,
            leverage: default_leverage(),
            point_value: 1.0,
            qty_precision: default_precision(),
            currency: String::new(),
            target_color: default_target_color(),
            stop_color: default_stop_color(),
            entry_color: default_entry_color(),
            show_qty: true,
            show_risk: true,
            show_amounts: true,
            show_ratio: true,
            show_percent: true,
            show_ticks: false,
            show_pips: false,
            show_price: true,
            compact: true,
            always_stats: false,
        }
    }
}

/// What the numbers of a position come to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub qty: f64,
    /// What the stop would lose at that quantity, positive.
    pub loss: f64,
    /// What the target would gain at that quantity, positive.
    pub profit: f64,
    /// Reward over risk, by price distance.
    pub ratio: f64,
    /// The risk asked for, as an amount.
    pub risk_amount: f64,
    /// Whether the leverage, and not the risk, set the quantity.
    pub capped: bool,
    pub stop_percent: f64,
    pub target_percent: f64,
    pub stop_ticks: f64,
    pub target_ticks: f64,
}

impl PositionSettings {
    /// The settings with every number in a range that means something, whatever a file said.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let fix = |value: f64, fallback: f64, least: f64, most: f64| {
            if value.is_finite() {
                value.clamp(least, most)
            } else {
                fallback
            }
        };
        self.account = fix(self.account, default_account(), 1.0, 1e12);
        self.risk = if self.risk_percent {
            fix(self.risk, default_risk(), 0.01, 100.0)
        } else {
            fix(self.risk, default_risk(), 0.01, 1e12)
        };
        self.lot_size = fix(self.lot_size, 1.0, 1e-8, 1e9);
        self.leverage = fix(self.leverage, default_leverage(), 1.0, 10_000.0);
        self.point_value = fix(self.point_value, 1.0, 1e-9, 1e9);
        self.qty_precision = self.qty_precision.min(8);
        self.currency = self.currency.trim().chars().take(8).collect();
        self.target_color &= 0xff_ffff;
        self.stop_color &= 0xff_ffff;
        self.entry_color &= 0xff_ffff;
        self
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The amount risked.
    pub fn risk_amount(&self) -> f64 {
        if self.risk_percent {
            self.account * self.risk / 100.0
        } else {
            self.risk
        }
    }

    /// The numbers of a position with these real (not raw) prices. `tick` is the smallest step of
    /// the price, or 0 when it is not known. `None` when the stop sits on the entry, so there is
    /// nothing to size.
    pub fn stats(&self, entry: f64, stop: f64, target: f64, tick: f64) -> Option<Stats> {
        let risk_distance = (entry - stop).abs();
        if !(risk_distance.is_finite() && risk_distance > 0.0 && entry > 0.0) {
            return None;
        }
        let by_risk = self.risk_amount() / (risk_distance * self.point_value);
        let by_leverage = self.account * self.leverage / entry;
        let capped = by_leverage < by_risk;
        let mut qty = by_risk.min(by_leverage);
        // Down to the lot step, then to the decimals it is written with: never up, so the
        // rounding cannot take the risk past what was asked.
        qty = (qty / self.lot_size).floor() * self.lot_size;
        let scale = 10f64.powi(i32::from(self.qty_precision));
        qty = (qty * scale + 1e-9).floor() / scale;
        if !qty.is_finite() || qty < 0.0 {
            qty = 0.0;
        }
        let reward_distance = (target - entry).abs();
        let percent = |price: f64| (price - entry) / entry * 100.0;
        let ticks = |distance: f64| if tick > 0.0 { distance / tick } else { 0.0 };
        Some(Stats {
            qty,
            loss: qty * risk_distance * self.point_value,
            profit: qty * reward_distance * self.point_value,
            ratio: reward_distance / risk_distance,
            risk_amount: self.risk_amount(),
            capped,
            stop_percent: percent(stop),
            target_percent: percent(target),
            stop_ticks: ticks(risk_distance),
            target_ticks: ticks(reward_distance),
        })
    }

    /// A quantity as written: its decimals, no trailing zeros.
    pub fn format_qty(&self, qty: f64) -> String {
        let text = format!("{qty:.*}", usize::from(self.qty_precision));
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text
        }
    }

    /// An amount with its sign, two decimals and the currency.
    pub fn format_amount(&self, amount: f64) -> String {
        let sign = if amount < 0.0 { "-" } else { "+" };
        let text = format!("{sign}{:.2}", amount.abs());
        if self.currency.is_empty() {
            text
        } else {
            format!("{text} {}", self.currency)
        }
    }

    /// An amount that has no sign of its own (a risk, an account).
    pub fn format_plain(&self, amount: f64) -> String {
        let text = format!("{amount:.2}");
        if self.currency.is_empty() {
            text
        } else {
            format!("{text} {}", self.currency)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> PositionSettings {
        PositionSettings {
            account: 10_000.0,
            risk: 1.0,
            risk_percent: true,
            leverage: 1000.0,
            ..PositionSettings::default()
        }
    }

    #[test]
    fn the_quantity_is_the_risk_over_the_distance_to_the_stop() {
        // 1% of 10 000 is 100; a stop 2.0 away means 50 units; the target 5.0 away gains 250.
        let stats = settings().stats(100.0, 98.0, 105.0, 0.01).unwrap();
        assert_eq!(stats.qty, 50.0);
        assert_eq!(stats.loss, 100.0);
        assert_eq!(stats.profit, 250.0);
        assert!((stats.ratio - 2.5).abs() < 1e-9);
        assert!((stats.target_percent - 5.0).abs() < 1e-9);
        assert!((stats.stop_percent + 2.0).abs() < 1e-9);
        assert_eq!(stats.target_ticks.round(), 500.0);
        assert!(!stats.capped);
    }

    #[test]
    fn a_short_reads_the_same_as_a_long_by_distance() {
        let long = settings().stats(100.0, 98.0, 105.0, 0.0).unwrap();
        let short = settings().stats(100.0, 102.0, 95.0, 0.0).unwrap();
        assert_eq!(long.qty, short.qty);
        assert_eq!(long.profit, short.profit);
        assert_eq!(long.target_ticks, 0.0, "no tick, no ticks");
    }

    #[test]
    fn the_leverage_caps_a_quantity_the_risk_would_allow() {
        // A stop 0.01 away asks for 10 000 units; 10 000 at 100 with leverage 10 allows 1 000.
        let capped = PositionSettings {
            leverage: 10.0,
            ..settings()
        };
        let stats = capped.stats(100.0, 99.99, 101.0, 0.0).unwrap();
        assert_eq!(stats.qty, 1_000.0);
        assert!(stats.capped);
        assert!(
            stats.loss < stats.risk_amount,
            "the cap is safer, not riskier"
        );
    }

    #[test]
    fn the_quantity_is_rounded_down_to_the_lot_and_the_precision() {
        let lots = PositionSettings {
            lot_size: 10.0,
            ..settings()
        };
        // 100 / 3 = 33.33 units: three lots of ten is thirty, never thirty-three or forty.
        assert_eq!(lots.stats(100.0, 97.0, 110.0, 0.0).unwrap().qty, 30.0);

        let precise = PositionSettings {
            lot_size: 0.01,
            qty_precision: 1,
            ..settings()
        };
        let stats = precise.stats(100.0, 97.0, 110.0, 0.0).unwrap();
        assert_eq!(stats.qty, 33.3);
        assert!(
            stats.loss <= stats.risk_amount + 1e-9,
            "never past the risk"
        );
    }

    #[test]
    fn an_amount_of_risk_and_a_point_value_are_honored() {
        let amount = PositionSettings {
            risk: 50.0,
            risk_percent: false,
            point_value: 2.0,
            lot_size: 0.5,
            ..settings()
        };
        // 50 over (2.0 away x 2 per point) is 12.5 units, worth 50 if the stop is hit.
        let stats = amount.stats(100.0, 98.0, 104.0, 0.0).unwrap();
        assert_eq!(stats.qty, 12.5);
        assert_eq!(stats.loss, 50.0);
        assert_eq!(stats.profit, 100.0);
    }

    #[test]
    fn a_stop_on_the_entry_has_nothing_to_size() {
        assert!(settings().stats(100.0, 100.0, 105.0, 0.0).is_none());
        assert!(settings().stats(0.0, -1.0, 5.0, 0.0).is_none());
        assert!(settings().stats(f64::NAN, 98.0, 105.0, 0.0).is_none());
    }

    #[test]
    fn wild_values_are_brought_back_in_range() {
        let wild = PositionSettings {
            account: -5.0,
            risk: 900.0,
            lot_size: 0.0,
            leverage: f64::NAN,
            point_value: -1.0,
            qty_precision: 200,
            currency: "  a very long currency  ".to_owned(),
            ..PositionSettings::default()
        }
        .normalized();
        assert_eq!(wild.account, 1.0);
        assert_eq!(wild.risk, 100.0);
        assert!(wild.lot_size > 0.0);
        assert_eq!(wild.leverage, 100.0);
        assert!(wild.point_value > 0.0);
        assert_eq!(wild.qty_precision, 8);
        assert_eq!(wild.currency, "a very l");
    }

    #[test]
    fn numbers_and_amounts_are_written_plainly() {
        let mut s = PositionSettings::default();
        assert_eq!(s.format_qty(50.0), "50");
        assert_eq!(s.format_qty(33.30), "33.3");
        assert_eq!(s.format_amount(250.0), "+250.00");
        assert_eq!(s.format_amount(-100.0), "-100.00");
        s.currency = "USD".to_owned();
        assert_eq!(s.format_amount(-100.0), "-100.00 USD");
        assert_eq!(s.format_plain(100.0), "100.00 USD");
    }

    #[test]
    fn a_position_saved_before_these_settings_loads_with_the_defaults() {
        let old: PositionSettings = toml::from_str("account = 500.0\n").unwrap();
        assert_eq!(old.account, 500.0);
        assert_eq!(old.risk, 1.0);
        assert!(old.risk_percent && old.compact && !old.always_stats && old.show_qty);
        let visible: PositionSettings = toml::from_str("always_stats = true\n").unwrap();
        assert!(visible.always_stats);
        let full: PositionSettings = toml::from_str("compact = false\n").unwrap();
        assert!(!full.compact);
        assert!(PositionSettings::default().is_default());
        assert!(!old.is_default());
    }
}
