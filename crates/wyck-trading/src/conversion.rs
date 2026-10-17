//! Turning one asset into another through the chain of symbols the server gives, for a profit or
//! a margin in a currency that is not the symbol's.

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
        (Some(b), Some(a)) => Some(f64::midpoint(b, a)),
        (b, a) => b.or(a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
