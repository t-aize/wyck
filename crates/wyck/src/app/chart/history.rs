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
        if let Some(cursor) = self.replay_cursor_ms.filter(|_| self.symbol.is_some()) {
            self.replay_seek(cursor, cx);
            return;
        }
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
    //
    // A chart only ever holds the bars still to reveal (`self.replay`); the clock
    // (cursor, speed, play state) is shared across every chart in the layout and lives
    // on `Dashboard`, so a multichart layout replays as one instead of each chart
    // running its own independent clock. See the module docs on `super::replay`.

    /// Whether this chart has nothing left to reveal: no replay running at all counts as
    /// exhausted too, so a chart that could not start one never blocks the shared
    /// clock from noticing every other chart is done.
    pub fn replay_is_exhausted(&self) -> bool {
        if matches!(self.load, Load::Failed(_)) {
            return true;
        }
        self.replay_cursor_ms.is_none_or(|_| {
            self.replay_loaded_until_ms >= self.replay_end_ms
                && self
                    .replay
                    .as_ref()
                    .is_some_and(super::replay::ReplayFeed::is_exhausted)
        })
    }

    pub fn replay_next_time(&self) -> Option<i64> {
        self.replay.as_ref()?.next_time()
    }

    pub fn replay_needs_data(&self) -> bool {
        self.replay_cursor_ms.is_some()
            && !matches!(self.load, Load::Failed(_))
            && (self.replay.is_none()
                || (self.replay_loading_more
                    && self
                        .replay
                        .as_ref()
                        .is_some_and(super::replay::ReplayFeed::is_exhausted)))
    }

    /// "Now" for anything the chart draws that means "the current moment": the real wall
    /// clock, unless replaying, in which case the shared replay clock's own position.
    /// This is what lets the "time left in this bar" countdown mean something during a
    /// replay instead of comparing a historical bar's time to today's date and never
    /// showing at all.
    pub(super) fn now_for_display(&self) -> i64 {
        self.replay_cursor_ms.unwrap_or_else(now_ms)
    }

    /// Moves to `start_ms`: reloads the chart truncated to that point and prepares a
    /// fresh [`super::replay::ReplayFeed`] for everything after it, replacing any replay
    /// already in progress. This is how a replay is started, and also how "go to a
    /// different date" and "jump to start" both work: see the module docs on
    /// [`super::replay`] for why seeking reloads rather than scrubbing bars already
    /// shown. All timeframes use the same bid tick stream after the pick.
    pub fn replay_seek(&mut self, start_ms: i64, cx: &mut Context<Self>) {
        let timeframe = self.timeframe;
        let period = timeframe
            .period()
            .unwrap_or(wyck_openapi::market::Period::M1);
        let Some(symbol) = self.symbol.clone() else {
            return;
        };
        self.replay = None;
        self.replay_cursor_ms = Some(start_ms);
        self.replay_loaded_until_ms = 0;
        self.replay_end_ms = 0;
        self.replay_loading_more = false;
        self.epoch += 1;
        let epoch = self.epoch;
        self.hub.set(self.id, None);
        self.series = empty_series(self.timeframe);
        self.display = Default::default();
        self.view.jump_to_latest();
        self.pending_focus = Some(start_ms);
        self.reset_flow();
        self.hover = None;
        self.drag = None;
        self.bid = None;
        self.ask = None;
        self.group_tail.clear();
        self.last_time_ms = 0;
        self.older = Older::Idle;
        self.load = Load::Loading;
        cx.notify();
        let session = self.session.clone();
        let catalog = crate::app::market_data::catalog(cx);
        let now = now_ms();
        const TICK_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
        let until = now.min(start_ms.saturating_add(TICK_WINDOW_MS));
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                let past = match &catalog {
                    Some(catalog) => {
                        load::initial_cached(
                            &session,
                            catalog.clone(),
                            symbol.id,
                            timeframe,
                            start_ms.saturating_sub(1),
                        )
                        .await?
                    }
                    None => {
                        load::initial(&session, symbol.id, timeframe, start_ms.saturating_sub(1))
                            .await?
                    }
                };
                let future = match (timeframe.period(), &catalog) {
                    (Some(period), Some(catalog)) => {
                        load::gap_cached(
                            &session,
                            catalog.clone(),
                            symbol.id,
                            Timeframe::Bars(period),
                            start_ms,
                            until,
                        )
                        .await?
                    }
                    (Some(period), None) => {
                        load::gap(
                            &session,
                            symbol.id,
                            Timeframe::Bars(period),
                            start_ms,
                            until,
                        )
                        .await?
                    }
                    (None, _) => Loaded::Bars(Vec::new()),
                };
                let ticks = match &catalog {
                    Some(catalog) => {
                        load::gap_cached(
                            &session,
                            catalog.clone(),
                            symbol.id,
                            Timeframe::Ticks,
                            start_ms,
                            until,
                        )
                        .await?
                    }
                    None => {
                        load::gap(&session, symbol.id, Timeframe::Ticks, start_ms, until).await?
                    }
                };
                Ok::<_, wyck_openapi::OpenApiError>((past, future, ticks))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                this.replay_seek_loaded(
                    epoch,
                    symbol.id,
                    period,
                    (until, now),
                    flatten(result),
                    cx,
                );
            });
        })
        .detach();
    }

    fn replay_seek_loaded(
        &mut self,
        epoch: u64,
        symbol_id: i64,
        period: wyck_openapi::market::Period,
        bounds: (i64, i64),
        result: ApiResult<(Loaded, Loaded, Loaded)>,
        cx: &mut Context<Self>,
    ) {
        if epoch != self.epoch {
            return;
        }
        match result {
            Ok((past, future, ticks)) => {
                let future_bars = match future {
                    Loaded::Bars(bars) | Loaded::Grouped { bars, .. } => bars,
                    Loaded::Ticks(_) => Vec::new(),
                };
                let Loaded::Ticks(ticks) = ticks else {
                    unreachable!()
                };
                self.replay = Some(super::replay::ReplayFeed::new(
                    symbol_id,
                    period,
                    future_bars,
                    ticks,
                ));
                self.replay_loaded_until_ms = bounds.0;
                self.replay_end_ms = bounds.1;
                self.initial_loaded(epoch, Ok(past), cx);
                if let Some(cursor) = self.replay_cursor_ms {
                    self.replay_reveal_one_to(cursor, cx);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not load the chart history for replay");
                self.load = Load::Failed(error.to_string().into());
                cx.notify();
            }
        }
    }

    /// Reveals at most one tick at or before the shared replay cursor.
    pub fn replay_reveal_one_to(&mut self, cursor_ms: i64, cx: &mut Context<Self>) {
        self.replay_cursor_ms = Some(cursor_ms);
        if self.replay.is_none() {
            return;
        }
        let updates = self
            .replay
            .as_mut()
            .map(|feed| feed.reveal_one(cursor_ms))
            .unwrap_or_default();
        if updates.is_empty() {
            self.replay_load_more(cx);
            cx.notify();
            return;
        }
        for update in &updates {
            self.apply_live(update, cx);
        }
        self.replay_load_more(cx);
    }

    fn replay_load_more(&mut self, cx: &mut Context<Self>) {
        if self.replay_loading_more
            || self.replay_loaded_until_ms >= self.replay_end_ms
            || self
                .replay_cursor_ms
                .is_none_or(|cursor| cursor < self.replay_loaded_until_ms - 60_000)
        {
            return;
        }
        let Some(symbol_id) = self.symbol.as_ref().map(|symbol| symbol.id) else {
            return;
        };
        let from = self.replay_loaded_until_ms.saturating_add(1);
        let until = self
            .replay_end_ms
            .min(from.saturating_add(24 * 60 * 60 * 1_000));
        let epoch = self.epoch;
        let period = self.timeframe.period();
        let session = self.session.clone();
        let catalog = crate::app::market_data::catalog(cx);
        self.replay_loading_more = true;
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                let bars = match (period, &catalog) {
                    (Some(period), Some(catalog)) => {
                        load::gap_cached(
                            &session,
                            catalog.clone(),
                            symbol_id,
                            Timeframe::Bars(period),
                            from,
                            until,
                        )
                        .await?
                    }
                    (Some(period), None) => {
                        load::gap(&session, symbol_id, Timeframe::Bars(period), from, until).await?
                    }
                    (None, _) => Loaded::Bars(Vec::new()),
                };
                let ticks = match catalog {
                    Some(catalog) => {
                        load::gap_cached(
                            &session,
                            catalog,
                            symbol_id,
                            Timeframe::Ticks,
                            from,
                            until,
                        )
                        .await?
                    }
                    None => load::gap(&session, symbol_id, Timeframe::Ticks, from, until).await?,
                };
                Ok::<_, wyck_openapi::OpenApiError>((bars, ticks))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.epoch != epoch {
                    return;
                }
                this.replay_loading_more = false;
                match flatten(result) {
                    Ok((bars, Loaded::Ticks(ticks))) => {
                        if let Some(feed) = &mut this.replay {
                            match bars {
                                Loaded::Bars(bars) | Loaded::Grouped { bars, .. } => {
                                    feed.append_bars(bars)
                                }
                                Loaded::Ticks(_) => {}
                            }
                            feed.append_ticks(ticks);
                        }
                        this.replay_loaded_until_ms = until;
                        if let Some(cursor) = this.replay_cursor_ms {
                            this.replay_reveal_one_to(cursor, cx);
                        }
                    }
                    Err(error) => tracing::warn!(%error, "could not extend replay ticks"),
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Leaves Replay: resubscribes to the real live feed and reloads from scratch, the
    /// simplest way back to an honestly-live chart.
    pub fn stop_replay(&mut self, cx: &mut Context<Self>) {
        self.replay = None;
        self.replay_cursor_ms = None;
        self.replay_loading_more = false;
        if let Some(symbol) = &self.symbol {
            self.hub.set(
                self.id,
                Some(Wish::symbol(symbol.id, self.timeframe.period())),
            );
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
        if self.replay_cursor_ms.is_some() {
            return;
        }
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
        if self.replay_cursor_ms.is_some() {
            return;
        }
        self.apply_live(update, cx);
    }

    fn apply_live(&mut self, update: &LiveUpdate, cx: &mut Context<Self>) {
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
