use super::{ReverseOrder, ReverseTracker};
use crate::domain::trading::ExecutionType;

fn order() -> ReverseOrder {
    ReverseOrder {
        symbol: 7,
        buy: false,
        volume: 1_000,
    }
}

#[test]
fn reverse_waits_for_both_close_acceptance_and_full_closure() {
    let mut tracker = ReverseTracker::default();
    assert!(tracker.start(11, order()));
    assert!(!tracker.start(11, order()));
    assert_eq!(
        tracker.acknowledged(11, Some(ExecutionType::OrderAccepted), Some(101)),
        None
    );
    assert_eq!(tracker.closed(11), Some(order()));
    assert_eq!(tracker.closed(11), None);

    assert!(tracker.start(12, order()));
    assert_eq!(tracker.closed(12), None);
    assert_eq!(
        tracker.acknowledged(12, Some(ExecutionType::OrderPartialFill), Some(102)),
        Some(order())
    );
}

#[test]
fn rejected_or_uncertain_close_never_opens_the_reverse_order() {
    for kind in [Some(ExecutionType::OrderRejected), None] {
        let mut tracker = ReverseTracker::default();
        assert!(tracker.start(11, order()));
        assert_eq!(tracker.closed(11), None);
        assert_eq!(tracker.acknowledged(11, kind, None), None);
        assert_eq!(tracker.closed(11), None);
    }
}

#[test]
fn a_later_close_error_cancels_only_its_reverse() {
    let mut tracker = ReverseTracker::default();
    assert!(tracker.start(11, order()));
    assert!(tracker.start(12, order()));
    assert_eq!(
        tracker.acknowledged(11, Some(ExecutionType::OrderAccepted), Some(101)),
        None
    );
    assert_eq!(
        tracker.acknowledged(12, Some(ExecutionType::OrderAccepted), Some(102)),
        None
    );
    tracker.cancel_related(None, Some(101));
    assert_eq!(tracker.closed(11), None);
    assert_eq!(tracker.closed(12), Some(order()));
}
