//! The arithmetic of trading: lots and volumes, pips, which pending order a price makes, the
//! profit of a position between two answers of the server, and the account's totals.
//!
//! # Units
//!
//! - A **volume** is what the server counts: hundredths of a unit of the base asset.
//! - A **lot** is `lot_size` of those (100 000 units of a currency pair, for example).
//! - **Prices** here are real (1.08412), like the prices of positions and orders on the wire.
//! - **Money** is in the account's deposit currency, as real numbers.

use wyck::openapi::market::PRICE_SCALE;

/// How a symbol trades: its lot, the volumes it accepts and where its pip is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contract {
    pub digits: u32,
    pub pip_position: i64,
    /// Hundredths of a unit per lot.
    pub lot_size: i64,
    pub min_volume: i64,
    pub max_volume: i64,
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
    pub fn from_symbol(symbol: &wyck::openapi::market::Symbol) -> Self {
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

    pub fn lots_of_volume(&self, volume: i64) -> f64 {
        volume as f64 / self.lot_size.max(1) as f64
    }

    /// The size of one pip in price.
    pub fn pip(&self) -> f64 {
        10f64.powi(-i32::try_from(self.pip_position).unwrap_or(4))
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
    let text = format!("{lots:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A price distance in the server's relative units (a stop loss or take profit of a market
/// order is given as a distance from where it fills).
pub fn relative_distance(distance: f64) -> i64 {
    (distance.abs() * PRICE_SCALE as f64).round() as i64
}

/// Which pending order a price makes for a side, given the market.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    Limit,
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
pub enum TicketProblem {
    NoPrice,
    StopLossWrongSide,
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

/// The totals of an account.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Summary {
    pub balance: f64,
    pub equity: f64,
    pub margin: f64,
    pub free_margin: f64,
    /// Equity over used margin, in percent; `None` without margin in use.
    pub margin_level: Option<f64>,
    pub unrealized: f64,
}

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

/// An amount of money with its currency: `1,234.56 USD`, `-12.30 EUR`.
pub fn format_money(amount: f64, currency: &str) -> String {
    let negative = amount < 0.0;
    let cents = (amount.abs() * 100.0).round() as i64;
    let (whole, rest) = (cents / 100, cents % 100);
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
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
    fn money_is_grouped_and_signed() {
        assert_eq!(format_money(1_234.5, "USD"), "1,234.50 USD");
        assert_eq!(format_money(-12.3, "EUR"), "-12.30 EUR");
        assert_eq!(format_money(-0.001, ""), "0.00");
        assert_eq!(format_money(1_000_000.0, ""), "1,000,000.00");
        assert_eq!(format_lots(0.10), "0.1");
        assert_eq!(format_lots(2.0), "2");
    }
}
