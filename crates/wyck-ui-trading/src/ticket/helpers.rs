//! The small arithmetic and formats the ticket shares.

use wyck_core::math::{f64_to_i64, f64_to_u32, i64_to_f64};
use wyck_trading::contract::Contract;
use wyck_trading::ticket::{Offset, SizeMode};
use wyck_ui_kit::number;

/// The margin of a side: the buy one or the sell one.
pub(super) fn side_of(margins: (f64, f64), buy: bool) -> f64 {
    if buy { margins.0 } else { margins.1 }
}

/// A distance in pips as the points the server counts: the smallest step of the price.
pub(super) fn slippage_points(pips: f64, contract: &Contract) -> i32 {
    let digits = i32::try_from(contract.digits).unwrap_or(5);
    let points = f64_to_u32((pips * contract.pip() * 10f64.powi(digits)).round());
    i32::try_from(points).unwrap_or(i32::MAX)
}

/// Where the limit of a stop limit sits: past the stop by the range, in the direction the order
/// trades.
pub(super) fn stop_limit_price(buy: bool, stop: f64, range: f64) -> f64 {
    if buy { stop + range } else { stop - range }
}

/// What the size field holds in a mode.
pub(super) fn size_kind(mode: SizeMode, contract: &Contract) -> number::Kind {
    match mode {
        SizeMode::Lots => number::Kind::Volume {
            step: contract.lots_of_volume(contract.step_volume.max(1)),
        },
        SizeMode::Units => number::Kind::Volume {
            step: i64_to_f64(contract.step_volume.max(1)) / 100.0,
        },
        SizeMode::RiskBalance | SizeMode::RiskEquity | SizeMode::FreeMargin => {
            number::Kind::Percent
        }
        SizeMode::RiskMoney => number::Kind::Money,
    }
}

/// What a protection field holds in its unit.
pub(super) fn offset_kind(unit: Offset, contract: &Contract) -> number::Kind {
    match unit {
        Offset::Price => number::Kind::Price {
            step: contract.pip(),
        },
        Offset::Pips => number::Kind::Pips,
        Offset::Money => number::Kind::Money,
        Offset::Percent => number::Kind::Percent,
        Offset::Ratio => number::Kind::Ratio,
    }
}

/// A value of a protection written for its unit.
pub(super) fn format_offset(unit: Offset, value: f64) -> String {
    let decimals = match unit {
        Offset::Pips => 1,
        _ => 2,
    };
    number::format(value, decimals)
}

/// An amount rounded to two significant digits, for a preset: 97.3 is 97, 1 234 is 1 200.
pub(super) fn nice(amount: f64) -> f64 {
    if !amount.is_finite() || amount <= 0.0 {
        return 10.0;
    }
    let exponent = i32::try_from(f64_to_i64(amount.log10().floor())).unwrap_or(0);
    let magnitude = 10f64.powi(exponent - 1);
    ((amount / magnitude).round() * magnitude).max(1.0)
}

#[cfg(test)]
mod tests {

    use super::super::*;
    use super::*;

    #[test]
    fn presets_are_rounded_to_two_digits() {
        assert_eq!(nice(97.3), 97.0);
        assert_eq!(nice(1_234.0), 1_200.0);
        assert_eq!(nice(25.0), 25.0);
        assert_eq!(nice(0.0), 10.0);
        assert_eq!(format_offset(Offset::Pips, 12.345), "12.3");
        assert_eq!(format_offset(Offset::Money, 100.0), "100");
    }

    #[test]
    fn a_slippage_in_pips_is_written_in_points() {
        // A five digit pair: a pip is ten points.
        let pair = Contract::default();
        assert_eq!(slippage_points(2.0, &pair), 20);
        assert_eq!(slippage_points(0.5, &pair), 5);
        // A three digit yen pair, the pip at the second decimal.
        let yen = Contract {
            digits: 3,
            pip_position: 2,
            ..Contract::default()
        };
        assert_eq!(slippage_points(3.0, &yen), 30);
        assert_eq!(slippage_points(-1.0, &yen), 0);
    }

    fn plan(atr: Option<AtrStop>, rr: Option<f64>, stop: f64, target: f64) -> PositionPlan {
        PositionPlan {
            drawing: 7,
            buy: true,
            entry: 100.0,
            stop_loss: stop,
            take_profit: target,
            atr,
            rr,
        }
    }

    #[test]
    fn a_drawing_asks_for_a_change_only_when_its_own_values_move() {
        let fixed = plan(None, None, 98.0, 104.0);
        assert!(same_shape(&fixed, &fixed.clone()));
        assert!(
            !same_shape(&fixed, &plan(None, None, 97.0, 104.0)),
            "a dragged stop"
        );
        assert!(
            !same_shape(&fixed, &plan(None, None, 98.0, 105.0)),
            "a dragged target"
        );
        let mut flipped = fixed.clone();
        flipped.buy = false;
        assert!(!same_shape(&fixed, &flipped));
        let mut moved = fixed.clone();
        moved.entry = 101.0;
        assert!(!same_shape(&fixed, &moved));
    }

    #[test]
    fn the_levels_the_ticket_works_out_are_not_read_as_moves() {
        // The stop of an ATR stop changes with every bar, the target of a multiple of the
        // risk with the stop: the ticket works both out, so neither is a change to write.
        let atr = Some(AtrStop::default());
        let before = plan(atr.clone(), Some(2.0), 98.0, 104.0);
        let after = plan(atr.clone(), Some(2.0), 97.5, 105.0);
        assert!(same_shape(&before, &after));
        // The settings of the ATR and the multiple are.
        let wider = AtrStop {
            multiplier: 3.0,
            ..AtrStop::default()
        };
        assert!(!same_shape(
            &before,
            &plan(Some(wider), Some(2.0), 98.0, 104.0)
        ));
        assert!(!same_shape(
            &before,
            &plan(atr.clone(), Some(3.0), 98.0, 104.0)
        ));
        // An ATR stop with a fixed target: the target still counts.
        let half = plan(atr.clone(), None, 98.0, 104.0);
        assert!(same_shape(&half, &plan(atr.clone(), None, 97.0, 104.0)));
        assert!(!same_shape(&half, &plan(atr, None, 98.0, 106.0)));
    }

    #[test]
    fn the_limit_of_a_stop_limit_is_past_the_stop() {
        assert!((stop_limit_price(true, 1.1000, 0.0003) - 1.1003).abs() < 1e-12);
        assert!((stop_limit_price(false, 1.1000, 0.0003) - 1.0997).abs() < 1e-12);
    }
}
