//! The transport of a replay: play, pause, steps, jumps, speed, and what it does on every frame.

use gpui::Context;
use wyck_domain::chart::{
    ClockStep, ReplayBounds, ReplayClock, ReplayNoGaps, ReplayStep, ReplayTimeline,
};

use super::plan;
use super::store::{DataTimes, Store};
use super::ticks::TickTimes;
use super::{Demand, FRAME, Phase, Replay, ReplayEvent, TickMode};
use crate::market_data::now_ms;

impl Replay {
    pub fn play(&mut self, cx: &mut Context<Self>) {
        if self.phase != Phase::Active {
            return;
        }
        if let Some(clock) = &mut self.clock {
            clock.play(now_ms());
        }
        self.buffering = false;
        self.ticker += 1;
        let id = self.ticker;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                match this.update(cx, |this, cx| this.tick(id, cx)) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = &mut self.clock {
            clock.pause();
        }
        self.ticker += 1;
        self.buffering = false;
        // A chart that skipped frames while it played catches up with where playing stopped.
        cx.emit(ReplayEvent::Cursor);
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.is_playing() {
            self.pause(cx);
        } else {
            self.play(cx);
        }
    }

    pub fn step_forward(&mut self, cx: &mut Context<Self>) {
        self.step_by_hand(true, cx);
    }

    pub fn step_back(&mut self, cx: &mut Context<Self>) {
        self.step_by_hand(false, cx);
    }

    fn step_by_hand(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.phase != Phase::Active {
            return;
        }
        let Some(mut clock) = self.clock.take() else {
            return;
        };
        let bounds = self.bounds(&clock, now_ms());
        let timeline = self.timeline(clock.step());
        let moved = if forward {
            clock.step_forward(bounds, &*timeline)
        } else {
            clock.step_back(bounds, &*timeline)
        };
        drop(timeline);
        self.clock = Some(clock);
        self.ticker += 1;
        self.buffering = false;
        if moved {
            cx.emit(ReplayEvent::Cursor);
        }
        self.pump(cx);
        cx.notify();
    }

    /// Goes to `ms`: a jump inside the data held, or a new start when it is outside.
    pub fn go_to(&mut self, ms: i64, cx: &mut Context<Self>) {
        if self.phase == Phase::Active
            && let Some(mut clock) = self.clock.take()
        {
            let bounds = self.bounds(&clock, now_ms());
            if (bounds.floor_ms..=bounds.cap_ms).contains(&ms) {
                clock.seek(ms, bounds);
                self.clock = Some(clock);
                self.ticker += 1;
                self.buffering = false;
                cx.emit(ReplayEvent::Cursor);
                self.pump(cx);
                cx.notify();
                return;
            }
            self.clock = Some(clock);
        }
        self.start_at(ms, cx);
    }

    /// Back to the bar the replay started from.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        self.go_to(self.start_ms, cx);
    }

    pub fn set_rate(&mut self, rate_hz: f64, cx: &mut Context<Self>) {
        if let Some(clock) = &mut self.clock {
            clock.set_rate(rate_hz, now_ms());
            self.rate_hz = clock.rate_hz();
        } else {
            self.rate_hz = rate_hz.clamp(0.05, 60.0);
        }
        cx.notify();
    }

    pub fn set_step(&mut self, step: ReplayStep, cx: &mut Context<Self>) {
        self.step = step;
        self.refresh_step();
        self.pump(cx);
        cx.notify();
    }

    /// Lets the replay build the candle in progress from real ticks (`Auto`, where the broker has
    /// them) or from minute bars only.
    pub fn set_tick_mode(&mut self, mode: TickMode, cx: &mut Context<Self>) {
        if self.tick_mode == mode {
            return;
        }
        self.tick_mode = mode;
        self.tick_stores.clear();
        self.tick_support.clear();
        self.tick_inflight.clear();
        self.refresh_step();
        self.pump(cx);
        cx.emit(ReplayEvent::DataChanged);
        cx.notify();
    }

    pub fn set_skip_gaps(&mut self, skip: bool, cx: &mut Context<Self>) {
        self.skip_gaps = skip;
        cx.notify();
    }

    pub fn set_future_drawings(&mut self, show: bool, cx: &mut Context<Self>) {
        self.future_drawings = show;
        cx.emit(ReplayEvent::DataChanged);
        cx.notify();
    }

    /// What the charts need held. Charts call it when they enter the replay and whenever their
    /// symbol or timeframe changes; the same demand twice is nothing.
    pub fn set_demand(&mut self, demand: Vec<Demand>, cx: &mut Context<Self>) {
        if demand == self.demand {
            return;
        }
        self.demand = demand;
        let wanted = self.wanted();
        self.stores.retain(|key, _| wanted.contains_key(key));
        self.refresh_step();
        self.pump(cx);
        cx.emit(ReplayEvent::DataChanged);
        cx.notify();
    }

    /// The connection is back: try again what failed.
    pub fn on_ready(&mut self, cx: &mut Context<Self>) {
        self.blocked.clear();
        self.pump(cx);
    }

    // ---- Playing.

    fn tick(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        if id != self.ticker {
            return false;
        }
        let Some(mut clock) = self.clock.take() else {
            return false;
        };
        let now = now_ms();
        let bounds = self.bounds(&clock, now);
        let timeline = self.timeline(clock.step());
        let advance = clock.advance(now, bounds, &*timeline);
        drop(timeline);
        let caught_up = advance.hit_cap && bounds.cap_ms >= plan::live_edge_ms(now);
        if caught_up {
            clock.pause();
        }
        let playing = clock.is_playing();
        self.clock = Some(clock);
        self.buffering = advance.hit_cap && !caught_up;
        if advance.steps > 0 {
            cx.emit(ReplayEvent::Cursor);
        }
        if caught_up {
            cx.emit(ReplayEvent::ReachedNow);
        }
        self.pump(cx);
        cx.notify();
        playing
    }

    /// The range the cursor may take: from where every store has data to where all of them reach.
    fn bounds(&self, clock: &ReplayClock, now_ms: i64) -> ReplayBounds {
        let cursor = clock.cursor_ms();
        let floor = self
            .stores
            .values()
            .map(Store::oldest_ms)
            .max()
            .unwrap_or(cursor)
            .min(cursor);
        let tick_cap = self
            .tick_stores
            .iter()
            .filter(|(symbol, _)| self.symbol_uses_ticks(**symbol))
            .map(|(_, store)| store.coverage().to_ms + 1);
        let cap = self
            .stores
            .values()
            .map(|store| store.covered_to_ms() + 1)
            .chain(tick_cap)
            .min()
            .unwrap_or(cursor)
            .min(plan::live_edge_ms(now_ms));
        ReplayBounds {
            floor_ms: floor,
            cap_ms: cap,
        }
    }

    /// When there is data, for the clock to jump over the hours with none (when asked to) and for a
    /// step in ticks, which always needs the times of the ticks.
    fn timeline(&self, step: ClockStep) -> Box<dyn ReplayTimeline + '_> {
        if matches!(step, ClockStep::Ticks(_)) {
            Box::new(TickTimes(
                self.tick_stores
                    .iter()
                    .filter(|(symbol, _)| self.symbol_uses_ticks(**symbol))
                    .map(|(_, store)| store.ticks())
                    .collect(),
            ))
        } else if self.skip_gaps {
            Box::new(DataTimes(self.stores.values().map(Store::bars).collect()))
        } else {
            Box::new(ReplayNoGaps)
        }
    }

    pub(super) fn refresh_step(&mut self) {
        let step = self.clock_step();
        if let Some(clock) = &mut self.clock {
            clock.set_step(step);
        }
    }
}
