//! The arithmetic of trading: lots and volumes, pips, which pending order a price makes, the
//! profit of a position between two answers of the server, and the account's totals.
//!
//! # Units
//!
//! - A **volume** is what the server counts: hundredths of a unit of the base asset.
//! - A **lot** is `lot_size` of those (100 000 units of a currency pair, for example).
//! - **Prices** here are real (1.08412), like the prices of positions and orders on the wire.
//! - **Money** is in the account's deposit currency, as real numbers.

use crate::domain::market::PRICE_SCALE;

/// How a symbol trades: its lot, the volumes it accepts and where its pip is.
///
/// ```
/// use wyck::domain::trading::contract::{Contract, lots_for_risk};
///
/// // A forex pair: 100 000 units a lot, steps of 0.01 lot, pips at the fourth decimal.
/// let eurusd = Contract::default();
/// assert_eq!(eurusd.volume_of_lots(0.5), 5_000_000);
/// assert!((eurusd.pips(0.0020) - 20.0).abs() < 1e-9);
///
/// // Risk 100 of the deposit currency over a 20 pip stop, quote currency = deposit currency.
/// let lots = lots_for_risk(100.0, 0.0020, 1.0, &eurusd).unwrap();
/// assert_eq!(eurusd.volume_of_lots(lots), 5_000_000);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
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
    pub fn from_symbol(symbol: &crate::domain::market::Symbol) -> Self {
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
    pub fn volume_of_lots(&self, lots: f64) -> i64 {
        if !lots.is_finite() || lots <= 0.0 {
            return self.min_volume;
        }
        let raw = lots * self.lot_size as f64;
        let step = self.step_volume.max(1) as f64;
        let stepped = ((raw / step).round() * step) as i64;
        stepped.clamp(self.min_volume, self.max_volume.max(self.min_volume))
    }

    /// The lots a volume makes.
    pub fn lots_of_volume(&self, volume: i64) -> f64 {
        volume as f64 / self.lot_size.max(1) as f64
    }

    /// The size of one pip in price.
    pub fn pip(&self) -> f64 {
        crate::domain::market::pip_size(self.pip_position)
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
}

/// Lots written plainly: `0.01`, `1.5`, `10`.
pub fn format_lots(lots: f64) -> String {
    let mut text = format!("{lots:.2}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    text
}

/// A price distance in the server's relative units (a stop loss or take profit of a market
/// order is given as a distance from where it fills).
///
/// The server takes only multiples of the smallest step of the symbol's price, so the distance
/// is rounded to `digits` decimals: on a three digit symbol the units go by hundreds. A distance
/// is never rounded down to nothing.
pub fn relative_distance(distance: f64, digits: u32) -> i64 {
    let step = 10i64.pow(PRICE_DIGITS.saturating_sub(digits.min(PRICE_DIGITS)));
    let units = (distance.abs() * PRICE_SCALE as f64 / step as f64).round() as i64;
    units.max(1) * step
}

/// The decimals of the server's price scale (`PRICE_SCALE` is ten to this).
const PRICE_DIGITS: u32 = 5;

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
#[non_exhaustive]
pub enum TicketProblem {
    /// A pending order without a price.
    NoPrice,
    /// The stop loss is on the profit side of the entry.
    StopLossWrongSide,
    /// The take profit is on the loss side of the entry.
    TakeProfitWrongSide,
    /// No volume to size by lots or units.
    NoVolume,
    /// No risk to size by.
    NoRisk,
    /// Sizing by risk needs a stop loss, and it is off.
    StopLossOff,
    /// The stop loss has no value.
    NoStopLoss,
    /// The take profit has no value.
    NoTakeProfit,
    /// The rate that turns the quote currency into the deposit currency is not known yet.
    WaitingForRate,
    /// No share of the free margin to size by.
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
    /// No symbol is chosen.
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

/// The profit of a position in the symbol's quote currency, at the price it would close at.
pub fn quote_profit(buy: bool, entry: f64, close: f64, units: f64) -> f64 {
    if buy {
        (close - entry) * units
    } else {
        (entry - close) * units
    }
}

/// The server's word on a position's profit, and the rate that turns a profit in the quote
/// currency into the deposit currency, so the profit can follow the price until the next answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PnlMark {
    /// Before and after commission and swap, in the deposit currency.
    pub gross: f64,
    /// See [`PnlMark::gross`].
    pub net: f64,
    /// Deposit currency per unit of quote currency, when it is known.
    pub rate: Option<f64>,
}

/// The rate a server answer implies: its gross profit over the profit in the quote currency
/// computed here at about the same time. Only a move of at least `least` (a pip's worth) is
/// trusted, since the two are not taken at the same instant.
pub fn implied_rate(gross: f64, quote: f64, least: f64) -> Option<f64> {
    if quote.abs() < least.max(1e-9) {
        return None;
    }
    let rate = gross / quote;
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

/// The profit of a position now: the profit in the quote currency at the current market,
/// converted at the mark's rate, less the costs the server counted. Without a rate, the
/// server's answer as it is.
pub fn live_net(mark: &PnlMark, quote_now: f64) -> f64 {
    match mark.rate {
        Some(rate) => quote_now * rate + (mark.net - mark.gross),
        None => mark.net,
    }
}

/// Which way a protection sits from the entry: `-1.0` under it, `1.0` over it.
pub fn protection_side(buy: bool, stop: bool) -> f64 {
    if buy == stop { -1.0 } else { 1.0 }
}

/// The quantity that loses `risk` over a move of `distance` in price, when one unit of quantity
/// is worth `value` in the risk's currency for each unit of price moved. `None` when these
/// numbers cannot size anything. This is the one formula: the ticket (through [`lots_for_risk`])
/// and the position tool of the chart both size from it.
pub fn quantity_for_risk(risk: f64, distance: f64, value: f64) -> Option<f64> {
    let per_unit = distance * value;
    (risk > 0.0 && per_unit > 0.0 && per_unit.is_finite()).then(|| risk / per_unit)
}

/// What `quantity` gains or loses over a move of `distance` in price; see [`quantity_for_risk`].
pub fn move_amount(quantity: f64, distance: f64, value: f64) -> f64 {
    quantity * distance * value
}

/// The lots whose loss over `stop_distance` is `risk` in the deposit currency, before they are
/// stepped. `rate` is the deposit currency per unit of quote currency.
pub fn lots_for_risk(risk: f64, stop_distance: f64, rate: f64, contract: &Contract) -> Option<f64> {
    let units_per_lot = contract.lot_size as f64 / 100.0;
    quantity_for_risk(risk, stop_distance, rate * units_per_lot)
}

/// A volume chosen by the ticket, and whether the broker's limits changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stepped {
    /// The volume to send.
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

impl Contract {
    /// The volume of `lots` rounded to the nearest step and kept inside the broker's limits.
    pub fn volume_near(&self, lots: f64) -> Stepped {
        let volume = self.volume_of_lots(lots);
        let raw = lots * self.lot_size as f64;
        let limit = if raw < self.min_volume as f64 - 0.5 {
            Some(Limit::Min)
        } else if raw > self.max_volume.max(self.min_volume) as f64 + 0.5 {
            Some(Limit::Max)
        } else {
            None
        };
        Stepped { volume, limit }
    }

    /// The volume of `lots` stepped down, so a volume sized from a risk never risks more, and
    /// kept inside the broker's limits.
    pub fn volume_at_most(&self, lots: f64) -> Stepped {
        let raw = if lots.is_finite() { lots.max(0.0) } else { 0.0 } * self.lot_size as f64;
        let step = self.step_volume.max(1) as f64;
        // A hair of tolerance, so 0.3 lots is not stepped down to 0.29 by the float error.
        let stepped = ((raw / step + 1e-9).floor() * step) as i64;
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

/// A symbol of a conversion chain, with the assets it trades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub symbol_id: i64,
    /// The asset it prices.
    pub base: i64,
    /// The asset it is priced in.
    pub quote: i64,
}

/// How much of asset `to` one unit of asset `from` is worth, walking the chain the server gave
/// at the middle prices of its symbols. `None` while a price is missing or the chain does not
/// lead from one to the other.
pub fn chain_rate(
    from: i64,
    to: i64,
    chain: &[Link],
    mid: &dyn Fn(i64) -> Option<f64>,
) -> Option<f64> {
    let mut asset = from;
    let mut rate = 1.0;
    for link in chain {
        let price = mid(link.symbol_id).filter(|p| *p > 0.0)?;
        if link.base == asset {
            rate *= price;
            asset = link.quote;
        } else if link.quote == asset {
            rate /= price;
            asset = link.base;
        } else {
            return None;
        }
    }
    (asset == to).then_some(rate)
}

/// The middle of a bid and an ask, or whichever is known.
pub fn mid(bid: Option<f64>, ask: Option<f64>) -> Option<f64> {
    match (bid, ask) {
        (Some(b), Some(a)) => Some((b + a) / 2.0),
        (b, a) => b.or(a),
    }
}

/// The totals of an account.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Summary {
    /// The balance, without the open positions.
    pub balance: f64,
    /// The balance with the open positions' profit.
    pub equity: f64,
    /// The margin the open positions use.
    pub margin: f64,
    /// The equity left over the margin in use.
    pub free_margin: f64,
    /// Equity over used margin, in percent; `None` without margin in use.
    pub margin_level: Option<f64>,
    /// The open positions' profit or loss.
    pub unrealized: f64,
}

/// The totals of an account from its balance, its open profit and the margin in use.
pub fn summary(balance: f64, unrealized: f64, margin: f64) -> Summary {
    let equity = balance + unrealized;
    Summary {
        balance,
        equity,
        margin,
        free_margin: equity - margin,
        margin_level: (margin > 0.0).then(|| equity / margin * 100.0),
        unrealized,
    }
}

#[cfg(test)]
mod tests;
