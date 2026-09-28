//! The arithmetic of trading: lots and volumes, pips, which pending order a price makes, the
//! profit of a position between two answers of the server, and the account's totals.
//!
//! # Units
//!
//! - A **volume** is what the server counts: hundredths of a unit of the base asset.
//! - A **lot** is `lot_size` of those (100 000 units of a currency pair, for example).
//! - **Prices** here are real (1.08412), like the prices of positions and orders on the wire.
//! - **Money** is in the account's deposit currency, as real numbers.

use crate::market::PRICE_SCALE;

/// How a symbol trades: its lot, the volumes it accepts and where its pip is.
///
/// ```
/// use wyck_openapi::trading::contract::{Contract, lots_for_risk};
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
pub fn relative_distance(distance: f64) -> i64 {
    (distance.abs() * PRICE_SCALE as f64).round() as i64
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

/// Why an order would be refused before it is sent.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TicketProblem {
    /// A pending order without a price.
    NoPrice,
    /// The stop loss is on the profit side of the entry.
    StopLossWrongSide,
    /// The take profit is on the loss side of the entry.
    TakeProfitWrongSide,
}

impl std::fmt::Display for TicketProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoPrice => "Set the price of the order",
            Self::StopLossWrongSide => "The stop loss is on the wrong side of the entry",
            Self::TakeProfitWrongSide => "The take profit is on the wrong side of the entry",
        })
    }
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

/// The lots whose loss over `stop_distance` is `risk` in the deposit currency, before they are
/// stepped. `rate` is the deposit currency per unit of quote currency.
pub fn lots_for_risk(risk: f64, stop_distance: f64, rate: f64, contract: &Contract) -> Option<f64> {
    let units_per_lot = contract.lot_size as f64 / 100.0;
    let loss_per_lot = stop_distance * rate * units_per_lot;
    (risk > 0.0 && loss_per_lot > 0.0 && loss_per_lot.is_finite()).then(|| risk / loss_per_lot)
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
    /// The symbol.
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
    fn a_distance_is_counted_in_the_servers_units() {
        assert_eq!(relative_distance(0.0025), 250);
        assert_eq!(relative_distance(-0.0025), 250);
    }

    #[test]
    fn the_profit_follows_the_price_at_the_rate_it_is_converted_at() {
        // Long 100 000 units from 1.1000; the server said +20 USD gross, +18 net.
        let units = 100_000.0;
        let quote_then = quote_profit(true, 1.1000, 1.1002, units);
        assert!((quote_then - 20.0).abs() < 1e-6);
        let mark = PnlMark {
            gross: 20.0,
            net: 18.0,
            rate: Some(1.0),
        };
        let now = quote_profit(true, 1.1000, 1.1010, units);
        assert!((live_net(&mark, now) - 98.0).abs() < 1e-6);
        // Profit in another currency: gross 10 for a quote profit of 20 is a rate of 0.5.
        let rate = implied_rate(10.0, 20.0, 1.0);
        assert_eq!(rate, Some(0.5));
        let converted = PnlMark {
            gross: 10.0,
            net: 9.0,
            rate,
        };
        assert!((live_net(&converted, 100.0) - 49.0).abs() < 1e-6);
        // Too small a move, or one of the other sign, says nothing about the rate.
        assert_eq!(implied_rate(0.3, 0.5, 1.0), None);
        assert_eq!(implied_rate(-10.0, 20.0, 1.0), None);
        let unknown = PnlMark {
            gross: 0.0,
            net: -2.0,
            rate: None,
        };
        assert!(close(live_net(&unknown, 50.0), -2.0));
        assert!(close(quote_profit(false, 1.1, 1.09, 10.0), 0.1));
    }

    #[test]
    fn the_totals_of_an_account_add_up() {
        let s = summary(10_000.0, 250.0, 500.0);
        assert!(close(s.equity, 10_250.0));
        assert!(close(s.free_margin, 9_750.0));
        assert!(close(s.margin_level.unwrap(), 2_050.0));
        assert_eq!(summary(1.0, 0.0, 0.0).margin_level, None);
    }

    #[test]
    fn lots_are_written_plainly() {
        assert_eq!(format_lots(0.10), "0.1");
        assert_eq!(format_lots(2.0), "2");
    }
    #[test]
    fn a_risk_makes_the_volume_that_loses_it_at_the_stop() {
        let c = Contract::default();
        // 100 USD over 20 pips of EURUSD: 10 USD a pip, so 0.5 lots.
        let lots = lots_for_risk(100.0, 0.0020, 1.0, &c).unwrap();
        assert!((lots - 0.5).abs() < 1e-9);
        assert_eq!(c.volume_at_most(lots).volume, 5_000_000);
        // Stepped down, never up: 0.567 lots is 0.56.
        assert_eq!(c.volume_at_most(0.567).volume, 5_600_000);
        assert_eq!(c.volume_at_most(0.3).volume, 3_000_000);
        assert_eq!(c.volume_at_most(0.001).limit, Some(Limit::Min));
        assert_eq!(c.volume_at_most(1e9).limit, Some(Limit::Max));
        assert_eq!(c.volume_near(0.567).volume, 5_700_000);
        assert_eq!(c.volume_near(0.567).limit, None);
        assert_eq!(c.volume_near(0.001).limit, Some(Limit::Min));
        assert_eq!(lots_for_risk(100.0, 0.0, 1.0, &c), None);
        assert_eq!(lots_for_risk(0.0, 0.002, 1.0, &c), None);
        // A quote currency worth half the deposit one needs twice the lots.
        let half = lots_for_risk(100.0, 0.0020, 0.5, &c).unwrap();
        assert!((half - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_conversion_chain_is_walked_either_way() {
        // Assets: 1 EUR, 2 USD, 3 JPY, 4 GBP. Symbols: 10 EURUSD, 11 USDJPY, 12 GBPUSD.
        let prices = |id: i64| match id {
            10 => Some(1.10),
            11 => Some(150.0),
            12 => Some(1.25),
            _ => None,
        };
        let eurusd = Link {
            symbol_id: 10,
            base: 1,
            quote: 2,
        };
        let usdjpy = Link {
            symbol_id: 11,
            base: 2,
            quote: 3,
        };
        let gbpusd = Link {
            symbol_id: 12,
            base: 4,
            quote: 2,
        };
        assert_eq!(chain_rate(2, 2, &[], &prices), Some(1.0));
        assert!((chain_rate(1, 2, &[eurusd], &prices).unwrap() - 1.10).abs() < 1e-12);
        // One JPY in USD, then in EUR.
        let jpy_usd = chain_rate(3, 2, &[usdjpy], &prices).unwrap();
        assert!((jpy_usd - 1.0 / 150.0).abs() < 1e-12);
        let jpy_eur = chain_rate(3, 1, &[usdjpy, eurusd], &prices).unwrap();
        assert!((jpy_eur - 1.0 / 150.0 / 1.10).abs() < 1e-12);
        let gbp_eur = chain_rate(4, 1, &[gbpusd, eurusd], &prices).unwrap();
        assert!((gbp_eur - 1.25 / 1.10).abs() < 1e-12);
        // A missing price, or a chain that leads elsewhere, gives no rate.
        let unpriced = Link {
            symbol_id: 99,
            base: 3,
            quote: 2,
        };
        assert_eq!(chain_rate(3, 2, &[unpriced], &prices), None);
        assert_eq!(chain_rate(3, 1, &[usdjpy], &prices), None);
        assert_eq!(mid(Some(1.0), Some(1.2)), Some(1.1));
        assert_eq!(mid(None, Some(1.2)), Some(1.2));
    }
}
