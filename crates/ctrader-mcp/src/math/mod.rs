//! Pure, network-free conversion and sizing routines.
//!
//! This module is a faithful Rust port of the skill's Python `scripts/` (`pip_math.py`,
//! `units_encoding.py`, `tiered_margin.py`, `conversion_rate.py`, and
//! `position_sizing.py`) so a workflow (see [`crate::workflows`]) never has to shell out
//! to a subprocess to do this math. Every public function here is a pure computation: no
//! I/O, no `async`, safe to call from anywhere (including a hot-path UI thread).
//!
//! ## A note on numeric precision
//!
//! The Python originals use [`decimal.Decimal`] with 28 significant digits specifically
//! to avoid floating-point rounding drift on money and price values. This module uses
//! `f64` instead, to avoid pulling a big-decimal crate into the workspace for a domain
//! (FX/CFD/metals/indices position sizing) where `f64`'s ~15-17 significant decimal
//! digits comfortably exceed any realistic account balance, lot size, or price
//! precision. `round_half_up_to_digits` follows `ROUND_HALF_UP` for non-negative inputs.
//! `round_down_to_step` uses a small quotient tolerance to recover intended whole
//! steps lost to binary floating-point drift, so it differs slightly from strict
//! decimal `ROUND_DOWN` at that boundary. If accounting or reconciliation needs
//! exact decimals, replace `f64` with `rust_decimal::Decimal` inside this module.
//!
//! [`decimal.Decimal`]: https://docs.python.org/3/library/decimal.html

pub mod conversion;
pub mod margin;
pub mod pip;
pub mod sizing;
pub mod units;

/// Rounds `value` to `digits` decimal places, half-away-from-zero (matches Python's
/// `Decimal.quantize(..., rounding=ROUND_HALF_UP)` for the non-negative values every
/// caller in this crate passes).
pub(crate) fn round_half_up_to_digits(value: f64, digits: u32) -> f64 {
    let factor = 10f64.powi(digits as i32);
    (value * factor).round() / factor
}

/// Rounds `value` down to the nearest multiple of `step`, except when the quotient
/// is less than 1e-9 below the next integer due to floating-point drift. Used by
/// `Q-L1`-aware volume rounding and the sizing/margin routines.
///
/// # Panics
///
/// Panics if `step` is not strictly positive or `value` is negative: both are
/// programming errors at every call site in this crate (never user input passed through
/// unchecked), matching the Python original's `ValueError` behavior translated to a
/// debug-time invariant.
pub(crate) fn round_down_to_step(value: f64, step: f64) -> f64 {
    debug_assert!(
        step > 0.0,
        "round_down_to_step: step must be > 0, got {step}"
    );
    debug_assert!(
        value >= 0.0,
        "round_down_to_step: value must be >= 0, got {value}"
    );
    // Recover quotients just below an integer due to binary floating-point drift.
    // A quotient genuinely more than 1e-9 below a step still rounds down.
    ((value / step) + 1e-9).floor() * step
}
