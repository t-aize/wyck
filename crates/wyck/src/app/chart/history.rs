//! Getting the prices: the first load, older pages as the user scrolls back, the gap after a lost
//! connection, and the live prices.

use std::time::{Duration, Instant};

use gpui::Context;
use wyck_openapi::Result as ApiResult;
use wyck_openapi::market::Tick;

use super::data::{self, MAX_BARS, MAX_TICKS, Series};
use super::live::{LiveUpdate, Wish};
use super::load::{self, Loaded};
use super::view::View;
use super::{Chart, Load, Older, Timeframe, empty_series, flatten, now_ms};
use crate::app::runtime;

impl Chart {
    /// ATR for this chart's symbol and the selected chart or dedicated timeframe.
    pub fn atr_value(&self, settings: &wyck_chart::study::atr_stop::AtrStop) -> Option<f64> {
        let timeframe = settings
            .timeframe
            .as_deref()
            .map(Timeframe::from_code)
            .unwrap_or(Some(self.timeframe))?;
        let bars = if timeframe == self.timeframe {
            match &self.series {
                Series::Bars(bars) => bars,
                Series::Ticks(_) => return None,
            }
        } else {
            let id = self.symbol.as_ref()?.id;
            let (bars, loaded_at) = self.atr_history.get(&(id, timeframe))?;
            if now_ms().saturating_sub(*loaded_at) > 30_000 {
                return None;
            }
            bars
        };
        let last = bars.last()?;
        let last_is_open = timeframe.group_key(last.time_ms) == timeframe.group_key(now_ms());
        settings.value(bars, last_is_open)
    }

    /// Fetches an ATR timeframe that differs from the visible chart.
    pub fn request_atr(
        &mut self,
        settings: &wyck_chart::study::atr_stop::AtrStop,
        cx: &mut Context<Self>,
    ) {
        let Some(timeframe) = settings.timeframe.as_deref().and_then(Timeframe::from_code) else {
            return;
        };
        if timeframe == self.timeframe {
            return;
        }
        let Some(symbol) = self.symbol.as_ref() else {
            return;
        };
        let key = (symbol.id, timeframe);
        if self.atr_loading.contains(&key)
            || self
                .atr_history
                .get(&key)
                .is_some_and(|(_, at)| now_ms().saturating_sub(*at) < 30_000)
        {
            return;
        }
        self.atr_loading.insert(key);
        let session = self.session.clone();
        let catalog = crate::app::market_data::catalog(cx);
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                match catalog {
                    Some(catalog) => {
                        load::initial_cached(&session, catalog, key.0, timeframe, now_ms()).await
                    }
                    None => load::initial(&session, key.0, timeframe, now_ms()).await,
                }
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                this.atr_loading.remove(&key);
                let bars = match flatten(result) {
                    Ok(Loaded::Bars(bars) | Loaded::Grouped { bars, .. }) => Some(bars),
                    _ => None,
                };
                if let Some(bars) = bars {
                    this.atr_history.insert(key, (bars, now_ms()));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Throws the data away and loads the current symbol and timeframe from scratch.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        let epoch = self.epoch;
        self.series = empty_series(self.timeframe);
        self.display = Default::default();
        self.view = View::new(super::bar_px_for(&self.settings, self.timeframe));
        self.pending_focus = None;
        self.reset_flow();
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
        let catalog = crate::app::market_data::catalog(cx);
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                match catalog {
                    Some(catalog) => {
                        load::initial_cached(&session, catalog, id, timeframe, now_ms()).await
                    }
                    None => load::initial(&session, id, timeframe, now_ms()).await,
                }
            })
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
                self.focus_pending_time(cx);
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

    // ---- replay ----

    /// Whether a replay is currently active (played, paused, or mid-seek).
    pub fn is_replaying(&self) -> bool {
        self.replay.is_some()
    }

    /// A read-only snapshot of the active replay, for the control bar to render. `None`
    /// while no replay is active, or while a seek is in flight and has not landed yet.
    pub fn replay_view(&self) -> Option<super::replay::ReplayView> {
        let state = self.replay.as_ref()?;
        Some(super::replay::ReplayView {
            speed: state.session.speed(),
            playing: state.session.is_playing(),
            cursor_ms: state.session.cursor_ms(),
            start_ms: state.session.start_ms(),
            exhausted: state.feed.is_exhausted(),
        })
    }

    /// Moves to `start_ms`: reloads the chart truncated to that point and prepares a
    /// fresh [`super::replay::ReplayFeed`] for everything after it, replacing any replay
    /// already in progress. This is how a replay is started, and also how "go to a
    /// different date" and "jump to start" both work: see the module docs on
    /// [`super::replay`] for why seeking reloads rather than scrubbing bars already shown.
    /// `default_speed` seeds the new session's speed when there was no replay already
    /// running to carry a speed over from (a fresh start, not a seek within one); a
    /// speed already in progress always wins over it. Only bar timeframes are supported;
    /// a no-op otherwise.
    pub fn replay_seek(&mut self, start_ms: i64, default_speed: Option<f64>, cx: &mut Context<Self>) {
        let Timeframe::Bars(period) = self.timeframe else {
            return;
        };
        let Some(symbol) = self.symbol.clone() else {
            return;
        };
        let speed = self
            .replay
            .take()
            .map(|state| state.session.speed())
            .or(default_speed);
        self.epoch += 1;
        let epoch = self.epoch;
        self.hub.set(self.id, None);
        self.older = Older::Idle;
        self.load = Load::Loading;
        cx.notify();
        let session = self.session.clone();
        let catalog = crate::app::market_data::catalog(cx);
        let now = now_ms();
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                let past = match &catalog {
                    Some(catalog) => {
                        load::initial_cached(&session, catalog.clone(), symbol.id, Timeframe::Bars(period), start_ms)
                            .await?
                    }
                    None => load::initial(&session, symbol.id, Timeframe::Bars(period), start_ms).await?,
                };
                let future = match &catalog {
                    Some(catalog) => {
                        load::gap_cached(&session, catalog.clone(), symbol.id, Timeframe::Bars(period), start_ms, now)
                            .await?
                    }
                    None => load::gap(&session, symbol.id, Timeframe::Bars(period), start_ms, now).await?,
                };
                Ok::<_, wyck_openapi::OpenApiError>((past, future))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                this.replay_seek_loaded(epoch, symbol.id, period, start_ms, speed, flatten(result), cx);
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    fn replay_seek_loaded(
        &mut self,
        epoch: u64,
        symbol_id: i64,
        period: wyck_openapi::market::Period,
        start_ms: i64,
        previous_speed: Option<f64>,
        result: ApiResult<(Loaded, Loaded)>,
        cx: &mut Context<Self>,
    ) {
        if epoch != self.epoch {
            return;
        }
        match result {
            Ok((past, future)) => {
                let future_bars = match future {
                    Loaded::Bars(bars) | Loaded::Grouped { bars, .. } => bars,
                    Loaded::Ticks(_) => Vec::new(),
                };
                let mut session = wyck_market_data::replay::ReplaySession::new(start_ms, period.millis());
                if let Some(speed) = previous_speed {
                    session.set_speed(speed, now_ms());
                }
                self.replay = Some(super::replay::ReplayState {
                    session,
                    feed: super::replay::ReplayFeed::new(symbol_id, period, future_bars),
                });
                self.initial_loaded(epoch, Ok(past), cx);
            }
            Err(error) => {
                tracing::warn!(%error, "could not load the chart history for replay");
                self.load = Load::Failed(error.to_string().into());
                cx.notify();
            }
        }
    }

    /// Reveals every bar the feed holds up to `cursor_ms`, applying each exactly as a
    /// live update. A no-op while no replay is active (including mid-seek, when the
    /// previous feed was already cleared but the new one has not landed yet).
    fn replay_reveal(&mut self, cursor_ms: i64, cx: &mut Context<Self>) {
        let Some(state) = &mut self.replay else {
            return;
        };
        let updates = state.feed.reveal(cursor_ms);
        if updates.is_empty() {
            return;
        }
        for update in &updates {
            self.on_live(update, cx);
        }
    }

    /// Advances the replay clock to `wall_now_ms` and reveals whatever it newly crossed.
    /// A no-op while there is no replay, or it is paused. Pauses itself once every held
    /// bar has been revealed.
    pub fn replay_tick(&mut self, wall_now_ms: i64, cx: &mut Context<Self>) {
        let Some(state) = &mut self.replay else {
            return;
        };
        if !state.session.is_playing() {
            return;
        }
        state.session.advance_to(wall_now_ms);
        let cursor = state.session.cursor_ms();
        self.replay_reveal(cursor, cx);
        if let Some(state) = &mut self.replay
            && state.feed.is_exhausted()
        {
            state.session.pause();
        }
        cx.notify();
    }

    /// Starts or stops the replay clock.
    pub fn replay_play_pause(&mut self, wall_now_ms: i64, cx: &mut Context<Self>) {
        let Some(state) = &mut self.replay else {
            return;
        };
        if state.session.is_playing() {
            state.session.pause();
        } else {
            state.session.play(wall_now_ms);
        }
        cx.notify();
    }

    /// Moves forward by exactly one bar and reveals it immediately, whether or not the
    /// replay is playing.
    pub fn replay_step_forward(&mut self, cx: &mut Context<Self>) {
        let Some(state) = &mut self.replay else {
            return;
        };
        state.session.step(true);
        let cursor = state.session.cursor_ms();
        self.replay_reveal(cursor, cx);
        cx.notify();
    }

    /// Sets the playback speed multiplier (clamped by [`wyck_market_data::replay::ReplaySession::set_speed`]).
    pub fn replay_set_speed(&mut self, speed: f64, wall_now_ms: i64, cx: &mut Context<Self>) {
        let Some(state) = &mut self.replay else {
            return;
        };
        state.session.set_speed(speed, wall_now_ms);
        cx.notify();
    }

    /// Returns to the point this replay currently started from, undoing forward progress.
    pub fn replay_jump_to_start(&mut self, cx: &mut Context<Self>) {
        let Some(start_ms) = self.replay.as_ref().map(|state| state.session.start_ms()) else {
            return;
        };
        self.replay_seek(start_ms, None, cx);
    }

    /// Leaves Replay: resubscribes to the real live feed and reloads from scratch, the
    /// simplest way back to an honestly-live chart.
    pub fn stop_replay(&mut self, cx: &mut Context<Self>) {
        self.replay = None;
        if let Some(symbol) = &self.symbol {
            self.hub
                .set(self.id, Some(Wish::symbol(symbol.id, self.timeframe.period())));
        }
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
        self.flow_gap_open();
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
        let catalog = crate::app::market_data::catalog(cx);
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                match catalog {
                    Some(catalog) => {
                        load::gap_cached(&session, catalog, id, timeframe, from, to).await
                    }
                    None => load::gap(&session, id, timeframe, from, to).await,
                }
            })
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
        // A footprint needs the flow of the bars it shows too.
        self.ensure_flow(cx);
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
            self.pending_focus = None;
            return;
        }
        let plot_w = self.geometry().plot_w();
        let shown = self.shown().len();
        let (first, _) = self.view.visible(shown, plot_w);
        let seeking_older = self
            .pending_focus
            .is_some_and(|time| self.series.first_time().is_some_and(|oldest| time < oldest));
        if !seeking_older && (first as f64) > self.view.span(plot_w) * 0.5 {
            return;
        }
        let (Some(symbol), Some(oldest)) = (&self.symbol, self.series.first_time()) else {
            return;
        };
        self.older = Older::Loading;
        cx.notify();
        let (session, id, timeframe, epoch) =
            (self.session.clone(), symbol.id, self.timeframe, self.epoch);
        let catalog = crate::app::market_data::catalog(cx);
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                match catalog {
                    Some(catalog) => {
                        load::older_cached(&session, catalog, id, timeframe, oldest).await
                    }
                    None => load::older(&session, id, timeframe, oldest).await,
                }
            })
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
                    self.focus_pending_time(cx);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not load older history");
                self.older = Older::Retry(Instant::now() + Duration::from_secs(5));
            }
        }
        cx.notify();
        if matches!(self.older, Older::Exhausted) {
            self.pending_focus = None;
        }
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
        self.feed_flow(update);
        if appended > 0 && !self.display.is_derived() {
            self.view.on_appended(appended);
        }
        self.data_changed(cx);
    }
}
