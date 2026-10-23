//! The profit of a position between two answers of the server: the server's word, and the price
//! moving it until the next answer.

/// The profit of a position in the symbol's quote currency, at the price it would close at.
pub(crate) fn quote_profit(buy: bool, entry: f64, close: f64, units: f64) -> f64 {
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
pub(crate) fn implied_rate(gross: f64, quote: f64, least: f64) -> Option<f64> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
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
}
