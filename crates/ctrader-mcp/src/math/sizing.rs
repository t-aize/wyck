//! Risk-based position sizing, ported from `scripts/position_sizing.py`.
//!
//! Translates a risk percentage (or a flat risk amount) plus a stop-loss distance into a
//! server-native volume figure (units for Local, cents for Remote), rounding DOWN to the
//! symbol's `volumeStep` so the sized position never exceeds the requested risk budget
//! (`SKILL.md` W5 invariant: "Risk amount is the UPPER BOUND: the script rounds DOWN
//! volumes to respect `volumeStep`").

use crate::math::round_down_to_step;

/// Parameters shared by [`from_risk_percent`] and [`from_risk_amount`], all in the
/// symbol's quote currency / base-asset units unless noted otherwise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizingParams {
    /// Stop-loss distance in pips (must be > 0).
    pub sl_pips: i64,
    /// Pip value per lot in the symbol's quote currency (must be > 0).
    pub pip_value_per_lot: f64,
    /// Multiplier converting quote currency to account currency: resolve with
    /// [`crate::math::conversion::compute_chain`] when the two differ. `1.0` if they
    /// match.
    pub conversion_rate: f64,
    /// Base-asset units per lot: always read live from `get_symbol_details`
    /// (`Q-L1`: broker-dependent, sometimes `1` rather than the FX-market convention of
    /// `100_000`).
    pub lot_size: f64,
    /// Minimum volume increment, in base-asset units.
    pub min_volume_step: f64,
    /// Minimum allowed volume, in base-asset units.
    pub min_volume: f64,
    /// Maximum allowed volume, in base-asset units (`None` = unlimited).
    pub max_volume: Option<f64>,
}

/// The result of a sizing computation.
#[derive(Debug, Clone, PartialEq)]
pub struct SizingResult {
    /// Server-native volume for the Local server (base-asset units).
    pub units: i64,
    /// Server-native volume for the Remote server (`units * 100`, rounded
    /// independently to respect cents-scale stepping).
    pub cents: i64,
    /// The resulting size in display lots.
    pub lots: f64,
    /// The actual risk (in account currency) the rounded volume represents: may differ
    /// slightly from the requested risk amount due to step rounding or min/max
    /// clipping; see `warnings` when it does.
    pub risk_currency_amount: f64,
    /// Human-readable notices about rounding, min/max clipping, or risk-budget changes.
    /// Never empty on a clipped result; always empty on a clean division.
    pub warnings: Vec<String>,
}

/// A sizing input was invalid (non-positive where a positive value is required).
#[derive(Debug, thiserror::Error, PartialEq)]
#[error("invalid sizing parameter `{field}`: {reason}")]
pub struct SizingError {
    pub field: &'static str,
    pub reason: &'static str,
}

fn invalid(field: &'static str, reason: &'static str) -> SizingError {
    SizingError { field, reason }
}

fn validate_params(params: &SizingParams) -> Result<(), SizingError> {
    if params.sl_pips <= 0 {
        return Err(invalid("sl_pips", "must be > 0"));
    }
    if params.pip_value_per_lot <= 0.0 {
        return Err(invalid("pip_value_per_lot", "must be > 0"));
    }
    if params.conversion_rate <= 0.0 {
        return Err(invalid("conversion_rate", "must be > 0"));
    }
    if params.lot_size <= 0.0 {
        return Err(invalid("lot_size", "must be > 0"));
    }
    if params.min_volume_step <= 0.0 {
        return Err(invalid("min_volume_step", "must be > 0"));
    }
    if params.min_volume < 0.0 {
        return Err(invalid("min_volume", "must be >= 0"));
    }
    Ok(())
}

/// Core sizing routine shared by [`from_risk_percent`] and [`from_risk_amount`].
fn size_from_risk_amount(
    risk_amount: f64,
    params: &SizingParams,
) -> Result<SizingResult, SizingError> {
    validate_params(params)?;

    let mut warnings = Vec::new();
    let pip_value_per_lot_account = params.pip_value_per_lot * params.conversion_rate;
    let target_lots = risk_amount / (params.sl_pips as f64 * pip_value_per_lot_account);
    let target_units_raw = target_lots * params.lot_size;

    let mut target_units_rounded = round_down_to_step(target_units_raw, params.min_volume_step);
    let rounding_loss = target_units_raw - target_units_rounded;
    if rounding_loss >= params.min_volume_step {
        warnings.push(format!(
            "volume rounded down from {target_units_raw} to {target_units_rounded} due to volume step {}",
            params.min_volume_step
        ));
    }

    if target_units_rounded < params.min_volume {
        let original = target_units_rounded;
        target_units_rounded = params.min_volume;
        warnings.push(format!(
            "computed volume {original} below minimum {}; clipped to minimum",
            params.min_volume
        ));
        let actual_lots = target_units_rounded / params.lot_size;
        let actual_risk = actual_lots * params.sl_pips as f64 * pip_value_per_lot_account;
        if actual_risk > risk_amount {
            warnings.push(format!(
                "clipping to min_volume increases risk from {risk_amount} to {actual_risk} in account currency"
            ));
        }
    }

    if let Some(max_volume) = params.max_volume
        && target_units_rounded > max_volume
    {
        let original = target_units_rounded;
        target_units_rounded = max_volume;
        warnings.push(format!(
            "computed volume {original} above maximum {max_volume}; clipped to maximum"
        ));
    }

    let final_lots = target_units_rounded / params.lot_size;
    let actual_risk_amount = final_lots * params.sl_pips as f64 * pip_value_per_lot_account;
    let units_int = target_units_rounded as i64;

    let cent_lot_size = params.lot_size * 100.0;
    let target_cents_raw = target_lots * cent_lot_size;
    let mut target_cents_rounded = round_down_to_step(target_cents_raw, 1.0);
    if params.min_volume > 0.0 && target_cents_rounded < params.min_volume * 100.0 {
        target_cents_rounded = params.min_volume * 100.0;
    }
    if let Some(max_volume) = params.max_volume
        && target_cents_rounded > max_volume * 100.0
    {
        target_cents_rounded = max_volume * 100.0;
    }
    let cents_int = target_cents_rounded as i64;

    Ok(SizingResult {
        units: units_int,
        cents: cents_int,
        lots: final_lots,
        risk_currency_amount: actual_risk_amount,
        warnings,
    })
}

/// Sizes a position from a risk percentage of account `balance` (e.g. "risk 1% of my
/// 10,000 USD account").
///
/// # Errors
///
/// Returns [`SizingError`] if `balance <= 0`, `risk_pct` is outside `(0, 100)`, or any
/// field in `params` is invalid.
pub fn from_risk_percent(
    balance: f64,
    risk_pct: f64,
    params: &SizingParams,
) -> Result<SizingResult, SizingError> {
    if balance <= 0.0 {
        return Err(invalid("balance", "must be > 0"));
    }
    if risk_pct <= 0.0 || risk_pct >= 100.0 {
        return Err(invalid("risk_pct", "must be > 0 and < 100"));
    }
    let risk_amount = balance * (risk_pct / 100.0);
    size_from_risk_amount(risk_amount, params)
}

/// Sizes a position from an explicit risk amount in account currency (e.g. "risk $100").
///
/// # Errors
///
/// Returns [`SizingError`] if `risk_amount <= 0` or any field in `params` is invalid.
pub fn from_risk_amount(
    risk_amount: f64,
    params: &SizingParams,
) -> Result<SizingResult, SizingError> {
    if risk_amount <= 0.0 {
        return Err(invalid("risk_amount", "must be > 0"));
    }
    size_from_risk_amount(risk_amount, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    fn default_params(sl_pips: i64, pip_value_per_lot: f64, conversion_rate: f64) -> SizingParams {
        SizingParams {
            sl_pips,
            pip_value_per_lot,
            conversion_rate,
            lot_size: 100_000.0,
            min_volume_step: 1.0,
            min_volume: 0.0,
            max_volume: None,
        }
    }

    // Ported from position_sizing.py's `_self_test` fixtures.

    #[test]
    fn eurusd_one_percent_risk_on_10000_skill_md_row_2() {
        let result = from_risk_percent(10_000.0, 1.0, &default_params(30, 10.0, 1.0)).unwrap();
        assert_eq!(result.units, 33_333);
        assert_eq!(result.cents, 3_333_333);
        assert!(close(result.risk_currency_amount, 99.999, 1e-3));
    }

    #[test]
    fn xauusd_two_percent_risk_on_5000_lot_size_100() {
        let params = SizingParams {
            sl_pips: 100,
            pip_value_per_lot: 1.0,
            conversion_rate: 1.0,
            lot_size: 100.0,
            min_volume_step: 1.0,
            min_volume: 0.0,
            max_volume: None,
        };
        let result = from_risk_percent(5_000.0, 2.0, &params).unwrap();
        assert!(close(result.lots, 1.0, 1e-6));
        assert_eq!(result.units, 100);
        assert_eq!(result.cents, 10_000);
        assert!(close(result.risk_currency_amount, 100.0, 1e-6));
    }

    #[test]
    fn us500_fixed_50_risk_lot_size_1() {
        let params = SizingParams {
            sl_pips: 20,
            pip_value_per_lot: 1.0,
            conversion_rate: 1.0,
            lot_size: 1.0,
            min_volume_step: 1.0,
            min_volume: 0.0,
            max_volume: None,
        };
        let result = from_risk_amount(50.0, &params).unwrap();
        assert_eq!(result.units, 2);
        assert_eq!(result.cents, 250);
        assert!(close(result.risk_currency_amount, 40.0, 1e-6));
    }

    #[test]
    fn cross_currency_eur_account_conversion_rate_0_85() {
        let result = from_risk_percent(10_000.0, 1.0, &default_params(30, 10.0, 0.85)).unwrap();
        assert_eq!(result.units, 39_215);
        assert_eq!(result.cents, 3_921_568);
    }

    #[test]
    fn zero_balance_is_rejected() {
        let err = from_risk_percent(0.0, 1.0, &default_params(30, 10.0, 1.0)).unwrap_err();
        assert_eq!(err.field, "balance");
    }

    #[test]
    fn clipping_to_min_volume_warns_when_risk_increases() {
        let params = SizingParams {
            sl_pips: 30,
            pip_value_per_lot: 10.0,
            conversion_rate: 1.0,
            lot_size: 100_000.0,
            min_volume_step: 1.0,
            min_volume: 50_000.0,
            max_volume: None,
        };
        // A tiny risk amount sizes far below min_volume, forcing a clip.
        let result = from_risk_amount(1.0, &params).unwrap();
        assert_eq!(result.units, 50_000);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("clipped to minimum"))
        );
        assert!(result.warnings.iter().any(|w| w.contains("increases risk")));
    }
}
