//! What a replay asks the server for in ticks, and how it finds out whether the broker has them.
//!
//! Ticks are held in a window around the cursor, not for the whole replay: a symbol keeps its
//! last few hours, and a minute of them is all a candle of any timeframe needs. Where the broker
//! has no ticks (it keeps them for a limited time), the symbol falls back to minute bars.

use std::collections::HashMap;
use std::time::Duration;

use crate::system::runtime;
use gpui::Context;
use wyck_market::Period;

use super::plan::{self, Job, Need};
use super::ticks::{TickStore, TickSupport, Verdict, probe_verdict};
use super::{DemandKind, MAX_IN_FLIGHT, Phase, RETRY_MS, Replay, ReplayEvent, TickDone, TickMode};
use crate::broker::Error as ApiError;
use crate::broker::Result as ApiResult;
use crate::lost::lost;
use crate::market_data::load;
use crate::market_data::now_ms;
use crate::market_data::replay_ticks::{tick_chunk_before, tick_chunk_forward};

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
/// Ticks kept before the cursor for the candle in progress of a bar chart.
const BARS_BACK_MS: i64 = 3 * MINUTE_MS;
/// Ticks kept before the cursor for a tick chart.
const TICKS_BACK_MS: i64 = 30 * MINUTE_MS;
/// The most a seconds chart keeps before the cursor.
const SECONDS_BACK_MS: i64 = 6 * HOUR_MS;
/// How far ahead of the cursor ticks are loaded, at the least and at the most.
const MIN_AHEAD_MS: i64 = 30 * MINUTE_MS;
const MAX_AHEAD_MS: i64 = 4 * HOUR_MS;

impl Replay {
    /// The symbols that need ticks, and how far back each needs them. Nothing when the user keeps
    /// the replay on minute bars.
    pub(super) fn wanted_ticks(&self) -> HashMap<i64, i64> {
        let mut wanted: HashMap<i64, i64> = HashMap::new();
        if self.tick_mode == TickMode::Minutes {
            return wanted;
        }
        for demand in &self.demand {
            let back = match demand.kind {
                DemandKind::Bars(_) => BARS_BACK_MS,
                DemandKind::Ticks => TICKS_BACK_MS,
                DemandKind::Seconds(seconds) => i64::try_from(demand.back_bars)
                    .unwrap_or(i64::MAX)
                    .saturating_mul(i64::from(seconds) * 1_000)
                    .clamp(BARS_BACK_MS, SECONDS_BACK_MS),
            };
            let slot = wanted.entry(demand.symbol_id).or_insert(0);
            *slot = (*slot).max(back);
        }
        wanted
    }

    /// How far ahead of the cursor ticks are loaded.
    fn ticks_ahead_ms(&self) -> i64 {
        let eaten = wyck_core::math::f64_to_i64(
            wyck_core::math::i64_to_f64(self.step_ms()) * self.rate_hz.max(0.0) * 60.0,
        );
        eaten.clamp(MIN_AHEAD_MS, MAX_AHEAD_MS)
    }

    /// Asks for the ticks the symbols still need, a chunk at a time. A symbol whose ticks are not
    /// known to exist is first asked for a chunk at the cursor: what comes back says whether the
    /// broker has ticks there.
    pub(super) fn pump_ticks(&mut self, cursor: i64, now: i64, cx: &mut Context<Self>) {
        let ahead = self.ticks_ahead_ms();
        let mut gave_up = Vec::new();
        for (symbol, back_ms) in self.wanted_ticks() {
            if self.tick_support.get(&symbol) == Some(&TickSupport::Unavailable) {
                continue;
            }
            let need = Need {
                cursor_ms: cursor,
                back_ms,
                ahead_ms: ahead,
                now_ms: now,
            };
            let store = self
                .tick_stores
                .entry(symbol)
                .or_insert_with(|| TickStore::new(cursor));
            store.trim(cursor - back_ms);
            let known = self.tick_support.get(&symbol) == Some(&TickSupport::Available);
            let coverage = store.coverage();
            let span_ms = store.span_ms();
            let limit_ms = plan::live_edge_ms(now) - 1;
            let job = if known {
                plan::next_job(&coverage, &need)
            } else if coverage.to_ms + 1 > limit_ms {
                // The probe reached the present with no tick: there is nothing to look further into.
                gave_up.push(symbol);
                None
            } else {
                Some(Job::Forward {
                    from_ms: coverage.to_ms + 1,
                    limit_ms,
                })
            };
            let blocked = self
                .tick_blocked
                .get(&symbol)
                .is_some_and(|until| *until > now);
            if let Some(job) = job
                && !blocked
                && !self.tick_inflight.contains(&symbol)
                && self.inflight.len() + self.tick_inflight.len() < MAX_IN_FLIGHT
            {
                self.run_ticks(symbol, job, span_ms, cx);
            }
        }
        for symbol in gave_up {
            self.give_up_ticks(symbol);
        }
    }

    fn run_ticks(&mut self, symbol: i64, job: Job, span_ms: i64, cx: &mut Context<Self>) {
        self.tick_inflight.insert(symbol);
        let (session, epoch) = (self.session.clone(), self.epoch);
        cx.spawn(async move |this, cx| {
            let done = runtime::spawn(async move {
                let market = load::market(&session)?;
                match job {
                    Job::Before { before_ms } => {
                        tick_chunk_before(&market, symbol, before_ms, span_ms)
                            .await
                            .map(TickDone::Before)
                    }
                    Job::Forward { from_ms, limit_ms } => {
                        tick_chunk_forward(&market, symbol, from_ms, limit_ms, span_ms)
                            .await
                            .map(TickDone::Forward)
                    }
                }
            })
            .await;
            lost(
                "replay: load ticks",
                this.update(cx, |this, cx| {
                    this.ticks_loaded(symbol, epoch, done.unwrap_or(Err(ApiError::Closed)), cx);
                }),
            );
        })
        .detach();
    }

    fn ticks_loaded(
        &mut self,
        symbol: i64,
        epoch: u64,
        done: ApiResult<TickDone>,
        cx: &mut Context<Self>,
    ) {
        if epoch != self.epoch {
            return;
        }
        self.tick_inflight.remove(&symbol);
        match done {
            Ok(done) => self.apply_ticks(symbol, done),
            Err(error) => {
                tracing::warn!(%error, "could not load ticks for the replay");
                if self.tick_support.get(&symbol) == Some(&TickSupport::Available) {
                    self.tick_blocked.insert(symbol, now_ms() + RETRY_MS);
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(RETRY_MS.unsigned_abs()))
                            .await;
                        lost("replay: retry ticks", this.update(cx, Self::pump));
                    })
                    .detach();
                } else {
                    // Not known to exist: the candle is made of minute bars, as if the broker had
                    // none, rather than the replay failing for ticks it may not need.
                    self.give_up_ticks(symbol);
                }
            }
        }
        self.refresh_step();
        cx.emit(ReplayEvent::DataChanged);
        self.pump(cx);
        cx.notify();
    }

    fn apply_ticks(&mut self, symbol: i64, done: TickDone) {
        let known = self.tick_support.get(&symbol) == Some(&TickSupport::Available);
        let Some(store) = self.tick_stores.get_mut(&symbol) else {
            return;
        };
        let (from_ms, to_ms, has_ticks) = match &done {
            TickDone::Before(chunk) | TickDone::Forward(chunk) => {
                (chunk.from_ms, chunk.to_ms, !chunk.ticks.is_empty())
            }
        };
        match done {
            TickDone::Before(chunk) => store.apply_before(chunk),
            TickDone::Forward(chunk) => store.apply_forward(chunk),
        }
        if known {
            return;
        }
        let searched = store.searched_ms();
        let minutes = self.stores.get(&(symbol, Period::M1)).is_some_and(|m1| {
            m1.bars()
                .iter()
                .any(|b| b.time_ms >= from_ms && b.time_ms <= to_ms)
        });
        match probe_verdict(has_ticks, minutes, searched) {
            Verdict::Available => {
                self.tick_support.insert(symbol, TickSupport::Available);
            }
            Verdict::Unavailable => self.give_up_ticks(symbol),
            Verdict::KeepLooking => {}
        }
    }

    /// The broker has no ticks here: the symbol is made of minute bars.
    fn give_up_ticks(&mut self, symbol: i64) {
        self.tick_support.insert(symbol, TickSupport::Unavailable);
        self.tick_stores.remove(&symbol);
    }

    /// Whether the ticks every chart needs are held, or known not to exist, so the replay may
    /// start. A symbol whose ticks are still being looked for waits.
    pub(super) fn ticks_ready(&self, cursor: i64, now: i64) -> bool {
        let ahead = self.ticks_ahead_ms();
        self.wanted_ticks().into_iter().all(|(symbol, back_ms)| {
            match self.tick_support.get(&symbol).copied().unwrap_or_default() {
                TickSupport::Unavailable => true,
                TickSupport::Unknown => false,
                TickSupport::Available => {
                    let need = Need {
                        cursor_ms: cursor,
                        back_ms,
                        ahead_ms: ahead,
                        now_ms: now,
                    };
                    self.tick_stores
                        .get(&symbol)
                        .is_some_and(|store| plan::ready_to_play(&store.coverage(), &need))
                }
            }
        })
    }

    /// Whether the replay is still preparing: used to show a spinner rather than play.
    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.phase == Phase::Preparing
    }
}
