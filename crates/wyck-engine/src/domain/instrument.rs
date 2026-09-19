//! Tradable instruments, normalized across brokers.

use serde::{Deserialize, Serialize};

use super::volume::Volume;

/// Where an instrument's volume rules came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SpecsSource {
    /// Published by the broker for this symbol.
    Broker,
    /// Not published, so the engine assumed them from
    /// [`AssumedSpecs`](crate::config::AssumedSpecs). Risk sizing still works but the lot
    /// size, minimum and step may not match the broker's, and the planner warns.
    Assumed,
}

/// Volume rules for one instrument.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolumeSpecs {
    /// Base-asset units per lot.
    pub lot_size: f64,
    /// Smallest tradable volume.
    pub min: Volume,
    /// Volume increment.
    pub step: Volume,
    /// Largest tradable volume, when the broker publishes one.
    pub max: Option<Volume>,
}

/// One tradable symbol, in a shape that does not depend on which server it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instrument {
    /// The human ticker, e.g. `"EURUSD"`. Case as the broker reports it.
    pub symbol: String,
    /// The broker's numeric id. Present on Remote (`symbolId`), absent on Local.
    pub symbol_id: Option<i64>,
    /// Decimal places of a quoted price (`5` for EURUSD, `3` for USDJPY). On Remote this
    /// is `pipDigits`, the pipette precision.
    pub price_digits: u32,
    /// The size of one pip in price terms (`0.0001` for EURUSD, `0.01` for USDJPY).
    pub pip_size: f64,
    /// Base currency code, when known.
    pub base_currency: Option<String>,
    /// Quote currency code, when known.
    pub quote_currency: Option<String>,
    /// Whether the broker currently allows trading it.
    pub enabled: bool,
    /// Volume rules.
    pub volume: VolumeSpecs,
    /// Whether `volume` is the broker's or an assumption.
    pub specs_source: SpecsSource,
}

impl Instrument {
    /// Rounds a volume down to the instrument's step. Never rounds up: rounding a
    /// risk-sized volume up would exceed the risk that was asked for.
    #[must_use]
    pub fn round_volume_down(&self, volume: Volume) -> Volume {
        volume.round_down_to(self.volume.step)
    }

    /// Checks a volume against the instrument's minimum, maximum and step.
    ///
    /// # Errors
    ///
    /// A message naming the rule that failed.
    pub fn check_volume(&self, volume: Volume) -> Result<(), String> {
        let v = &self.volume;
        if !volume.is_positive() {
            return Err("volume must be positive".to_owned());
        }
        if volume < v.min {
            return Err(format!(
                "{volume} is below the minimum of {} for {}",
                v.min, self.symbol
            ));
        }
        if let Some(max) = v.max
            && volume > max
        {
            return Err(format!(
                "{volume} is above the maximum of {max} for {}",
                self.symbol
            ));
        }
        if !volume.is_multiple_of(v.step) {
            return Err(format!(
                "{volume} is not a multiple of the {} step for {}",
                v.step, self.symbol
            ));
        }
        Ok(())
    }

    /// The price distance of `pips` pips.
    #[must_use]
    pub fn pips_to_distance(&self, pips: f64) -> f64 {
        pips * self.pip_size
    }

    /// The number of pips in a price distance (absolute value).
    #[must_use]
    pub fn distance_to_pips(&self, distance: f64) -> f64 {
        distance.abs() / self.pip_size
    }

    /// Rounds a price to the instrument's decimal places.
    #[must_use]
    pub fn round_price(&self, price: f64) -> f64 {
        let factor = 10f64.powi(i32::try_from(self.price_digits).unwrap_or(8));
        (price * factor).round() / factor
    }
}

#[cfg(test)]
pub(crate) mod testing {
    #![allow(dead_code)]
    use super::*;

    pub(crate) fn eurusd() -> Instrument {
        Instrument {
            symbol: "EURUSD".to_owned(),
            symbol_id: Some(1),
            price_digits: 5,
            pip_size: 0.0001,
            base_currency: Some("EUR".to_owned()),
            quote_currency: Some("USD".to_owned()),
            enabled: true,
            volume: VolumeSpecs {
                lot_size: 100_000.0,
                min: Volume::from_units(1_000),
                step: Volume::from_units(1_000),
                max: Some(Volume::from_units(100_000_000)),
            },
            specs_source: SpecsSource::Broker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::eurusd;
    use super::*;

    #[test]
    fn volume_checks_name_the_broken_rule() {
        let i = eurusd();
        assert!(i.check_volume(Volume::from_units(10_000)).is_ok());
        assert!(
            i.check_volume(Volume::ZERO)
                .unwrap_err()
                .contains("positive")
        );
        assert!(
            i.check_volume(Volume::from_units(500))
                .unwrap_err()
                .contains("minimum")
        );
        assert!(
            i.check_volume(Volume::from_units(200_000_000))
                .unwrap_err()
                .contains("maximum")
        );
        assert!(
            i.check_volume(Volume::from_units(10_500))
                .unwrap_err()
                .contains("multiple")
        );
    }

    #[test]
    fn rounding_never_goes_up() {
        let i = eurusd();
        assert_eq!(
            i.round_volume_down(Volume::from_units(12_999)),
            Volume::from_units(12_000)
        );
    }

    #[test]
    fn pip_and_price_helpers() {
        let i = eurusd();
        assert!((i.pips_to_distance(30.0) - 0.0030).abs() < 1e-12);
        assert!((i.distance_to_pips(-0.0025) - 25.0).abs() < 1e-9);
        assert!((i.round_price(1.085_004_9) - 1.08500).abs() < 1e-12);
    }
}
