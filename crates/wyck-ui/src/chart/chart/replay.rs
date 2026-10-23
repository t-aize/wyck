//! A chart in a replay: its series is the part of the history the replay cursor has reached, so
//! the indicators, scripts and exports that read the series only ever see the past.
//!
//! While a chart replays it takes nothing from the live prices and loads no history of its own;
//! the `Replay` entity of `wyck-app` holds the bars and the ticks and the chart asks it for what
//! the cursor shows.

use std::time::{Duration, Instant};

use super::events::ChartEvent;
use super::{Chart, Load, Older, bar_px_for, empty_series};
use crate::kit::{theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, div, px};
use wyck_app::market_data::now_ms;
use wyck_app::replay::{Demand, DemandKind, Replay, Store, TickStore, TickSupport};
use wyck_domain::chart::{
    Display, Geometry, MAX_BARS, MAX_TICKS, ReplayChange, ReplayFeed, ReplayInput,
    ReplaySecondsFeed, ReplaySource, ReplayTickFeed, Series, TIME_PATTERN, View,
};

/// Bars before the start a chart asks for, like a first load.
const BACKDROP_BARS: usize = 1_200;
const MAX_BACKDROP_BARS: usize = 12_000;
/// A chart with more points than this rebuilds at most this often while playing.
const BIG_SERIES: usize = 20_000;
const BIG_SERIES_EVERY: Duration = Duration::from_millis(100);

/// What the candle in progress of a chart is made of, for the banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Source {
    #[default]
    Own,
    Minutes,
    Ticks,
    /// A tick or seconds chart where the broker has no ticks.
    NoTicks,
}

/// What a chart remembers while it replays.
#[derive(Default)]
pub(super) struct ChartReplay {
    feed: ReplayFeed,
    tick_feed: ReplayTickFeed,
    seconds_feed: ReplaySecondsFeed,
    /// The cursor the series was last written for.
    cursor_ms: i64,
    source: Source,
    last_rebuild: Option<Instant>,
}

impl ChartReplay {
    fn reset(&mut self) {
        self.feed.reset();
        self.tick_feed.reset();
        self.seconds_feed.reset();
    }
}

impl Chart {
    /// "Now" for everything the chart draws that means the current moment: the replay cursor
    /// while it replays, the clock otherwise.
    pub(super) fn clock_ms(&self) -> i64 {
        self.replay
            .as_ref()
            .map_or_else(now_ms, |state| state.cursor_ms)
    }

    /// What this chart needs the replay to hold.
    pub(crate) fn replay_demand(&self) -> Option<Demand> {
        let symbol = self.symbol.as_ref()?;
        let group = usize::try_from(self.timeframe.group_size()).unwrap_or(1);
        let (kind, bar_ms) = match self.timeframe {
            wyck_domain::market::Timeframe::Ticks => (DemandKind::Ticks, None),
            wyck_domain::market::Timeframe::Seconds(seconds) => (
                DemandKind::Seconds(seconds),
                Some(i64::from(seconds) * 1_000),
            ),
            timeframe => (DemandKind::Bars(timeframe.period()?), timeframe.bar_ms()),
        };
        Some(Demand {
            symbol_id: symbol.id,
            kind,
            bar_ms,
            back_bars: (BACKDROP_BARS * group).min(MAX_BACKDROP_BARS),
        })
    }

    /// Leaves the live data: the chart is emptied and shows the replay from now on.
    pub(crate) fn enter_replay(&mut self, cx: &mut Context<Self>) {
        if self.replay.is_some() {
            return;
        }
        self.replay = Some(ChartReplay::default());
        self.reset_for_replay(cx);
    }

    /// Back to the live data.
    pub(crate) fn exit_replay(&mut self, cx: &mut Context<Self>) {
        if self.replay.take().is_none() {
            return;
        }
        self.custom.reset_outputs();
        self.reload(cx);
    }

    /// The chart changed symbol or timeframe while it replays: start from nothing, and tell the
    /// layout, which tells the replay what to hold and gives the chart its bars again.
    pub(super) fn reseed_replay(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = &mut self.replay {
            state.reset();
        }
        self.reset_for_replay(cx);
        cx.emit(ChartEvent::ReplayStale);
    }

    fn reset_for_replay(&mut self, cx: &mut Context<Self>) {
        // Answers of loads that were on their way are not for this series.
        self.epoch += 1;
        self.series = empty_series(self.timeframe);
        self.display = Display::default();
        self.view = View::new(bar_px_for(&self.settings, self.timeframe));
        self.pending_focus = None;
        self.reset_flow();
        self.custom.forget_others();
        self.custom.reset_outputs();
        // Older pages come from the replay's own backdrop, not from the live loader.
        self.older = Older::Exhausted;
        self.hover = None;
        self.drag = None;
        self.bid = None;
        self.ask = None;
        self.last_time_ms = 0;
        self.group_tail.clear();
        self.hub.set(self.id, None);
        self.load = Load::Ready;
        cx.notify();
    }

    /// Writes into the series what the replay cursor shows, and redraws.
    pub(crate) fn replay_to(&mut self, replay: &Entity<Replay>, cx: &mut Context<Self>) {
        let Some(symbol_id) = self.symbol.as_ref().map(|symbol| symbol.id) else {
            return;
        };
        let timeframe = self.timeframe;
        let revealed = {
            let held = replay.read(cx);
            let Some(cursor) = held.cursor_ms() else {
                return;
            };
            let Some(state) = self.replay.as_mut() else {
                return;
            };
            let big = self.series.len() > BIG_SERIES;
            if big
                && held.is_playing()
                && state
                    .last_rebuild
                    .is_some_and(|at| at.elapsed() < BIG_SERIES_EVERY)
            {
                return;
            }
            let went_back = cursor < state.cursor_ms;
            let result = match (timeframe, &mut self.series) {
                (wyck_domain::market::Timeframe::Ticks, Series::Ticks(out)) => {
                    Some(reveal_ticks(state, out, held.tick_store(symbol_id), cursor))
                }
                (wyck_domain::market::Timeframe::Seconds(seconds), Series::Bars(out)) => {
                    Some(reveal_seconds(
                        state,
                        out,
                        held.tick_store(symbol_id),
                        i64::from(seconds) * 1_000,
                        cursor,
                    ))
                }
                (_, Series::Bars(out)) => {
                    let Some(period) = timeframe.period() else {
                        return;
                    };
                    let Some(base) = held.store(symbol_id, period) else {
                        return;
                    };
                    let minutes = held.minutes(symbol_id);
                    let ticks = held.ticks_for(symbol_id);
                    let input = ReplayInput {
                        timeframe,
                        base: base.bars(),
                        minutes: minutes.map(Store::bars),
                        ticks: ticks.map(TickStore::ticks),
                        generation: base.generation(),
                    };
                    let change = state.feed.reveal_into(out, &input, cursor);
                    state.source = match state.feed.source() {
                        ReplaySource::Ticks => Source::Ticks,
                        ReplaySource::Minutes => Source::Minutes,
                        ReplaySource::Own => Source::Own,
                    };
                    Some(change)
                }
                _ => None,
            };
            let Some(change) = result else {
                return;
            };
            if matches!(
                timeframe,
                wyck_domain::market::Timeframe::Ticks | wyck_domain::market::Timeframe::Seconds(_)
            ) && held.tick_support(symbol_id) == TickSupport::Unavailable
            {
                state.source = Source::NoTicks;
            }
            state.cursor_ms = cursor;
            state.last_rebuild = Some(Instant::now());
            (change, went_back)
        };
        self.after_reveal(revealed.0, revealed.1, cx);
    }

    /// What follows a write to the series: the view stays where it was, the scripts forget what
    /// they made for bars that were not there, and what is drawn is made again.
    fn after_reveal(&mut self, change: ReplayChange, went_back: bool, cx: &mut Context<Self>) {
        let derived = self.display.is_derived();
        if change.new_len > change.old_len && !derived {
            self.view.on_appended(change.new_len - change.old_len);
        } else if change.new_len < change.old_len && !derived {
            self.view.on_truncated(change.old_len - change.new_len);
        }
        if went_back {
            self.custom.reset_outputs();
        }
        self.last_time_ms = self.series.last_time().unwrap_or(0);
        self.bid = match &self.series {
            Series::Bars(bars) => bars.last().map(|bar| bar.close),
            Series::Ticks(ticks) => ticks.last().map(|tick| tick.price),
        };
        self.ask = None;
        self.rebuild_display();
        let plot_w = self.geometry().plot_w();
        let len = self.shown().len();
        self.view.clamp(len, plot_w);
        cx.notify();
    }
}

/// A tick chart: the ticks before the cursor, when the replay holds them.
fn reveal_ticks(
    state: &mut ChartReplay,
    out: &mut Vec<wyck_domain::market::Tick>,
    store: Option<&TickStore>,
    cursor_ms: i64,
) -> ReplayChange {
    let Some(store) = store else {
        // No ticks: the chart stays empty, and says why.
        let old_len = out.len();
        out.clear();
        state.tick_feed.reset();
        state.source = Source::NoTicks;
        return ReplayChange {
            old_len,
            new_len: 0,
            first_changed: 0,
        };
    };
    state.source = Source::Ticks;
    state
        .tick_feed
        .reveal_into(out, store.ticks(), store.generation(), cursor_ms, MAX_TICKS)
}

/// A seconds chart: the bars the ticks before the cursor make, when the replay holds them.
fn reveal_seconds(
    state: &mut ChartReplay,
    out: &mut Vec<wyck_domain::market::Bar>,
    store: Option<&TickStore>,
    bucket_ms: i64,
    cursor_ms: i64,
) -> ReplayChange {
    let Some(store) = store else {
        let old_len = out.len();
        out.clear();
        state.seconds_feed.reset();
        state.source = Source::NoTicks;
        return ReplayChange {
            old_len,
            new_len: 0,
            first_changed: 0,
        };
    };
    state.source = Source::Ticks;
    state.seconds_feed.reveal_into(
        out,
        store.ticks(),
        store.generation(),
        bucket_ms,
        cursor_ms,
        MAX_BARS,
    )
}

impl Chart {
    /// The user is (or no longer is) choosing the bar a replay starts from.
    pub(crate) fn set_replay_pick(&mut self, pick: bool, cx: &mut Context<Self>) {
        if self.replay_pick != pick {
            self.replay_pick = pick;
            cx.notify();
        }
    }

    /// What the chart says about the replay: how to pick the start, then the time it shows and
    /// what its candles are made of.
    pub(super) fn replay_banner(&self, _geometry: &Geometry) -> Option<AnyElement> {
        let (strong, rest) = if self.replay_pick {
            (
                "Replay".to_owned(),
                "Click a bar to start from it. Esc cancels".to_owned(),
            )
        } else if let Some(state) = &self.replay {
            let time = self.settings.zone.format(state.cursor_ms, TIME_PATTERN);
            let note = match state.source {
                Source::Ticks => "real ticks",
                Source::Minutes => "minute bars",
                Source::Own => "candles formed along their own path",
                Source::NoTicks => "no tick data at this date",
            };
            ("Replay".to_owned(), format!("{time}  ({note})"))
        } else {
            return None;
        };
        Some(
            div()
                .absolute()
                .top(px(34.0))
                .left_0()
                .right(px(wyck_domain::chart::axis_w()))
                .flex()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_1()
                        .rounded_full()
                        .bg(theme::surface_alpha(0.92))
                        .border_1()
                        .border_color(theme::border_subtle())
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(
                            div()
                                .text_color(theme::accent())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(strong),
                        )
                        .child(rest),
                )
                .into_any_element(),
        )
    }
}
