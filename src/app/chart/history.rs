//! Getting the prices: the first load, older pages as the user scrolls back, the gap after a lost
//! connection, and the live prices.

use std::time::{Duration, Instant};

use gpui::Context;
use wyck::openapi::Result as ApiResult;
use wyck::openapi::market::Tick;

use super::data::{self, MAX_BARS, MAX_TICKS, Series};
use super::live::{LiveUpdate, Wish};
use super::load::{self, Loaded};
use super::view::View;
use super::{Chart, Load, Older, Timeframe, empty_series, flatten, now_ms};
use crate::app::runtime;

impl Chart {
    /// Throws the data away and loads the current symbol and timeframe from scratch.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        let epoch = self.epoch;
        self.series = empty_series(self.timeframe);
        self.display = Default::default();
        self.view = View::new(self.timeframe.default_bar_px());
        self.older = Older::Idle;
        self.hover = None;
        self.drag = None;
        self.bid = None;
        self.ask = None;
        self.last_time_ms = 0;
        self.group_tail.clear();
        let Some(symbol) = &self.symbol else {
            self.load = Load::Idle;
            self.hub.set(self.id, None);
            cx.notify();
            return;
        };
        self.load = Load::Loading;
        self.hub.set(
            self.id,
            Some(Wish::symbol(symbol.id, self.timeframe.period())),
        );
        cx.notify();

        let (session, id, timeframe) = (self.session.clone(), symbol.id, self.timeframe);
        cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(
                    async move { load::initial(&session, id, timeframe, now_ms()).await },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                this.initial_loaded(epoch, flatten(result), cx);
            });
        })
        .detach();
    }

    fn initial_loaded(&mut self, epoch: u64, result: ApiResult<Loaded>, cx: &mut Context<Self>) {
        if epoch != self.epoch {
            return;
        }
        match result {
            Ok(loaded) => {
                match (loaded, &mut self.series) {
                    (Loaded::Bars(bars), Series::Bars(held)) => *held = bars,
                    (Loaded::Grouped { bars, tail }, Series::Bars(held)) => {
                        *held = bars;
                        self.group_tail = tail;
                    }
                    (Loaded::Ticks(ticks), Series::Ticks(held)) => *held = ticks,
                    _ => {}
                }
                self.last_time_ms = self.series.last_time().unwrap_or(0);
                self.load = Load::Ready;
                self.rebuild_display();
                let plot_w = self.geometry().plot_w();
                let len = self.shown().len();
                self.view.clamp(len, plot_w);
                cx.notify();
                // Prices that arrived while the history was on its way are not in it: fetch the
                // few seconds in between. The server's own bars fix themselves on the next tick.
                if self.timeframe.is_tick_built() {
                    self.refill(cx);
                }
                self.load_older_if_needed(cx);
            }
            Err(error) => {
                tracing::warn!(%error, "could not load the chart history");
                self.load = Load::Failed(error.to_string().into());
                cx.notify();
            }
        }
    }

    /// Loads again after a failure (the retry button).
    pub(super) fn retry(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    // ---- the connection ----

    /// The session is connected (again): fetch what was missed, or start over if the first
    /// load never made it.
    pub fn on_ready(&mut self, cx: &mut Context<Self>) {
        match self.load {
            Load::Failed(_) => self.reload(cx),
            Load::Ready => self.refill(cx),
            Load::Idle | Load::Loading => {}
        }
    }

    /// Fetches the prices between the newest point held and now, and joins them in.
    fn refill(&mut self, cx: &mut Context<Self>) {
        let (Some(symbol), Some(from)) = (&self.symbol, self.series.last_time()) else {
            if self.symbol.is_some() && self.series.is_empty() {
                self.reload(cx);
            }
            return;
        };
        let to = now_ms();
        // A very long absence is cheaper to reload than to patch.
        if self.timeframe.is_tick_built() {
            let span = match self.timeframe.bar_ms() {
                Some(bar_ms) => self.timeframe.tick_span_ms().max(bar_ms),
                None => self.timeframe.tick_span_ms(),
            };
            if to - from > 6 * span {
                self.reload(cx);
                return;
            }
        }
        let (session, id, timeframe, epoch) =
            (self.session.clone(), symbol.id, self.timeframe, self.epoch);
        cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(async move { load::gap(&session, id, timeframe, from, to).await })
                    .await;
            let _ = this.update(cx, |this, cx| {
                if epoch != this.epoch {
                    return;
                }
                match flatten(result) {
                    Ok(loaded) => this.gap_loaded(loaded, from, to, cx),
                    Err(error) => tracing::warn!(%error, "could not fill the gap in the chart"),
                }
            });
        })
        .detach();
    }

    fn gap_loaded(&mut self, loaded: Loaded, from: i64, to: i64, cx: &mut Context<Self>) {
        let before = self.series.len();
        match (loaded, &mut self.series) {
            (Loaded::Bars(bars), Series::Bars(held)) => data::merge_bars(held, bars),
            (Loaded::Grouped { bars, tail }, Series::Bars(held)) => {
                data::merge_bars(held, bars);
                if !tail.is_empty() {
                    self.group_tail = tail;
                }
            }
            (Loaded::Ticks(ticks), Series::Ticks(held)) => {
                data::splice_ticks(held, ticks, from, to);
            }
            _ => return,
        }
        let added = self.series.len().saturating_sub(before);
        if !self.display.is_derived() {
            self.view.on_appended(added);
        }
        self.last_time_ms = self.series.last_time().unwrap_or(self.last_time_ms);
        self.data_changed(cx);
    }

    // ---- older history ----

    /// Asks for older history when the view is near the oldest point held.
    pub(super) fn load_older_if_needed(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.load, Load::Ready) {
            return;
        }
        match self.older {
            Older::Idle => {}
            Older::Retry(at) if Instant::now() >= at => {}
            _ => return,
        }
        if self.series.len() >= self.series.capacity_limit() {
            self.older = Older::Exhausted;
            return;
        }
        let plot_w = self.geometry().plot_w();
        let shown = self.shown().len();
        let (first, _) = self.view.visible(shown, plot_w);
        if (first as f64) > self.view.span(plot_w) * 0.5 {
            return;
        }
        let (Some(symbol), Some(oldest)) = (&self.symbol, self.series.first_time()) else {
            return;
        };
        self.older = Older::Loading;
        cx.notify();
        let (session, id, timeframe, epoch) =
            (self.session.clone(), symbol.id, self.timeframe, self.epoch);
        cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(async move { load::older(&session, id, timeframe, oldest).await })
                    .await;
            let _ = this.update(cx, |this, cx| this.older_loaded(epoch, flatten(result), cx));
        })
        .detach();
    }

    fn older_loaded(&mut self, epoch: u64, result: ApiResult<Loaded>, cx: &mut Context<Self>) {
        if epoch != self.epoch {
            return;
        }
        match result {
            Ok(loaded) if loaded.is_empty() => self.older = Older::Exhausted,
            Ok(loaded) => {
                let added = match (loaded, &mut self.series) {
                    (Loaded::Bars(bars) | Loaded::Grouped { bars, .. }, Series::Bars(held)) => {
                        data::prepend_bars(held, bars)
                    }
                    (Loaded::Ticks(ticks), Series::Ticks(held)) => data::prepend_ticks(held, ticks),
                    _ => 0,
                };
                // Nothing new means the server has nothing older (or only what is held).
                self.older = if added == 0 {
                    Older::Exhausted
                } else {
                    Older::Idle
                };
                if added > 0 {
                    // The view counts from the newest point, so it stays where it was; a
                    // construction is rebuilt from the start and may differ at its old edge.
                    self.rebuild_display();
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not load older history");
                self.older = Older::Retry(Instant::now() + Duration::from_secs(5));
            }
        }
        cx.notify();
        if matches!(self.older, Older::Idle) {
            // Still near the edge after the step? Keep going until the screen is full.
            self.load_older_if_needed(cx);
        }
    }

    // ---- live prices ----

    /// A price event (with its live bars corrected) from the session.
    pub fn on_live(&mut self, update: &LiveUpdate, cx: &mut Context<Self>) {
        let Some(id) = self.symbol.as_ref().map(|s| s.id) else {
            return;
        };
        if update.symbol_id != id {
            return;
        }
        if let Some(ask) = update.ask {
            self.ask = Some(ask);
        }
        if let Some(bid) = update.bid {
            self.bid = Some(bid);
        }
        if !matches!(self.load, Load::Ready) {
            cx.notify();
            return;
        }
        let mut appended = 0;
        if let Some(bid) = update.bid {
            let time_ms = update
                .timestamp
                .unwrap_or_else(now_ms)
                .max(self.last_time_ms);
            self.last_time_ms = time_ms;
            let tick = Tick {
                time_ms,
                price: bid,
            };
            match (&mut self.series, self.timeframe) {
                (Series::Ticks(ticks), _) => {
                    ticks.push(tick);
                    appended += 1;
                    data::trim_front(ticks, MAX_TICKS);
                }
                (Series::Bars(bars), Timeframe::Seconds(seconds)) => {
                    if data::fold_tick(bars, i64::from(seconds) * 1_000, tick) {
                        appended += 1;
                    }
                    data::trim_front(bars, MAX_BARS);
                }
                (Series::Bars(bars), Timeframe::Bars(period)) => {
                    data::touch_last_bar(bars, period.millis(), tick);
                }
                (Series::Bars(bars), timeframe @ Timeframe::Multiple(..)) => {
                    if let Some(span) = timeframe.bar_ms() {
                        data::touch_last_bar(bars, span, tick);
                    }
                }
                _ => {}
            }
        }
        match (&mut self.series, self.timeframe) {
            (Series::Bars(bars), Timeframe::Bars(period)) => {
                for (live_period, bar) in &update.bars {
                    if *live_period == period && data::apply_live_bar(bars, *bar) {
                        appended += 1;
                        self.last_time_ms = self.last_time_ms.max(bar.time_ms);
                    }
                }
                data::trim_front(bars, MAX_BARS);
            }
            (Series::Bars(bars), timeframe @ Timeframe::Multiple(period, _)) => {
                for (live_period, bar) in &update.bars {
                    if *live_period == period
                        && data::fold_group(bars, &mut self.group_tail, *bar, timeframe)
                    {
                        appended += 1;
                        self.last_time_ms = self.last_time_ms.max(bar.time_ms);
                    }
                }
                data::trim_front(bars, MAX_BARS);
            }
            _ => {}
        }
        if appended > 0 && !self.display.is_derived() {
            self.view.on_appended(appended);
        }
        self.data_changed(cx);
    }
}
