//! Sending orders: one at a time or in a batch, and the queue of a plan (what the app does for
//! a plan once its orders are out is in [`super::plans`]).

use super::{Account, Busy, CALL_TIMEOUT, Notice, Tone, describe, flatten};
use crate::broker::trading::NewOrderReq;
use crate::broker::{Error as ApiError, Event, Result as ApiResult};
use crate::lost::lost;
use crate::system::runtime;
use gpui::Context;
use wyck_trading::doubt::InDoubt;
use wyck_trading::guard;
use wyck_trading::guard::{Blocked, Cleared, Verdict};

impl Account {
    /// Runs a trading call; on failure says why and reads the account again (the call may have
    /// reached the server even so).
    pub(super) fn trade<F, Fut>(&mut self, busy: Busy, cx: &mut Context<Self>, call: F)
    where
        F: FnOnce(crate::broker::trading::TradingClient) -> Fut + Send + 'static,
        Fut: Future<Output = ApiResult<wyck_trading::ExecutionEvent>> + Send + 'static,
    {
        if !self.busy.insert(busy) {
            return;
        }
        self.call_seq += 1;
        let seq = self.call_seq;
        self.calls.insert(busy, seq);
        cx.notify();
        // No answer in time: the button is free again, but the account cannot tell what became of
        // the call, so it says so and reads the account.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CALL_TIMEOUT).await;
            lost("account/orders: trade", this.update(cx, |this, cx| {
                if this.calls.get(&busy) == Some(&seq) {
                    this.calls.remove(&busy);
                    this.busy.remove(&busy);
                    this.uncertain = true;
                    this.queue.clear();
                    if busy == Busy::Placing {
                        this.doubt_placing();
                    }
                    Self::tell(
                        Notice::new(
                            Tone::Warning,
                            "No answer from the server",
                            "The request may or may not have gone through. Do not send it again yet.",
                        )
                        .hint(Some(
                            "Check the positions and orders of the account, then try again if it is not there."
                                .to_owned(),
                        )),
                        cx,
                    );
                    this.on_ready(cx);
                    cx.notify();
                }
            }));
        })
        .detach();
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                call(client.account(session.account_id()).trading()).await
            })
            .await;
            lost(
                "account/orders: trade",
                this.update(cx, |this, cx| {
                    // A call that was given up on has nothing to release: a newer one may hold it.
                    if this.calls.get(&busy) == Some(&seq) {
                        this.calls.remove(&busy);
                        this.busy.remove(&busy);
                    }
                    match flatten(result) {
                        // The first execution event answers the request and goes only to it; the
                        // ones after it (a fill after the acceptance) come on the event stream.
                        Ok(event) => {
                            if busy == Busy::Placing {
                                this.placing = None;
                            }
                            let reverse_order = match busy {
                                Busy::Closing(id) => this.reversals.acknowledged(
                                    id,
                                    event.kind(),
                                    event.order.as_ref().map(|o| o.order_id),
                                ),
                                _ => None,
                            };
                            this.on_event(&Event::Execution(Box::new(event)), cx);
                            if let Some(order) = reverse_order {
                                this.market(order.symbol, order.buy, order.volume, cx);
                            }
                            if let Busy::Closing(id) = busy
                                && let Some(pending) = this.reversals.0.get(&id)
                            {
                                Self::watch_reverse(id, pending.started, cx);
                            }
                            if busy == Busy::Placing {
                                this.send_next(cx);
                            }
                        }
                        Err(error) => {
                            if let Busy::Closing(id) = busy {
                                this.reversals.0.remove(&id);
                            }
                            if busy == Busy::Placing {
                                this.drop_queue(cx);
                            }
                            // A new order that failed without an answer may have reached the broker:
                            // it is kept in doubt, the account is read, and it is not sent again.
                            let unknown = busy == Busy::Placing && error.outcome_unknown();
                            if busy == Busy::Placing {
                                if unknown {
                                    this.uncertain = true;
                                    this.doubt_placing();
                                } else {
                                    this.placing = None;
                                }
                            }
                            let reason = describe(&error);
                            let title = match busy {
                                Busy::Placing if unknown => "No answer for the order",
                                Busy::Placing => "The order was not sent",
                                Busy::Closing(_) => "The position was not closed",
                                Busy::Cancelling(_) => "The order was not cancelled",
                                Busy::Amending(_) => "The change was not applied",
                            };
                            let hint = if unknown {
                                Some(
                                    "It may have reached the broker. It is not sent again: the account is read and you are told what became of it."
                                        .to_owned(),
                                )
                            } else {
                                reason.hint
                            };
                            Self::tell(
                                Notice::new(Tone::Error, title, reason.message)
                                    .hint(hint)
                                    .details(error.to_string()),
                                cx,
                            );
                            this.on_ready(cx);
                        }
                    }
                    cx.notify();
                }),
            );
        })
        .detach();
    }

    /// Counts one more order sent today, for the daily limit of the safety settings.
    fn count_sent(&mut self, now: i64) {
        let today = self.zone.day(now);
        if self.sent_today.0 != today {
            self.sent_today = (today, 0);
        }
        self.sent_today.1 += 1;
    }

    /// Sends a new order.
    pub(crate) fn place(&mut self, order: NewOrderReq, cx: &mut Context<Self>) {
        self.place_with(order, false, cx);
    }

    /// Sends a new order. `one_click` says it was sent without a confirmation, which the safety
    /// limits treat more strictly. The order is refused when a hard limit says so, and dropped
    /// when it repeats the one just sent.
    pub(crate) fn place_with(
        &mut self,
        mut order: NewOrderReq,
        one_click: bool,
        cx: &mut Context<Self>,
    ) {
        order.label.get_or_insert_with(|| "wyck".into());
        if let Err(error) = order.validate() {
            Self::tell(Notice::incomplete(&error), cx);
            return;
        }
        // The proof the connection asks for comes only from the checks: an order they refuse
        // cannot be sent.
        let cleared = match self.clear(&order, one_click) {
            Ok(cleared) => cleared,
            Err(Blocked(refusal)) => {
                Self::tell(Notice::blocked(self.refusal_text(&refusal)), cx);
                return;
            }
        };
        let now = crate::market_data::now_ms();
        let fingerprint = order.fingerprint();
        if self.busy.contains(&Busy::Placing) || self.duplicates.is_repeat(fingerprint, now) {
            Self::tell(Notice::repeated(), cx);
            return;
        }
        self.count_sent(now);
        self.identify(&mut order, now);
        self.send(order, cleared, now, cx);
    }

    /// Gives the order an id of its own (unless it came with one), so that it can be recognized on
    /// the account whatever happens to the answer.
    fn identify(&mut self, order: &mut NewOrderReq, now: i64) {
        if order.client_order_id.is_none() {
            self.orders_sent += 1;
            order.client_order_id = Some(guard::client_order_id(
                now,
                std::process::id(),
                self.orders_sent,
            ));
        }
    }

    /// Sends an order that was cleared, keeping what is needed to tell what became of it should its
    /// answer never come.
    fn send(&mut self, order: NewOrderReq, cleared: Cleared, now: i64, cx: &mut Context<Self>) {
        self.placing = order
            .client_order_id
            .clone()
            .map(|client_order_id| InDoubt {
                client_order_id,
                symbol: order.symbol_id,
                buy: order.fingerprint().buy,
                volume: order.volume,
                sent_at: now,
            });
        self.trade(Busy::Placing, cx, move |trading| async move {
            trading.new_order(order, &cleared).await
        });
    }

    /// The account was read again: tells the user what became of each new order whose answer never
    /// came. Nothing is sent from here; whether to send an order again is the user's to decide.
    pub(super) fn settle_in_doubt(&mut self, cx: &mut Context<Self>) {
        use wyck_trading::doubt::Found;
        for doubt in std::mem::take(&mut self.in_doubt) {
            let found = doubt.find(
                self.book.orders.values(),
                self.book.positions.values(),
                &self.book.deals,
            );
            let name = self.book.name(doubt.symbol);
            let lots = self
                .book
                .contract(doubt.symbol)
                .lots_of_volume(doubt.volume);
            let what = format!(
                "your {} order of {} lots on {name}",
                if doubt.buy { "buy" } else { "sell" },
                wyck_trading::contract::format_lots(lots)
            );
            let notice = match found {
                Found::Working { .. } => Notice::new(
                    Tone::Info,
                    "The order is on the account",
                    format!("The broker did receive {what}; it is working. It was not sent again."),
                ),
                Found::Executed { .. } => Notice::new(
                    Tone::Warning,
                    "The order went through",
                    format!(
                        "A trade like {what} was made after it was sent. It was not sent again."
                    ),
                )
                .hint(Some(
                    "Check the positions before you send anything else.".to_owned(),
                )),
                Found::Missing => Notice::new(
                    Tone::Warning,
                    "The order is not on the account",
                    format!(
                        "Nothing of {what} was found after its request got no answer. It was not sent again."
                    ),
                )
                .hint(Some(
                    "If you still want it, send it again, and look at the positions first."
                        .to_owned(),
                )),
            };
            Self::tell(notice, cx);
        }
    }

    /// The order in flight got no answer: it is now one whose fate the account will tell once it
    /// has been read again.
    pub(super) fn doubt_placing(&mut self) {
        if let Some(doubt) = self.placing.take() {
            self.in_doubt.push(doubt);
        }
    }

    /// What the safety checks say about the orders of a plan together: the side that sends the
    /// most is checked with its volumes added up (an OCO pair fills on one side only).
    pub fn assess_batch(&self, orders: &[NewOrderReq], one_click: bool) -> Verdict {
        match Self::batch_probe(orders) {
            Some(probe) => self.assess(&probe, one_click),
            None => Verdict::Ok,
        }
    }

    /// The order the checks look at for a plan: the side that sends the most, with its volumes
    /// added up. `None` for an empty plan.
    fn batch_probe(orders: &[NewOrderReq]) -> Option<NewOrderReq> {
        let mut totals: Vec<(i32, i64, &NewOrderReq)> = Vec::new();
        for order in orders {
            match totals
                .iter_mut()
                .find(|(side, _, _)| *side == order.trade_side)
            {
                Some(entry) => entry.1 += order.volume,
                None => totals.push((order.trade_side, order.volume, order)),
            }
        }
        let &(_, volume, first) = totals.iter().max_by_key(|(_, volume, _)| *volume)?;
        let mut probe = first.clone();
        probe.volume = volume;
        Some(probe)
    }

    /// Checks the orders of a plan together and gives the proof for each of them.
    fn clear_batch(
        &self,
        orders: &[NewOrderReq],
        one_click: bool,
    ) -> Result<Vec<Cleared>, Blocked> {
        let Some(probe) = Self::batch_probe(orders) else {
            return Ok(Vec::new());
        };
        let fingerprints: Vec<_> = orders.iter().map(NewOrderReq::fingerprint).collect();
        guard::clear_batch(
            &self.risk,
            &self.standing(),
            &self.facts_of(&probe, one_click),
            &fingerprints,
            self.mode,
        )
    }

    /// Sends the orders of a plan one after the other, each once the one before it was answered.
    /// They are checked and counted as one order. If one fails, the ones after it are not sent.
    pub fn place_batch(
        &mut self,
        orders: Vec<NewOrderReq>,
        one_click: bool,
        cx: &mut Context<Self>,
    ) {
        if orders.len() <= 1 {
            if let Some(order) = orders.into_iter().next() {
                self.place_with(order, one_click, cx);
            }
            return;
        }
        let Some(first) = orders.first() else {
            return;
        };
        for order in &orders {
            if let Err(error) = order.validate() {
                Self::tell(Notice::incomplete(&error), cx);
                return;
            }
        }
        let cleared = match self.clear_batch(&orders, one_click) {
            Ok(cleared) => cleared,
            Err(Blocked(refusal)) => {
                Self::tell(Notice::blocked(self.refusal_text(&refusal)), cx);
                return;
            }
        };
        let now = crate::market_data::now_ms();
        if self.busy.contains(&Busy::Placing)
            || !self.queue.is_empty()
            || self.duplicates.is_repeat(first.fingerprint(), now)
        {
            Self::tell(Notice::repeated(), cx);
            return;
        }
        self.count_sent(now);
        for (mut order, proof) in orders.into_iter().zip(cleared) {
            order.label.get_or_insert_with(|| "wyck".into());
            self.identify(&mut order, now);
            self.queue.push_back((order, proof));
        }
        self.send_next(cx);
    }

    /// Sends the next order of a plan, when none is in flight.
    pub(super) fn send_next(&mut self, cx: &mut Context<Self>) {
        if self.busy.contains(&Busy::Placing) {
            return;
        }
        if let Some((order, cleared)) = self.queue.pop_front() {
            let now = crate::market_data::now_ms();
            self.send(order, cleared, now, cx);
        }
    }

    /// Drops the orders of a plan that were waiting, and says so.
    pub(super) fn drop_queue(&mut self, cx: &mut Context<Self>) {
        let left = self.queue.len();
        self.queue.clear();
        if left > 0 {
            Self::tell(
                Notice::new(
                    Tone::Warning,
                    "The rest of the plan was not sent",
                    format!("{left} order(s) of the plan were left out."),
                )
                .hint(Some(
                    "Check what is open before sending it again.".to_owned(),
                )),
                cx,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck_trading::TradeSide;

    #[test]
    fn a_plan_is_checked_as_its_biggest_side_with_the_volumes_added_up() {
        let orders = [
            NewOrderReq::limit(1, TradeSide::Buy, 100, 1.1),
            NewOrderReq::limit(1, TradeSide::Buy, 200, 1.2),
            NewOrderReq::limit(1, TradeSide::Sell, 250, 1.3),
        ];
        let probe = Account::batch_probe(&orders).unwrap();
        assert_eq!(probe.volume, 300);
        assert_eq!(probe.trade_side, TradeSide::Buy.number());
        assert!(Account::batch_probe(&[]).is_none());
    }
}
