//! The ticket's own arithmetic on top of [`crate::infra::ctrader::trading::contract`]: how an order's
//! size and its stop loss and take profit are typed, and how money is written.

pub use crate::infra::ctrader::trading::contract::*;

/// How the volume of an order is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeMode {
    /// Lots, as typed.
    #[default]
    Lots,
    /// Units of the base asset, as typed.
    Units,
    /// A share of the balance lost if the stop loss is hit.
    RiskBalance,
    /// A share of the equity lost if the stop loss is hit.
    RiskEquity,
    /// An amount of the deposit currency lost if the stop loss is hit.
    RiskMoney,
    /// A share of the free margin the order may use.
    FreeMargin,
}

impl SizeMode {
    pub const ALL: [Self; 6] = [
        Self::Lots,
        Self::Units,
        Self::RiskBalance,
        Self::RiskEquity,
        Self::RiskMoney,
        Self::FreeMargin,
    ];

    /// Whether the volume comes from the distance to the stop loss.
    pub fn is_risk(self) -> bool {
        matches!(self, Self::RiskBalance | Self::RiskEquity | Self::RiskMoney)
    }
}

/// How a stop loss or take profit is given: as a price, or as a distance from the entry in
/// pips, in money, in percent of the balance, or in multiples of the risk (a take profit only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Offset {
    #[default]
    Price,
    Pips,
    Money,
    Percent,
    Ratio,
}

impl Offset {
    /// Whether the distance it stands for depends on the volume.
    pub fn needs_volume(self) -> bool {
        matches!(self, Self::Money | Self::Percent)
    }
}

/// What turns an offset into a price distance and back.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scale {
    /// The size of a pip in price.
    pub pip: f64,
    /// What a move of 1.0 in price is worth in the deposit currency, for the volume of the
    /// order: its units times the rate of the quote currency. `None` until the rate is known.
    pub money_per_price: Option<f64>,
    pub balance: f64,
    /// The distance from the entry to the stop loss, for a take profit given in multiples of
    /// the risk.
    pub stop_distance: Option<f64>,
}

impl Scale {
    /// The price distance a value of `offset` stands for (positive away from the entry, on the
    /// side the protection belongs).
    pub fn distance(&self, offset: Offset, value: f64) -> Option<f64> {
        let per_price = || self.money_per_price.filter(|m| *m > 0.0);
        match offset {
            Offset::Price => None,
            Offset::Pips => Some(value * self.pip),
            Offset::Money => per_price().map(|m| value / m),
            Offset::Percent => per_price().map(|m| self.balance * value / 100.0 / m),
            Offset::Ratio => self.stop_distance.map(|d| value * d),
        }
    }

    /// The value of `offset` a price distance stands for.
    pub fn value(&self, offset: Offset, distance: f64) -> Option<f64> {
        let per_price = || self.money_per_price.filter(|m| *m > 0.0);
        match offset {
            Offset::Price => None,
            Offset::Pips => (self.pip > 0.0).then(|| distance / self.pip),
            Offset::Money => per_price().map(|m| distance * m),
            Offset::Percent => per_price()
                .filter(|_| self.balance > 0.0)
                .map(|m| distance * m / self.balance * 100.0),
            Offset::Ratio => self
                .stop_distance
                .filter(|d| *d > 0.0)
                .map(|d| distance / d),
        }
    }
}

/// An amount of money with its currency: `1,234.56 USD`, `-12.30 EUR`.
pub fn format_money(amount: f64, currency: &str) -> String {
    let negative = amount < 0.0;
    let cents = (amount.abs() * 100.0).round() as i64;
    let (whole, rest) = (cents / 100, cents % 100);
    let grouped = crate::domain::chart::format::grouped(whole);
    let sign = if negative && cents > 0 { "-" } else { "" };
    if currency.is_empty() {
        format!("{sign}{grouped}.{rest:02}")
    } else {
        format!("{sign}{grouped}.{rest:02} {currency}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_grouped_and_signed() {
        assert_eq!(format_money(1_234.5, "USD"), "1,234.50 USD");
        assert_eq!(format_money(-12.3, "EUR"), "-12.30 EUR");
        assert_eq!(format_money(-0.001, ""), "0.00");
        assert_eq!(format_money(1_000_000.0, ""), "1,000,000.00");
    }

    #[test]
    fn offsets_turn_into_distances_and_back() {
        let scale = Scale {
            pip: 0.0001,
            // 1 lot of EURUSD: 100 000 USD per 1.0 of price.
            money_per_price: Some(100_000.0),
            balance: 10_000.0,
            stop_distance: Some(0.0020),
        };
        let cases = [
            (Offset::Pips, 20.0, 0.0020),
            (Offset::Money, 200.0, 0.0020),
            (Offset::Percent, 2.0, 0.0020),
            (Offset::Ratio, 2.0, 0.0040),
        ];
        for (offset, value, distance) in cases {
            let d = scale.distance(offset, value).unwrap();
            assert!((d - distance).abs() < 1e-12, "{offset:?}");
            let back = scale.value(offset, distance).unwrap();
            assert!((back - value).abs() < 1e-9, "{offset:?}");
        }
        assert_eq!(scale.distance(Offset::Price, 1.1), None);
        let unknown = Scale {
            money_per_price: None,
            stop_distance: None,
            ..scale
        };
        assert_eq!(unknown.distance(Offset::Money, 100.0), None);
        assert_eq!(unknown.distance(Offset::Ratio, 2.0), None);
        assert_eq!(protection_side(true, true), -1.0);
        assert_eq!(protection_side(true, false), 1.0);
        assert_eq!(protection_side(false, true), 1.0);
    }
}
