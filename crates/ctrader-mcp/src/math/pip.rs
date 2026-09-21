//! Pip distance <-> absolute price conversion, ported from `scripts/pip_math.py`.
//!
//! Operates on **display** prices throughout. Remote returns integer pipettes on market
//! data and DTO price fields (`Q-K19`): decode via [`crate::math::units::pipettes_to_price`]
//! before calling anything here, and re-encode the result afterward.

use crate::common::TradeSide;
use crate::math::NumericError;
use crate::math::round_half_up_to_digits;

/// SL or TP context for [`pips_to_price`] / polarity resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceLeg {
    StopLoss,
    TakeProfit,
}

/// Returns `+1.0` if pip movement adds to `reference_price`, `-1.0` if it subtracts, for
/// a given `(side, leg)` combination:
///
/// | Side | Leg         | Polarity |
/// |------|-------------|----------|
/// | Buy  | StopLoss    | -1       |
/// | Buy  | TakeProfit  | +1       |
/// | Sell | StopLoss    | +1       |
/// | Sell | TakeProfit  | -1       |
fn polarity(side: TradeSide, leg: PriceLeg) -> f64 {
    match (side, leg) {
        (TradeSide::Buy, PriceLeg::StopLoss) => -1.0,
        (TradeSide::Buy, PriceLeg::TakeProfit) => 1.0,
        (TradeSide::Sell, PriceLeg::StopLoss) => 1.0,
        (TradeSide::Sell, PriceLeg::TakeProfit) => -1.0,
    }
}

/// Converts a non-negative pip distance from `reference_price` into an absolute price,
/// rounded to `digits` decimal places.
///
/// `reference_price` is the entry price (for SL/TP placement) or the current quote;
/// `pip_size` is the symbol's price increment per pip (`0.0001` for most FX majors,
/// `0.01` for JPY pairs and `XAUUSD`, broker-dependent for indices: always read it from
/// `get_symbol_details` / `get_symbols`, never assume).
pub fn pips_to_price(
    reference_price: f64,
    pips: i64,
    pip_size: f64,
    side: TradeSide,
    leg: PriceLeg,
    digits: u32,
) -> f64 {
    let raw = reference_price + polarity(side, leg) * pips as f64 * pip_size;
    round_half_up_to_digits(raw, digits)
}

/// Converts an absolute `target_price` into a pip distance from `reference_price`.
///
/// Returns `(rounded_pips, raw_pips)`: the rounded integer distance (half-up) and the
/// unrounded distance, both non-negative (this function reports magnitude only: pair it
/// with the known `side`/`leg` context if the caller also needs to reconstruct
/// direction).
///
/// Returns an error for non-finite prices, an invalid pip size, or an
/// unrepresentable integer distance.
pub fn price_to_pips(
    reference_price: f64,
    target_price: f64,
    pip_size: f64,
) -> Result<(i64, f64), NumericError> {
    if !reference_price.is_finite() || !target_price.is_finite() {
        return Err(NumericError {
            field: "price",
            reason: "must be finite",
        });
    }
    if !pip_size.is_finite() || pip_size <= 0.0 {
        return Err(NumericError {
            field: "pip_size",
            reason: "must be finite and positive",
        });
    }
    let raw = (target_price - reference_price).abs() / pip_size;
    if !raw.is_finite() || raw >= i64::MAX as f64 {
        return Err(NumericError {
            field: "price",
            reason: "pip distance exceeds the wire integer range",
        });
    }
    Ok((raw.round() as i64, raw))
}

/// Given an entry price plus a risk (SL) and reward (TP) distance in pips, returns
/// `(stop_loss_price, take_profit_price)`.
pub fn sl_tp_from_risk_reward(
    entry: f64,
    risk_pips: i64,
    reward_pips: i64,
    pip_size: f64,
    side: TradeSide,
    digits: u32,
) -> (f64, f64) {
    let sl = pips_to_price(entry, risk_pips, pip_size, side, PriceLeg::StopLoss, digits);
    let tp = pips_to_price(
        entry,
        reward_pips,
        pip_size,
        side,
        PriceLeg::TakeProfit,
        digits,
    );
    (sl, tp)
}

/// Given an entry price plus absolute SL and TP prices, returns `(sl_pips, tp_pips)` as
/// non-negative integer distances.
pub fn sl_tp_to_pip_distances(
    entry: f64,
    sl_absolute: f64,
    tp_absolute: f64,
    pip_size: f64,
) -> Result<(i64, i64), NumericError> {
    let (sl_pips, _) = price_to_pips(entry, sl_absolute, pip_size)?;
    let (tp_pips, _) = price_to_pips(entry, tp_absolute, pip_size)?;
    Ok((sl_pips, tp_pips))
}

/// Converts a pip distance to Remote's integer POINTS encoding for
/// `relativeStopLoss`/`relativeTakeProfit` (see [`crate::quirks::market_with_relative_sl_tp`] and
/// `Q-R4`'s **P-REMOTE-MARKET-RELATIVE** pattern). 1 point = `1 / 10^pip_digits`; for a
/// 5-digit FX pair this is `points = pips * 10`, and it is the identity for a 4-digit
/// pair. `pip_digits` here is the symbol's *pipette* precision (from `get_symbols`), not
/// the pip-size itself.
pub fn pips_to_points(pips: i64, pip_digits: u32) -> Result<i64, NumericError> {
    // A "pip" is conventionally the second-to-last digit of the quoted price for FX
    // (i.e. one order of magnitude coarser than a pipette/point on 5- and 3-digit
    // pairs), so points-per-pip is 10 on those pairs and 1 on 2- and 4-digit pairs.
    let points_per_pip = if pip_digits >= 3 { 10 } else { 1 };
    if pips < 0 {
        return Err(NumericError {
            field: "pips",
            reason: "must be non-negative",
        });
    }
    pips.checked_mul(points_per_pip).ok_or(NumericError {
        field: "pips",
        reason: "point distance exceeds the wire integer range",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    // Ported from pip_math.py's `_self_test` fixtures.

    #[test]
    fn eurusd_long_sl_30_pips_below_1_0850() {
        let price = pips_to_price(1.0850, 30, 0.0001, TradeSide::Buy, PriceLeg::StopLoss, 5);
        assert!(close(price, 1.0820), "got {price}");
    }

    #[test]
    fn eurusd_long_tp_30_pips_above_1_0850() {
        let price = pips_to_price(1.0850, 30, 0.0001, TradeSide::Buy, PriceLeg::TakeProfit, 5);
        assert!(close(price, 1.0880), "got {price}");
    }

    #[test]
    fn usdjpy_short_sl_50_pips_above_150_30() {
        let price = pips_to_price(150.30, 50, 0.01, TradeSide::Sell, PriceLeg::StopLoss, 3);
        assert!(close(price, 150.80), "got {price}");
    }

    #[test]
    fn xauusd_long_tp_100_pips_above_1900_50() {
        let price = pips_to_price(1900.50, 100, 0.01, TradeSide::Buy, PriceLeg::TakeProfit, 2);
        assert!(close(price, 1901.50), "got {price}");
    }

    #[test]
    fn us500_long_sl_20_pips_below_5000() {
        let price = pips_to_price(5000.0, 20, 0.1, TradeSide::Buy, PriceLeg::StopLoss, 1);
        assert!(close(price, 4998.0), "got {price}");
    }

    #[test]
    fn price_to_pips_eurusd_entry_1_0850_sl_1_0820() {
        let (pips, _raw) = price_to_pips(1.0850, 1.0820, 0.0001).unwrap();
        assert_eq!(pips, 30);
    }

    #[test]
    fn zero_pip_distance_is_identity() {
        let price = pips_to_price(1.0850, 0, 0.0001, TradeSide::Buy, PriceLeg::StopLoss, 5);
        assert!(close(price, 1.0850), "got {price}");
    }

    #[test]
    fn points_conversion_matches_q_r4_worked_example() {
        // EURUSD 5-digit pair: 30 pips SL / 60 pips TP -> 300 / 600 points, per
        // self-healing-playbook.md's live-confirmed example.
        assert_eq!(pips_to_points(30, 5).unwrap(), 300);
        assert_eq!(pips_to_points(60, 5).unwrap(), 600);
    }

    #[test]
    fn invalid_pip_inputs_are_rejected() {
        assert!(price_to_pips(1.0, f64::NAN, 0.0001).is_err());
        assert!(price_to_pips(1.0, 2.0, 0.0).is_err());
        assert!(pips_to_points(-1, 5).is_err());
        assert!(pips_to_points(i64::MAX, 5).is_err());
    }
}
