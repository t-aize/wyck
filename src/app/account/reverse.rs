//! Reversing a position: the order that opens the other way waits for the close to be final.

use super::{Account, Busy, Notice, REVERSE_FINAL_WAIT, REVERSE_RECHECK, Tone};

use crate::app::broker::trading::NewOrderReq;
use crate::domain::trading::ExecutionType;
use crate::domain::trading::TradeSide;
use crate::domain::trading::guard::Verdict;
use gpui::Context;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReverseOrder {
    pub(super) symbol: i64,
    pub(super) buy: bool,
    pub(super) volume: i64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingReverse {
    pub(super) order: ReverseOrder,
    pub(super) started: Instant,
    pub(super) close_order_id: Option<i64>,
    pub(super) acknowledged: bool,
    pub(super) closed: bool,
}

#[derive(Default)]
pub(super) struct ReverseTracker(pub(super) HashMap<i64, PendingReverse>);

impl ReverseTracker {
    pub(super) fn start(&mut self, position_id: i64, order: ReverseOrder) -> bool {
        if self.0.contains_key(&position_id) {
            return false;
        }
        self.0.insert(
            position_id,
            PendingReverse {
                order,
                started: Instant::now(),
                close_order_id: None,
                acknowledged: false,
                closed: false,
            },
        );
        true
    }

    pub(super) fn acknowledged(
        &mut self,
        position_id: i64,
        kind: Option<ExecutionType>,
        close_order_id: Option<i64>,
    ) -> Option<ReverseOrder> {
        if !matches!(
            kind,
            Some(
                ExecutionType::OrderAccepted
                    | ExecutionType::OrderPartialFill
                    | ExecutionType::OrderFilled
            )
        ) {
            self.0.remove(&position_id);
            return None;
        }
        if let Some(pending) = self.0.get_mut(&position_id) {
            pending.acknowledged = true;
            pending.close_order_id = close_order_id;
        }
        self.take_ready(position_id)
    }

    pub(super) fn cancel_related(&mut self, position_id: Option<i64>, order_id: Option<i64>) {
        if position_id.is_none() && order_id.is_none() {
            self.0.clear();
            return;
        }
        self.0.retain(|id, pending| {
            Some(*id) != position_id && !(order_id.is_some() && pending.close_order_id == order_id)
        });
    }

    pub(super) fn closed(&mut self, position_id: i64) -> Option<ReverseOrder> {
        if let Some(pending) = self.0.get_mut(&position_id) {
            pending.closed = true;
        }
        self.take_ready(position_id)
    }

    pub(super) fn take_ready(&mut self, position_id: i64) -> Option<ReverseOrder> {
        self.0
            .get(&position_id)
            .is_some_and(|p| p.acknowledged && p.closed)
            .then(|| self.0.remove(&position_id))
            .flatten()
            .map(|p| p.order)
    }
}

impl Account {
    pub(super) fn watch_reverse(
        &mut self,
        position_id: i64,
        started: Instant,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REVERSE_RECHECK).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .reversals
                    .0
                    .get(&position_id)
                    .is_some_and(|pending| pending.started == started)
                {
                    this.on_ready(cx);
                }
            });
            cx.background_executor().timer(REVERSE_FINAL_WAIT).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .reversals
                    .0
                    .get(&position_id)
                    .is_some_and(|pending| pending.started == started)
                {
                    this.reversals.0.remove(&position_id);
                    this.tell(
                        Notice::new(
                            Tone::Warning,
                            "Reverse stopped",
                            "The position's closure was not confirmed.",
                        )
                        .hint(Some("Check the account before trying again.".to_owned())),
                        cx,
                    );
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Reverses a position: closes it and opens the same volume the other way.
    pub fn reverse_position(&mut self, position_id: i64, cx: &mut Context<Self>) {
        if self.busy.contains(&Busy::Closing(position_id)) {
            return;
        }
        let Some(position) = self.book.positions.get(&position_id).cloned() else {
            return;
        };
        let buy = !position.trade_data.is_buy();
        let (symbol, volume) = (position.trade_data.symbol_id, position.trade_data.volume);
        // The position closes first: if the order that opens the other way would be refused, it is
        // better to know before anything is closed.
        let side = if buy { TradeSide::Buy } else { TradeSide::Sell };
        if let Verdict::Block(reason) =
            self.assess(&NewOrderReq::market(symbol, side, volume), false)
        {
            self.tell(
                Notice::new(
                    Tone::Warning,
                    "Reverse blocked by your safety settings",
                    reason,
                )
                .hint(Some("Nothing was closed.".to_owned())),
                cx,
            );
            return;
        }
        if !self.reversals.start(
            position_id,
            ReverseOrder {
                symbol,
                buy,
                volume,
            },
        ) {
            return;
        }
        self.close_position(position_id, None, cx);
    }
}

#[cfg(test)]
mod tests {

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
}
