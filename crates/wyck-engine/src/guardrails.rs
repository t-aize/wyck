//! Guardrails: non-blocking warnings about a plan or the account.
//!
//! The rule is the one the README states for prop-firm limits: **warn, never block**. Nothing
//! here can refuse an order. A guardrail turns a fact ("this trade risks 3.4% of your
//! balance") into a sentence the front end shows next to the
//! order, and the person decides. The only things that stop an order are structural: the
//! engine is not armed, the request is invalid, the session cannot trade.
//!
//! Every function is pure, so the thresholds are easy to test.

use crate::config::GuardrailConfig;
use crate::domain::{AccountSnapshot, Instrument, Position, UnixMillis};
use crate::risk::OrderPlan;

/// Risk carried by open positions, computed only where it can be computed honestly.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OpenRisk {
    /// Sum of `distance to stop x volume` in account currency, over the positions that
    /// have a stop loss and a quote currency equal to the account currency.
    pub amount: f64,
    /// Positions that could not be included (no stop, other quote currency, no instrument).
    pub unknown_positions: usize,
}

/// Estimates the loss at the stop of every open position.
///
/// Conservative in the sense that matters: it never invents a number. A position whose risk
/// cannot be computed without an exchange rate is counted in `unknown_positions` instead of
/// being converted with a guess.
#[must_use]
pub fn estimate_open_risk<'a>(
    positions: &[Position],
    instrument_of: impl Fn(&str) -> Option<&'a Instrument>,
    account_currency: Option<&str>,
) -> OpenRisk {
    let mut risk = OpenRisk::default();
    for p in positions {
        let (Some(entry), Some(stop)) = (p.entry_price, p.stop_loss) else {
            risk.unknown_positions += 1;
            continue;
        };
        let same_currency = instrument_of(&p.symbol)
            .and_then(|i| i.quote_currency.as_deref())
            .zip(account_currency)
            .is_some_and(|(q, a)| q.eq_ignore_ascii_case(a));
        if !same_currency {
            risk.unknown_positions += 1;
            continue;
        }
        risk.amount += (entry - stop).abs() * p.volume.as_units();
    }
    risk
}

/// Warnings for a plan, as sentences.
#[must_use]
pub fn plan_warnings(
    plan: &OrderPlan,
    config: &GuardrailConfig,
    account: Option<&AccountSnapshot>,
    open_risk: OpenRisk,
    now: UnixMillis,
) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(pct) = plan.risk_percent
        && pct > config.max_risk_percent_per_trade
    {
        out.push(format!(
            "this trade risks {pct:.2}% of the balance, above your limit of {:.2}% per trade",
            config.max_risk_percent_per_trade
        ));
    }

    if let (Some(new_risk), Some(balance)) = (plan.risk_amount, account.and_then(|a| a.balance))
        && balance > 0.0
    {
        let total_pct = (open_risk.amount + new_risk) / balance * 100.0;
        if total_pct > config.max_total_risk_percent {
            out.push(format!(
                "open risk plus this trade would be {total_pct:.2}% of the balance, above your limit of {:.2}%",
                config.max_total_risk_percent
            ));
        }
        if open_risk.unknown_positions > 0 {
            out.push(format!(
                "{} open position(s) have no computable stop risk, so the total above may be understated",
                open_risk.unknown_positions
            ));
        }
    }

    match account {
        None => out.push("no account data is available".to_owned()),
        Some(a) => {
            let age_ms = now.saturating_sub(a.captured_at);
            if u128::try_from(age_ms).unwrap_or(0) > config.stale_account_after.as_millis() {
                out.push(format!("the account data is {} s old", age_ms / 1000));
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccountKind, Side, SpecsSource, Volume, VolumeSpecs};
    use crate::ids::{AccountId, PositionId};
    use crate::risk::PlanId;

    fn plan(risk_amount: Option<f64>, risk_percent: Option<f64>) -> OrderPlan {
        OrderPlan {
            id: PlanId::next(),
            account: AccountId::new("a"),
            symbol: "EURUSD".into(),
            side: Side::Buy,
            volume: Volume::from_units(10_000),
            entry_reference: 1.0,
            stop_loss_distance: Some(0.003),
            take_profit_distance: None,
            stop_loss_price: None,
            take_profit_price: None,
            risk_amount,
            risk_percent,
            account_currency: Some("USD".into()),
            spread: 0.0001,
            specs_source: SpecsSource::Broker,
            warnings: vec![],
            created_at: 0,
        }
    }

    fn account(balance: f64, captured_at: i64) -> AccountSnapshot {
        AccountSnapshot {
            account_id: AccountId::new("a"),
            kind: AccountKind::Demo,
            currency: Some("USD".into()),
            balance: Some(balance),
            equity: Some(balance),
            free_margin: Some(balance),
            used_margin: None,
            margin_level_pct: None,
            server_version: None,
            captured_at,
        }
    }

    fn cfg() -> GuardrailConfig {
        GuardrailConfig::default()
    }

    #[test]
    fn a_modest_trade_on_fresh_data_produces_no_warnings() {
        let acct = account(10_000.0, 1_000);
        let w = plan_warnings(
            &plan(Some(100.0), Some(1.0)),
            &cfg(),
            Some(&acct),
            OpenRisk::default(),
            2_000,
        );
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn per_trade_and_total_risk_limits_warn_but_are_only_sentences() {
        let acct = account(10_000.0, 1_000);
        let w = plan_warnings(
            &plan(Some(300.0), Some(3.0)),
            &cfg(),
            Some(&acct),
            OpenRisk {
                amount: 300.0,
                unknown_positions: 0,
            },
            2_000,
        );
        assert!(w.iter().any(|m| m.contains("per trade")));
        assert!(w.iter().any(|m| m.contains("open risk plus")));
    }

    #[test]
    fn understated_totals_are_called_out() {
        let acct = account(10_000.0, 1_000);
        let w = plan_warnings(
            &plan(Some(100.0), Some(1.0)),
            &cfg(),
            Some(&acct),
            OpenRisk {
                amount: 0.0,
                unknown_positions: 2,
            },
            2_000,
        );
        assert!(w.iter().any(|m| m.contains("understated")));
    }

    #[test]
    fn stale_or_missing_account_data_warns() {
        let old = account(10_000.0, 0);
        let w = plan_warnings(
            &plan(None, None),
            &cfg(),
            Some(&old),
            OpenRisk::default(),
            60_000,
        );
        assert!(w.iter().any(|m| m.contains("60 s old")));
        let w = plan_warnings(&plan(None, None), &cfg(), None, OpenRisk::default(), 0);
        assert!(w.iter().any(|m| m.contains("no account data")));
    }

    #[test]
    fn open_risk_only_counts_what_it_can_convert_honestly() {
        let usd_quote = Instrument {
            symbol: "EURUSD".into(),
            symbol_id: None,
            price_digits: 5,
            pip_size: 0.0001,
            base_currency: Some("EUR".into()),
            quote_currency: Some("USD".into()),
            enabled: true,
            volume: VolumeSpecs {
                lot_size: 100_000.0,
                min: Volume::from_units(1),
                step: Volume::from_units(1),
                max: None,
            },
            specs_source: SpecsSource::Broker,
        };
        let mut jpy_quote = usd_quote.clone();
        jpy_quote.symbol = "USDJPY".into();
        jpy_quote.quote_currency = Some("JPY".into());
        let mk = |id: i64, symbol: &str, stop: Option<f64>| Position {
            id: PositionId(id),
            symbol: symbol.into(),
            side: Side::Buy,
            volume: Volume::from_units(10_000),
            entry_price: Some(1.1000),
            stop_loss: stop,
            take_profit: None,
            swap: None,
            commission: None,
            unrealized_pnl: None,
            label: None,
        };
        let positions = [
            mk(1, "EURUSD", Some(1.0970)),
            mk(2, "USDJPY", Some(1.0)),
            mk(3, "EURUSD", None),
        ];
        let lookup = |s: &str| match s {
            "EURUSD" => Some(&usd_quote),
            "USDJPY" => Some(&jpy_quote),
            _ => None,
        };
        let risk = estimate_open_risk(&positions, lookup, Some("USD"));
        assert!(
            (risk.amount - 30.0).abs() < 1e-6,
            "0.0030 * 10,000 = 30 USD, got {}",
            risk.amount
        );
        assert_eq!(
            risk.unknown_positions, 2,
            "the JPY-quoted one and the one without a stop"
        );
    }
}
