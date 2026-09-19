//! Trade volume as an exact integer.

use std::fmt;
use std::ops::{Add, Sub};

use serde::{Deserialize, Serialize};

/// Hundredths of a unit per unit: the resolution of a [`Volume`].
const SCALE: i64 = 100;

/// A trade volume in **base-asset units** (for EURUSD, euros; for BTCUSD, bitcoins), kept to
/// a resolution of one hundredth of a unit.
///
/// An integer on purpose: volumes are compared, summed and rounded to a step all the
/// time, and floating point is the wrong tool for any of that. The hundredth-of-a-unit
/// resolution is Remote's wire unit ("cents", see [`Volume::to_cents`]) and is what lets a
/// crypto symbol with a minimum of 0.01 of a coin be represented. Lots are a *display*
/// unit (`units / lot_size`), converted at the edges with [`Volume::from_lots`] and
/// [`Volume::to_lots`].
///
/// Serialized as the integer count of hundredths.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Volume(i64);

impl Volume {
    /// No volume.
    pub const ZERO: Self = Self(0);

    /// A volume of `units` whole base-asset units.
    #[must_use]
    pub const fn from_units(units: i64) -> Self {
        Self(units.saturating_mul(SCALE))
    }

    /// A volume of `units` base-asset units, rounded to the nearest hundredth. A negative
    /// or non-finite input gives [`Volume::ZERO`].
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn from_units_f64(units: f64) -> Self {
        Self::from_hundredths(units * SCALE as f64)
    }

    /// The whole units of this volume, dropping any fraction. Use [`Volume::as_units`] for
    /// arithmetic.
    #[must_use]
    pub const fn units(self) -> i64 {
        self.0 / SCALE
    }

    /// The volume in base-asset units as a float, for price arithmetic and display.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_units(self) -> f64 {
        self.0 as f64 / SCALE as f64
    }

    /// Converts a lot count to a volume, rounding to the nearest hundredth of a unit.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn from_lots(lots: f64, lot_size: f64) -> Self {
        Self::from_hundredths(lots * lot_size * SCALE as f64)
    }

    /// The volume as a lot count for a symbol with the given lot size.
    #[must_use]
    pub fn to_lots(self, lot_size: f64) -> f64 {
        debug_assert!(lot_size > 0.0);
        self.as_units() / lot_size
    }

    /// Remote's wire encoding: hundredths of a unit.
    #[must_use]
    pub const fn to_cents(self) -> i64 {
        self.0
    }

    /// Decodes Remote's wire encoding.
    #[must_use]
    pub const fn from_cents(cents: i64) -> Self {
        Self(cents)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn from_hundredths(value: f64) -> Self {
        if value.is_finite() && value > 0.0 {
            Self(value.round().min(9.0e18) as i64)
        } else {
            Self::ZERO
        }
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
        let (whole, part) = (self.0 / SCALE, (self.0 % SCALE).abs());
        match part {
            0 => write!(f, "{whole} units"),
            p if p % 10 == 0 => write!(f, "{whole}.{} units", p / 10),
            p => write!(f, "{whole}.{p:02} units"),
        }
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
    fn fractional_units_are_kept_to_the_hundredth() {
        // BTCUSD on the demo servers: lot size 1, minimum and step 0.01.
        let v = Volume::from_units_f64(0.01);
        assert!(v.is_positive());
        assert_eq!(v.to_cents(), 1);
        assert_eq!(Volume::from_lots(0.07, 1.0).to_cents(), 7);
        assert!((Volume::from_cents(250).as_units() - 2.5).abs() < 1e-12);
        assert_eq!(Volume::from_cents(250).units(), 2);
        assert_eq!(Volume::from_units_f64(f64::NAN), Volume::ZERO);
        assert_eq!(Volume::from_units_f64(-1.0), Volume::ZERO);
    }

    #[test]
    fn cents_are_units_times_one_hundred() {
        assert_eq!(Volume::from_units(1_000).to_cents(), 100_000);
        assert_eq!(Volume::from_cents(100_000), Volume::from_units(1_000));
    }

    #[test]
    fn display_trims_the_fraction() {
        assert_eq!(Volume::from_units(1_000).to_string(), "1000 units");
        assert_eq!(Volume::from_cents(1).to_string(), "0.01 units");
        assert_eq!(Volume::from_cents(250).to_string(), "2.5 units");
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
        let coin_step = Volume::from_cents(1);
        assert_eq!(
            Volume::from_units_f64(0.0123).round_down_to(coin_step),
            Volume::from_cents(1)
        );
    }

    #[test]
    fn arithmetic_saturates_instead_of_overflowing() {
        let max = Volume::from_cents(i64::MAX);
        assert_eq!(max + Volume::from_units(1), max);
        assert_eq!(
            Volume::from_cents(i64::MIN) - Volume::from_units(1),
            Volume::from_cents(i64::MIN)
        );
        assert_eq!(Volume::from_units(i64::MAX), Volume::from_cents(i64::MAX));
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
            prop_assert!(v.to_cents() - r.to_cents() < s.to_cents());
        }

        #[test]
        fn cents_round_trip(units in -1_000_000_000i64..1_000_000_000) {
            let v = Volume::from_units(units);
            prop_assert_eq!(Volume::from_cents(v.to_cents()), v);
        }

        #[test]
        fn hundredths_survive_the_float_round_trip(cents in 0i64..1_000_000_000_000) {
            let v = Volume::from_cents(cents);
            prop_assert_eq!(Volume::from_units_f64(v.as_units()), v);
        }
    }
}
