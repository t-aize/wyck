//! A chart in a replay: its series is the part of the history the replay cursor has reached, so
//! the indicators, scripts and exports that read the series only ever see the past.
//!
//! While a chart replays it takes nothing from the live prices and loads no history of its own;
//! the `Replay` entity of `wyck-app` holds the bars and the chart asks it for what the cursor shows.

use super::events::ChartEvent;
use super::{Chart, Load, Older, bar_px_for, empty_series};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, div, px};
use wyck_app::market_data::now_ms;
use wyck_app::replay::{Demand, Replay, Store};
use wyck_chart::{Display, Geometry, ReplayFeed, ReplayInput, Series, TIME_PATTERN, View};
use wyck_ui_kit::{theme, tokens};

/// Bars before the start a chart asks for, like a first load.
const BACKDROP_BARS: usize = 1_200;
const MAX_BACKDROP_BARS: usize = 12_000;

/// What a chart remembers while it replays.
#[derive(Default)]
pub(super) struct ChartReplay {
    feed: ReplayFeed,
    /// The cursor the series was last written for.
    cursor_ms: i64,
    /// The candle in progress is drawn along its own path: the symbol has no minute bars.
    approximated: bool,
}

impl Chart {
    /// Whether this timeframe can be replayed: tick and second charts are built from ticks.
    #[must_use]
    pub(crate) fn replay_supported(&self) -> bool {
        self.timeframe.period().is_some()
    }

    /// "Now" for everything the chart draws that means the current moment: the replay cursor
    /// while it replays, the clock otherwise.
    pub(super) fn clock_ms(&self) -> i64 {
        self.replay
            .as_ref()
            .map_or_else(now_ms, |state| state.cursor_ms)
    }

    /// What this chart needs the replay to hold, or `None` when it cannot replay.
    pub(crate) fn replay_demand(&self) -> Option<Demand> {
        let symbol = self.symbol.as_ref()?;
        let period = self.timeframe.period()?;
        let group = usize::try_from(self.timeframe.group_size()).unwrap_or(1);
        Some(Demand {
            symbol_id: symbol.id,
            period,
            bar_ms: self.timeframe.bar_ms()?,
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
            state.feed.reset();
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
        let (Some(symbol_id), Some(period)) = (
            self.symbol.as_ref().map(|symbol| symbol.id),
            self.timeframe.period(),
        ) else {
            return;
        };
        let timeframe = self.timeframe;
        let revealed = {
            let held = replay.read(cx);
            let (Some(cursor), Some(base)) = (held.cursor_ms(), held.store(symbol_id, period))
            else {
                return;
            };
            let minutes = held.minutes(symbol_id);
            let input = ReplayInput {
                timeframe,
                base: base.bars(),
                minutes: minutes.map(Store::bars),
                generation: base.generation(),
            };
            let (Some(state), Series::Bars(bars)) = (self.replay.as_mut(), &mut self.series) else {
                return;
            };
            let went_back = cursor < state.cursor_ms;
            let change = state.feed.reveal_into(bars, &input, cursor);
            state.cursor_ms = cursor;
            state.approximated = minutes.is_none();
            (change, went_back)
        };
        let (change, went_back) = revealed;
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
            Series::Ticks(_) => None,
        };
        self.ask = None;
        self.rebuild_display();
        let plot_w = self.geometry().plot_w();
        let len = self.shown().len();
        self.view.clamp(len, plot_w);
        cx.notify();
    }
}

impl Chart {
    /// The user is (or no longer is) choosing the bar a replay starts from.
    pub(crate) fn set_replay_pick(&mut self, pick: bool, cx: &mut Context<Self>) {
        if self.replay_pick != pick {
            self.replay_pick = pick;
            cx.notify();
        }
    }

    /// What the chart says about the replay: how to pick the start, then the time it shows.
    pub(super) fn replay_banner(&self, _geometry: &Geometry) -> Option<AnyElement> {
        let (strong, rest) = if self.replay_pick {
            (
                "Replay".to_owned(),
                "Click a bar to start from it. Esc cancels".to_owned(),
            )
        } else if let Some(state) = &self.replay {
            let time = self.settings.zone.format(state.cursor_ms, TIME_PATTERN);
            let note = if !self.replay_supported() {
                "needs a timeframe of 1 minute or more"
            } else if state.approximated {
                "candles formed along their own path"
            } else {
                ""
            };
            (
                "Replay".to_owned(),
                if note.is_empty() {
                    time
                } else {
                    format!("{time}  ({note})")
                },
            )
        } else {
            return None;
        };
        Some(
            div()
                .absolute()
                .top(px(34.0))
                .left_0()
                .right(px(wyck_chart::axis_w()))
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
