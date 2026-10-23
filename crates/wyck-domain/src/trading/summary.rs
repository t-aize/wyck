//! The totals of an account: balance, equity, margin.

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
    fn the_totals_of_an_account_add_up() {
        let s = summary(10_000.0, 250.0, 500.0);
        assert!(close(s.equity, 10_250.0));
        assert!(close(s.free_margin, 9_750.0));
        assert!(close(s.margin_level.unwrap(), 2_050.0));
        assert_eq!(summary(1.0, 0.0, 0.0).margin_level, None);
    }
}
