//! What a replay asks the server for, and what it does with the answers.

use std::collections::HashMap;
use std::time::Duration;

use crate::system::runtime;
use gpui::Context;
use wyck_market::Period;

use super::plan::{self, Job, Need};
use super::store::Store;
use super::{
    DAY_MS, Done, Key, MAX_BACK_MS, MAX_IN_FLIGHT, MAX_LEAD_MS, Phase, RETRY_MS, Replay,
    ReplayEvent,
};
use crate::broker::Error as ApiError;
use crate::broker::Result as ApiResult;
use crate::lost::lost;
use crate::market_data::load;
use crate::market_data::now_ms;
use crate::market_data::replay_load;

impl Replay {
    /// The stores to hold, and how far back each one goes: the bars of every chart, and the
    /// minute bars of its symbol (as far back as its biggest period needs).
    pub(super) fn wanted(&self) -> HashMap<Key, i64> {
        let mut wanted: HashMap<Key, i64> = HashMap::new();
        let mut raise = |key: Key, back_ms: i64| {
            let slot = wanted.entry(key).or_insert(0);
            *slot = (*slot).max(back_ms);
        };
        for demand in &self.demand {
            let mut lead = DAY_MS;
            if let Some(period) = demand.period() {
                let back = i64::try_from(demand.back_bars)
                    .unwrap_or(i64::MAX)
                    .saturating_mul(period.millis())
                    .min(MAX_BACK_MS);
                raise((demand.symbol_id, period), back);
                lead = period.millis().clamp(DAY_MS, MAX_LEAD_MS);
            }
            raise((demand.symbol_id, Period::M1), lead);
        }
        wanted
    }

    /// Asks for whatever the stores still need, a little at a time.
    pub(super) fn pump(&mut self, cx: &mut Context<Self>) {
        if !self.is_on() {
            return;
        }
        let Some(cursor) = self.cursor_ms() else {
            return;
        };
        let now = now_ms();
        let ahead = plan::prefetch_ahead_ms(self.rate_hz, self.step_ms());
        for (key, back_ms) in self.wanted() {
            let need = Need {
                cursor_ms: cursor,
                back_ms,
                ahead_ms: ahead,
                now_ms: now,
            };
            let store = self.stores.entry(key).or_insert_with(|| Store::new(cursor));
            store.trim(cursor - back_ms);
            let job = plan::next_job(&store.coverage(), &need);
            let blocked = self.blocked.get(&key).is_some_and(|until| *until > now);
            if let Some(job) = job
                && !blocked
                && !self.inflight.contains(&key)
                && self.inflight.len() + self.tick_inflight.len() < MAX_IN_FLIGHT
            {
                self.run(key, job, cx);
            }
        }
        self.pump_ticks(cursor, now, cx);
        self.refresh_phase(cursor, now, cx);
    }

    fn run(&mut self, key: Key, job: Job, cx: &mut Context<Self>) {
        self.inflight.insert(key);
        let (session, epoch) = (self.session.clone(), self.epoch);
        let (symbol, period) = key;
        cx.spawn(async move |this, cx| {
            let done = runtime::spawn(async move {
                let market = load::market(&session)?;
                match job {
                    Job::Before { before_ms } => {
                        replay_load::chunk_before(&market, symbol, period, before_ms)
                            .await
                            .map(Done::Before)
                    }
                    Job::Forward { from_ms, limit_ms } => {
                        replay_load::chunk_forward(&market, symbol, period, from_ms, limit_ms)
                            .await
                            .map(Done::Forward)
                    }
                }
            })
            .await;
            lost(
                "replay: load history",
                this.update(cx, |this, cx| {
                    this.loaded(key, epoch, done.unwrap_or(Err(ApiError::Closed)), cx);
                }),
            );
        })
        .detach();
    }

    fn loaded(&mut self, key: Key, epoch: u64, done: ApiResult<Done>, cx: &mut Context<Self>) {
        if epoch != self.epoch {
            return;
        }
        self.inflight.remove(&key);
        match done {
            Ok(done) => {
                if let Some(store) = self.stores.get_mut(&key) {
                    match done {
                        Done::Before(chunk) => store.apply_before(chunk),
                        Done::Forward(chunk) => store.apply_forward(&chunk),
                    }
                }
                cx.emit(ReplayEvent::DataChanged);
            }
            Err(error) => {
                tracing::warn!(%error, "could not load history for the replay");
                if self.phase == Phase::Preparing {
                    return self.fail("The history could not be loaded for the replay.", cx);
                }
                self.blocked.insert(key, now_ms() + RETRY_MS);
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(RETRY_MS.unsigned_abs()))
                        .await;
                    lost("replay: retry", this.update(cx, Self::pump));
                })
                .detach();
            }
        }
        self.pump(cx);
        cx.notify();
    }

    /// Preparing ends when every store holds what the first steps need.
    fn refresh_phase(&mut self, cursor: i64, now: i64, cx: &mut Context<Self>) {
        if self.phase != Phase::Preparing {
            return;
        }
        let ahead = plan::prefetch_ahead_ms(self.rate_hz, self.step_ms());
        let ready = self.wanted().into_iter().all(|(key, back_ms)| {
            let need = Need {
                cursor_ms: cursor,
                back_ms,
                ahead_ms: ahead,
                now_ms: now,
            };
            self.stores
                .get(&key)
                .is_some_and(|store| plan::ready_to_play(&store.coverage(), &need))
        }) && self.ticks_ready(cursor, now);
        if ready {
            self.phase = Phase::Active;
            cx.emit(ReplayEvent::Ready);
        }
    }
}
