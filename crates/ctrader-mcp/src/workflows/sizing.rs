//! **W5 — Risk sizing** (`SKILL.md` recipe 1, "Position sizing by risk %").
//!
//! Translates a risk percentage or flat risk amount plus a stop-loss distance into a
//! server-native volume, then validates the resulting position against the account's
//! margin so opening it doesn't bring `post_trade_margin_level` below 2x the broker's
//! stop-out level (`SKILL.md` "Stop-out and margin level").
//!
//! This module is deliberately I/O-free: fetching quotes (`get_spot_prices`) and
//! resolving the pip-value/conversion-rate/tier-curve inputs is the caller's
//! responsibility (via [`crate::math::conversion::compute_chain`] and the live account
//! state), which keeps the sizing *decision* itself a pure, synchronous, easily
//! unit-tested function.

use crate::error::CTraderError;
use crate::math::margin::{self, MarginTier, TieredMarginResult};
use crate::math::sizing::{self, SizingParams, SizingResult};

/// What the caller is targeting: a percentage of account balance, or a flat amount in
/// account currency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RiskTarget {
    Percent(f64),
    Amount(f64),
}

/// Sizing inputs resolved by the caller (typically from a fresh `get_symbol_details`/
/// `get_symbols` read plus [`crate::math::conversion::compute_chain`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiskInputs {
    pub sl_pips: i64,
    pub pip_value_per_lot: f64,
    pub conversion_rate: f64,
    pub lot_size: f64,
    pub volume_step: f64,
    pub min_volume: f64,
    pub max_volume: Option<f64>,
}

/// Margin-safety inputs resolved by the caller from live account state
/// (`get_balance`/`get_positions`) plus the broker's published leverage tier curve.
#[derive(Debug, Clone)]
pub struct MarginSafetyInputs {
    pub tiers: Vec<MarginTier>,
    /// USD value of one base-asset unit (the symbol's spot price for a directly
    /// USD-quoted symbol).
    pub quote_rate_usd: f64,
    /// `1 USD = rate` account-currency units; `1.0` for a USD account.
    pub account_currency_rate_vs_usd: f64,
    pub equity: f64,
    /// Currently used margin, in account currency, BEFORE this candidate position.
    pub used_margin_before: f64,
    /// The broker's stop-out level as a percentage (commonly `50.0` or `30.0`).
    pub stop_out_level_pct: f64,
}

/// The result of [`size_position_by_risk`].
#[derive(Debug, Clone)]
pub struct RiskSizingDecision {
    pub sizing: SizingResult,
    pub margin: TieredMarginResult,
    /// `None` when `used_margin_before + margin.margin_account_ccy == 0` — per `Q-L18`,
    /// this is the "no positions / unconstrained" sentinel, not a missing-data error;
    /// the stop-out check is vacuously satisfied in this case.
    pub post_trade_margin_level_pct: Option<f64>,
    /// `true` iff the position may be opened without breaching the 2x stop-out floor.
    /// `SKILL.md`'s W5 invariant: "the 2x stop-out buffer is the FLOOR — never allow
    /// opening that brings `post_trade_margin_level` below 2x the broker's stop-out
    /// level."
    pub approved: bool,
    /// Human-readable notices: rounding/clipping warnings from the sizing step, plus
    /// (if `approved` is `false`) the specific margin-floor breach.
    pub reasons: Vec<String>,
}

/// Computes a risk-sized volume and validates it against the account's post-trade
/// margin level, per `references/trader-workflows.md` W5 steps 1-5.
///
/// # Errors
///
/// Returns [`CTraderError::Invariant`] if the underlying sizing or margin computation
/// rejects its inputs (see [`crate::math::sizing::SizingError`] and
/// [`crate::math::margin::MarginTierError`] for the specific conditions).
pub fn size_position_by_risk(
    balance: f64,
    target: RiskTarget,
    risk: &RiskInputs,
    margin_safety: &MarginSafetyInputs,
) -> Result<RiskSizingDecision, CTraderError> {
    let sizing_params = SizingParams {
        sl_pips: risk.sl_pips,
        pip_value_per_lot: risk.pip_value_per_lot,
        conversion_rate: risk.conversion_rate,
        lot_size: risk.lot_size,
        min_volume_step: risk.volume_step,
        min_volume: risk.min_volume,
        max_volume: risk.max_volume,
    };

    let sizing_result = match target {
        RiskTarget::Percent(pct) => sizing::from_risk_percent(balance, pct, &sizing_params),
        RiskTarget::Amount(amount) => sizing::from_risk_amount(amount, &sizing_params),
    }
    .map_err(|source| CTraderError::Invariant(source.to_string()))?;

    let margin_result = margin::compute_tiered_margin(
        sizing_result.units,
        margin_safety.quote_rate_usd,
        &margin_safety.tiers,
        margin_safety.account_currency_rate_vs_usd,
    )
    .map_err(|source| CTraderError::Invariant(source.to_string()))?;

    let used_margin_after = margin_safety.used_margin_before + margin_result.margin_account_ccy;
    let post_trade_margin_level_pct =
        (used_margin_after > 0.0).then(|| (margin_safety.equity / used_margin_after) * 100.0);

    let stop_out_floor_pct = margin_safety.stop_out_level_pct * 2.0;
    let mut reasons = sizing_result.warnings.clone();
    let approved = match post_trade_margin_level_pct {
        None => true,
        Some(level) if level >= stop_out_floor_pct => true,
        Some(level) => {
            reasons.push(format!(
                "would bring margin level to {level:.1}%, below the 2x stop-out floor of {stop_out_floor_pct:.1}% — refusing"
            ));
            false
        }
    };

    Ok(RiskSizingDecision {
        sizing: sizing_result,
        margin: margin_result,
        post_trade_margin_level_pct,
        approved,
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> RiskInputs {
        RiskInputs {
            sl_pips: 30,
            pip_value_per_lot: 10.0,
            conversion_rate: 1.0,
            lot_size: 100_000.0,
            volume_step: 1.0,
            min_volume: 0.0,
            max_volume: None,
        }
    }

    #[test]
    fn approves_when_plenty_of_margin_headroom() {
        let margin_safety = MarginSafetyInputs {
            tiers: vec![MarginTier {
                upper_usd: None,
                leverage: 500,
            }],
            quote_rate_usd: 1.0850,
            account_currency_rate_vs_usd: 1.0,
            equity: 1_000_000.0,
            used_margin_before: 0.0,
            stop_out_level_pct: 50.0,
        };
        let decision = size_position_by_risk(
            10_000.0,
            RiskTarget::Percent(1.0),
            &inputs(),
            &margin_safety,
        )
        .unwrap();
        assert!(decision.approved, "reasons: {:?}", decision.reasons);
    }

    #[test]
    fn refuses_when_margin_level_would_breach_2x_stop_out_floor() {
        // Tiny equity relative to the sized position's margin requirement forces a
        // breach of the 2x-stop-out floor.
        let margin_safety = MarginSafetyInputs {
            tiers: vec![MarginTier {
                upper_usd: None,
                leverage: 2,
            }],
            quote_rate_usd: 1.0850,
            account_currency_rate_vs_usd: 1.0,
            equity: 100.0,
            used_margin_before: 0.0,
            stop_out_level_pct: 50.0,
        };
        let decision = size_position_by_risk(
            10_000.0,
            RiskTarget::Percent(1.0),
            &inputs(),
            &margin_safety,
        )
        .unwrap();
        assert!(!decision.approved);
        assert!(!decision.reasons.is_empty());
    }

    #[test]
    fn no_used_margin_is_vacuously_approved() {
        let margin_safety = MarginSafetyInputs {
            tiers: vec![MarginTier {
                upper_usd: None,
                leverage: 500,
            }],
            quote_rate_usd: 0.0, // degenerate: zero notional, zero margin
            account_currency_rate_vs_usd: 1.0,
            equity: 100.0,
            used_margin_before: 0.0,
            stop_out_level_pct: 50.0,
        };
        let decision = size_position_by_risk(
            10_000.0,
            RiskTarget::Amount(50.0),
            &inputs(),
            &margin_safety,
        )
        .unwrap();
        assert_eq!(decision.post_trade_margin_level_pct, None);
        assert!(decision.approved);
    }
}
