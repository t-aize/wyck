//! Trade volume as an exact integer.

use std::fmt;
use std::ops::{Add, Sub};

use ctrader_mcp::math::units;
use serde::{Deserialize, Serialize};

/// A trade volume in **base-asset units** (for EURUSD, euros).
///
/// An integer on purpose: volumes are compared, summed and rounded to a step all the
/// time, and floating point is the wrong tool for any of that. Lots are a *display* unit
/// (`units / lot_size`), converted at the edges with [`Volume::from_lots`] and
/// [`Volume::to_lots`]. Remote's wire unit ("cents") is `units * 100`, see
/// [`Volume::to_cents`].
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Volume(i64);

impl Volume {
    /// No volume.
    pub const ZERO: Self = Self(0);

    /// A volume of `units` base-asset units.
    #[must_use]
    pub const fn from_units(units: i64) -> Self {
        Self(units)
    }

    /// The volume in base-asset units.
    #[must_use]
    pub const fn units(self) -> i64 {
        self.0
    }

    /// Converts a lot count to units, rounding to the nearest unit.
    #[must_use]
    pub fn from_lots(lots: f64, lot_size: f64) -> Self {
        Self(units::lots_to_units(lots, lot_size))
    }

    /// The volume as a lot count for a symbol with the given lot size.
    #[must_use]
    pub fn to_lots(self, lot_size: f64) -> f64 {
        units::units_to_lots(self.0, lot_size)
    }

    /// Remote's wire encoding (units times 100).
    #[must_use]
    pub fn to_cents(self) -> i64 {
        units::units_to_cents(self.0)
    }

    /// Decodes Remote's wire encoding.
    #[must_use]
    pub fn from_cents(cents: i64) -> Self {
        Self(units::cents_to_units(cents))
    }

    /// Whether this is a strictly positive volume.
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// Rounds down to a whole multiple of `step` (which must be positive; a
    /// non-positive step returns the volume unchanged).
    #[must_use]
    pub const fn round_down_to(self, step: Self) -> Self {
        if step.0 <= 0 {
            return self;
        }
        Self(self.0 - self.0.rem_euclid(step.0))
    }

    /// Whether this volume is a whole multiple of `step`.
    #[must_use]
    pub const fn is_multiple_of(self, step: Self) -> bool {
        step.0 > 0 && self.0 % step.0 == 0
    }
}

impl Add for Volume {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Volume {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl fmt::Display for Volume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} units", self.0)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn lots_and_units_convert() {
        assert_eq!(
            Volume::from_lots(0.5, 100_000.0),
            Volume::from_units(50_000)
        );
        assert!((Volume::from_units(20_000).to_lots(100_000.0) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn cents_are_units_times_one_hundred() {
        assert_eq!(Volume::from_units(1_000).to_cents(), 100_000);
        assert_eq!(Volume::from_cents(100_000), Volume::from_units(1_000));
    }

    #[test]
    fn rounding_down_to_a_step() {
        let step = Volume::from_units(1_000);
        assert_eq!(
            Volume::from_units(12_999).round_down_to(step),
            Volume::from_units(12_000)
        );
        assert_eq!(
            Volume::from_units(12_000).round_down_to(step),
            Volume::from_units(12_000)
        );
        assert_eq!(Volume::from_units(999).round_down_to(step), Volume::ZERO);
        assert_eq!(
            Volume::from_units(5).round_down_to(Volume::ZERO),
            Volume::from_units(5)
        );
    }

    #[test]
    fn arithmetic_saturates_instead_of_overflowing() {
        let max = Volume::from_units(i64::MAX);
        assert_eq!(max + Volume::from_units(1), max);
        assert_eq!(
            Volume::from_units(i64::MIN) - Volume::from_units(1),
            Volume::from_units(i64::MIN)
        );
    }

    proptest! {
        #[test]
        fn rounded_volume_is_a_multiple_never_larger_and_within_one_step(
            units in 0i64..10_000_000_000,
            step in 1i64..1_000_000,
        ) {
            let v = Volume::from_units(units);
            let s = Volume::from_units(step);
            let r = v.round_down_to(s);
            prop_assert!(r.is_multiple_of(s));
            prop_assert!(r <= v);
            prop_assert!(v.units() - r.units() < step);
        }

        #[test]
        fn cents_round_trip(units in -1_000_000_000i64..1_000_000_000) {
            let v = Volume::from_units(units);
            prop_assert_eq!(Volume::from_cents(v.to_cents()), v);
        }
    }
}
