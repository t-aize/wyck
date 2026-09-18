//! Pip distance <-> absolute price conversion, ported from `scripts/pip_math.py`.
//!
//! Operates on **display** prices throughout. Remote returns integer pipettes on market
//! data and DTO price fields (`Q-K19`) — decode via [`crate::math::units::pipettes_to_price`]
//! before calling anything here, and re-encode the result afterward.

use crate::common::TradeSide;
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
/// | Buy  | StopLoss    | −1       |
/// | Buy  | TakeProfit  | +1       |
/// | Sell | StopLoss    | +1       |
/// | Sell | TakeProfit  | −1       |
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
/// `0.01` for JPY pairs and `XAUUSD`, broker-dependent for indices — always read it from
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
/// unrounded distance, both non-negative (this function reports magnitude only — pair it
/// with the known `side`/`leg` context if the caller also needs to reconstruct
/// direction).
///
/// # Panics
///
/// Panics if `pip_size` is not strictly positive.
pub fn price_to_pips(reference_price: f64, target_price: f64, pip_size: f64) -> (i64, f64) {
    assert!(
        pip_size > 0.0,
        "price_to_pips: pip_size must be > 0, got {pip_size}"
    );
    let raw = (target_price - reference_price).abs() / pip_size;
    (raw.round() as i64, raw)
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
) -> (i64, i64) {
    let (sl_pips, _) = price_to_pips(entry, sl_absolute, pip_size);
    let (tp_pips, _) = price_to_pips(entry, tp_absolute, pip_size);
    (sl_pips, tp_pips)
}

/// Converts a pip distance to Remote's integer POINTS encoding for
/// `relativeStopLoss`/`relativeTakeProfit` (see [`crate::quirks::market_relative`] and
/// `Q-R4`'s **P-REMOTE-MARKET-RELATIVE** pattern). 1 point = `1 / 10^pip_digits`; for a
/// 5-digit FX pair this is `points = pips * 10`, and it is the identity for a 4-digit
/// pair. `pip_digits` here is the symbol's *pipette* precision (from `get_symbols`), not
/// the pip-size itself.
pub fn pips_to_points(pips: i64, pip_digits: u32) -> i64 {
    // A "pip" is conventionally the second-to-last digit of the quoted price for FX
    // (i.e. one order of magnitude coarser than a pipette/point on 5- and 3-digit
    // pairs), so points-per-pip is 10 on those pairs and 1 on 2- and 4-digit pairs.
    let points_per_pip = if pip_digits >= 3 { 10 } else { 1 };
    pips * points_per_pip
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
        let (pips, _raw) = price_to_pips(1.0850, 1.0820, 0.0001);
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
        assert_eq!(pips_to_points(30, 5), 300);
        assert_eq!(pips_to_points(60, 5), 600);
    }
}
