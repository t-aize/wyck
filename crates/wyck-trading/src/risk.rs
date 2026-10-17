//! Sizing an order by the money it may lose.

use crate::contract::Contract;
use wyck_core::math::i64_to_f64;

/// The quantity that loses `risk` over a move of `distance` in price, when one unit of quantity
/// is worth `value` in the risk's currency for each unit of price moved. `None` when these
/// numbers cannot size anything. This is the one formula: the ticket (through [`lots_for_risk`])
/// and the position tool of the chart both size from it.
pub fn quantity_for_risk(risk: f64, distance: f64, value: f64) -> Option<f64> {
    let per_unit = distance * value;
    (risk > 0.0 && per_unit > 0.0 && per_unit.is_finite()).then(|| risk / per_unit)
}

/// What `quantity` gains or loses over a move of `distance` in price; see [`quantity_for_risk`].
pub fn move_amount(quantity: f64, distance: f64, value: f64) -> f64 {
    quantity * distance * value
}

/// The lots whose loss over `stop_distance` is `risk` in the deposit currency, before they are
/// stepped. `rate` is the deposit currency per unit of quote currency.
pub fn lots_for_risk(risk: f64, stop_distance: f64, rate: f64, contract: &Contract) -> Option<f64> {
    let units_per_lot = i64_to_f64(contract.lot_size) / 100.0;
    quantity_for_risk(risk, stop_distance, rate * units_per_lot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Limit;

    #[test]
    fn the_quantity_for_a_risk_and_what_it_loses_are_the_same_formula() {
        let (risk, distance, value) = (100.0, 0.0020, 50_000.0);
        let quantity = quantity_for_risk(risk, distance, value).unwrap();
        assert!((move_amount(quantity, distance, value) - risk).abs() < 1e-9);
        for bad in [
            (0.0, 1.0, 1.0),
            (1.0, 0.0, 1.0),
            (1.0, 1.0, 0.0),
            (1.0, f64::NAN, 1.0),
        ] {
            assert_eq!(quantity_for_risk(bad.0, bad.1, bad.2), None, "{bad:?}");
        }
    }

    #[test]
    fn a_risk_makes_the_volume_that_loses_it_at_the_stop() {
        let c = Contract::default();
        // 100 USD over 20 pips of EURUSD: 10 USD a pip, so 0.5 lots.
        let lots = lots_for_risk(100.0, 0.0020, 1.0, &c).unwrap();
        assert!((lots - 0.5).abs() < 1e-9);
        assert_eq!(c.volume_at_most(lots).volume, 5_000_000);
        // Stepped down, never up: 0.567 lots is 0.56.
        assert_eq!(c.volume_at_most(0.567).volume, 5_600_000);
        assert_eq!(c.volume_at_most(0.3).volume, 3_000_000);
        assert_eq!(c.volume_at_most(0.001).limit, Some(Limit::Min));
        assert_eq!(c.volume_at_most(1e9).limit, Some(Limit::Max));
        assert_eq!(c.volume_near(0.567).volume, 5_700_000);
        assert_eq!(c.volume_near(0.567).limit, None);
        assert_eq!(c.volume_near(0.001).limit, Some(Limit::Min));
        assert_eq!(lots_for_risk(100.0, 0.0, 1.0, &c), None);
        assert_eq!(lots_for_risk(0.0, 0.002, 1.0, &c), None);
        // A quote currency worth half the deposit one needs twice the lots.
        let half = lots_for_risk(100.0, 0.0020, 0.5, &c).unwrap();
        assert!((half - 1.0).abs() < 1e-9);
    }
}
