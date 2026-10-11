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
