//! What the order ticket works with: how the size and the protection of an order are typed
//! ([`SizeMode`], [`Offset`], [`Scale`]), which pending order a price makes, and the problems that
//! stop an order before it is sent. The problems are codes: the sentence belongs to the screen.

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

/// Which pending order a price makes for a side, given the market.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    /// Waits for the price to come to it.
    Limit,
    /// Waits for the price to go through it.
    Stop,
}

/// A buy under the ask waits for the price to come down (a limit), above it for the price to
/// break up (a stop); a sell the other way round against the bid.
pub fn pending_kind(buy: bool, price: f64, bid: Option<f64>, ask: Option<f64>) -> Pending {
    let reference = if buy { ask } else { bid };
    match reference {
        Some(market) if buy && price < market => Pending::Limit,
        Some(market) if !buy && price > market => Pending::Limit,
        Some(_) => Pending::Stop,
        None => Pending::Limit,
    }
}

/// Why an order would be refused before it is sent. These are codes: the sentence a person reads
/// belongs to the screen that shows it.
#[derive(Debug, Clone, PartialEq)]
pub enum TicketProblem {
    /// A pending order without a price.
    NoPrice,
    /// The stop loss is on the profit side of the entry.
    StopLossWrongSide,
    /// The take profit is on the loss side of the entry.
    TakeProfitWrongSide,
    /// No volume to size by lots or units.
    NoVolume,
    NoRisk,
    /// Sizing by risk needs a stop loss, and it is off.
    StopLossOff,
    NoStopLoss,
    NoTakeProfit,
    /// The rate that turns the quote currency into the deposit currency is not known yet.
    WaitingForRate,
    NoMarginShare,
    /// The margin of one lot is not known yet.
    WaitingForMargin,
    /// The exits are targets in multiples of the risk, which needs a stop loss.
    ExitsNeedStopLoss,
    /// The exits are not a valid plan.
    Exits(super::plan::ExitsProblem),
    /// An OCO pair is made of two pending orders.
    OcoNeedsPending,
    /// A stop loss in ATR without an ATR for the chosen timeframe and bar.
    AtrUnavailable,
    /// A take profit in R without a stop loss to measure it from.
    RatioNeedsStopLoss,
    /// A good-till-date order without a date.
    NoExpiry,
    /// A stop limit order without the range of its limit.
    NoLimitRange,
    /// A market order with a slippage limit that has no value.
    NoSlippage,
    NoSymbol,
    /// The volume is too small to be cut between the exits.
    VolumeTooSmallForExits,
    /// The price of the other order of an OCO pair is not a price.
    OcoPriceInvalid,
}

/// Checks that the protection sits on the right side of the entry: under it for a buy's stop
/// loss and over it for its take profit, the other way round for a sell.
pub fn check_protection(
    buy: bool,
    entry: f64,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
) -> Result<(), TicketProblem> {
    if let Some(sl) = stop_loss
        && ((buy && sl >= entry) || (!buy && sl <= entry))
    {
        return Err(TicketProblem::StopLossWrongSide);
    }
    if let Some(tp) = take_profit
        && ((buy && tp <= entry) || (!buy && tp >= entry))
    {
        return Err(TicketProblem::TakeProfitWrongSide);
    }
    Ok(())
}

/// Which way a protection sits from the entry: `-1.0` under it, `1.0` over it.
pub fn protection_side(buy: bool, stop: bool) -> f64 {
    if buy == stop { -1.0 } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_price_makes_a_limit_or_a_stop_depending_on_the_side() {
        let (bid, ask) = (Some(1.1000), Some(1.1002));
        assert_eq!(pending_kind(true, 1.0990, bid, ask), Pending::Limit);
        assert_eq!(pending_kind(true, 1.1010, bid, ask), Pending::Stop);
        assert_eq!(pending_kind(false, 1.1010, bid, ask), Pending::Limit);
        assert_eq!(pending_kind(false, 1.0990, bid, ask), Pending::Stop);
        assert_eq!(pending_kind(true, 1.0, None, None), Pending::Limit);
    }

    #[test]
    fn protection_must_sit_on_the_right_side() {
        assert!(check_protection(true, 1.1, Some(1.09), Some(1.12)).is_ok());
        assert_eq!(
            check_protection(true, 1.1, Some(1.11), None),
            Err(TicketProblem::StopLossWrongSide)
        );
        assert_eq!(
            check_protection(false, 1.1, None, Some(1.11)),
            Err(TicketProblem::TakeProfitWrongSide)
        );
        assert!(check_protection(false, 1.1, Some(1.11), Some(1.08)).is_ok());
        assert!(check_protection(true, 1.1, None, None).is_ok());
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
