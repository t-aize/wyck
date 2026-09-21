//! Display units <-> server wire encoding conversions, ported from
//! `scripts/units_encoding.py`.
//!
//! `--lot-size` is **always required** on every lots-based conversion here (never
//! defaulted to `100_000`): broker lot sizes vary: Remote forex is typically `100000`
//! base-asset units per lot, but Local ICMarkets has been observed returning `lotSize: 1`
//! for `EURUSD` (`Q-L1`). Always read the live `lotSize` from `get_symbol_details`
//! (Local) or the symbol's static metadata (Remote) before calling these functions.

use crate::math::NumericError;
use crate::math::round_down_to_step;

fn checked_volume(lots: f64, lot_size: f64, scale: f64) -> Result<i64, NumericError> {
    if !lots.is_finite() || lots < 0.0 {
        return Err(NumericError {
            field: "lots",
            reason: "must be finite and non-negative",
        });
    }
    if !lot_size.is_finite() || lot_size <= 0.0 {
        return Err(NumericError {
            field: "lot_size",
            reason: "must be finite and positive",
        });
    }
    let value = lots * lot_size * scale;
    if !value.is_finite() || value >= i64::MAX as f64 {
        return Err(NumericError {
            field: "lots",
            reason: "volume exceeds the wire integer range",
        });
    }
    let rounded = round_down_to_step(value, 1.0);
    if rounded >= i64::MAX as f64 {
        return Err(NumericError {
            field: "lots",
            reason: "volume exceeds the wire integer range",
        });
    }
    Ok(rounded as i64)
}

/// Converts display lots to Local-server integer base-asset units: `floor(lots *
/// lot_size)`. Floor, not round, because partial units are not representable on the
/// wire.
pub fn lots_to_units(lots: f64, lot_size: f64) -> Result<i64, NumericError> {
    checked_volume(lots, lot_size, 1.0)
}

/// Converts Local-server integer units back to display lots.
pub fn units_to_lots(units: i64, lot_size: f64) -> f64 {
    debug_assert!(lot_size > 0.0);
    units as f64 / lot_size
}

/// Converts display lots to Remote-server integer cents: `floor(lots * lot_size * 100)`.
/// Remote cents are 100x the Local units for the same lot count (`SKILL.md` "Units
/// conventions across the two servers").
pub fn lots_to_cents(lots: f64, lot_size: f64) -> Result<i64, NumericError> {
    checked_volume(lots, lot_size, 100.0)
}

/// Converts Remote cents to the equivalent Local units (`cents / 100`, integer
/// division).
pub fn cents_to_units(cents: i64) -> i64 {
    cents / 100
}

/// Converts Local units to the equivalent Remote cents (`units * 100`).
pub fn units_to_cents(units: i64) -> Result<i64, NumericError> {
    if units < 0 {
        return Err(NumericError {
            field: "units",
            reason: "must be non-negative",
        });
    }
    units.checked_mul(100).ok_or(NumericError {
        field: "units",
        reason: "volume exceeds the wire integer range",
    })
}

/// Decodes a Remote money integer to its display value (`raw / 10^money_digits`). Thin
/// re-export point over [`crate::common::money_from_raw`] kept here so every
/// `units_encoding.py` subcommand has a same-named Rust counterpart.
pub fn display_money(raw: i64, money_digits: u32) -> f64 {
    crate::common::money_from_raw(raw, money_digits)
}

/// Encodes a display money value to Remote's integer encoding (`round(display *
/// 10^money_digits)`, half-up).
pub fn parse_money(display: f64, money_digits: u32) -> i64 {
    crate::common::money_to_raw(display, money_digits)
}

/// Decodes a Remote pipette integer to a display price (`Q-K19`).
pub fn pipettes_to_price(pipettes: i64, pip_digits: u32) -> f64 {
    crate::common::price_from_pipettes(pipettes, pip_digits)
}

/// Encodes a display price to Remote's integer pipettes.
pub fn price_to_pipettes(price: f64, pip_digits: u32) -> i64 {
    crate::common::price_to_pipettes(price, pip_digits)
}

/// Rounds a candidate volume (in base-asset units, or cents: the step and value must be
/// in the same unit) DOWN to the nearest multiple of `volume_step`, per the
/// `self-healing-playbook.md` §1.4 pre-flight gate ("round to the nearest valid step in
/// the direction of the user's intent (typically toward smaller risk)").
pub fn round_volume_down_to_step(volume: f64, volume_step: f64) -> f64 {
    round_down_to_step(volume, volume_step)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from units_encoding.py's `_self_test` fixtures.

    #[test]
    fn zero_point_one_lot_forex_to_units() {
        assert_eq!(lots_to_units(0.1, 100_000.0).unwrap(), 10_000);
    }

    #[test]
    fn floating_point_drift_does_not_lose_a_unit() {
        assert_eq!(lots_to_units(0.29, 100_000.0).unwrap(), 29_000);
        assert_eq!(lots_to_cents(0.29, 100_000.0).unwrap(), 2_900_000);
        assert_eq!(round_volume_down_to_step(2.9 - 1e-6, 1.0), 2.0);
    }

    #[test]
    fn invalid_or_overflowing_volumes_are_rejected() {
        assert!(lots_to_units(f64::NAN, 100_000.0).is_err());
        assert!(lots_to_cents(1.0, f64::INFINITY).is_err());
        assert!(lots_to_cents(1e100, 100_000.0).is_err());
        assert!(units_to_cents(i64::MAX).is_err());
    }

    #[test]
    fn zero_point_one_lot_forex_to_cents() {
        assert_eq!(lots_to_cents(0.1, 100_000.0).unwrap(), 1_000_000);
    }

    #[test]
    fn xauusd_half_lot_lot_size_100_to_units() {
        assert_eq!(lots_to_units(0.5, 100.0).unwrap(), 50);
    }

    #[test]
    fn us500_tenth_lot_lot_size_1_to_cents() {
        assert_eq!(lots_to_cents(0.1, 1.0).unwrap(), 10);
    }

    #[test]
    fn units_to_lots_round_trip() {
        assert!((units_to_lots(10_000, 100_000.0) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn cents_units_round_trip() {
        assert_eq!(cents_to_units(10_000_000), 100_000);
        assert_eq!(units_to_cents(100_000).unwrap(), 10_000_000);
    }

    #[test]
    fn money_round_trip() {
        assert!((display_money(1_234_567, 2) - 12_345.67).abs() < 1e-6);
        assert_eq!(parse_money(100.5, 2), 10_050);
        assert_eq!(parse_money(12_345.67, 2), 1_234_567);
    }

    #[test]
    fn pipette_round_trip() {
        assert_eq!(price_to_pipettes(1.0850, 5), 108_500);
        assert!((pipettes_to_price(108_500, 5) - 1.085).abs() < 1e-9);
    }

    #[test]
    fn xauusd_smallest_money_unit_round_trip() {
        assert_eq!(parse_money(0.01, 2), 1);
    }
}
