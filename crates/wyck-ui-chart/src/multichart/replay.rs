//! The layout side of a replay: every chart enters it, tells the replay what to hold, and shows
//! the cursor. Charts share one cursor, so charts of other timeframes or symbols stay in step.

use super::{MultiChart, MultiChartEvent};
use crate::chart::Chart;
use gpui::Context;
use wyck_app::replay::{Demand, ReplayEvent};

impl MultiChart {
    pub(super) fn on_replay_event(&mut self, event: &ReplayEvent, cx: &mut Context<Self>) {
        match event {
            ReplayEvent::ReachedNow => {}
            ReplayEvent::Picking => self.set_replay_pick(true, cx),
            ReplayEvent::PickCancelled => self.set_replay_pick(false, cx),
            ReplayEvent::Entered => {
                self.set_replay_pick(false, cx);
                for slot in &self.slots {
                    slot.chart.update(cx, Chart::enter_replay);
                }
                self.publish_replay_demand(cx);
            }
            ReplayEvent::Ready | ReplayEvent::Cursor | ReplayEvent::DataChanged => {
                self.reveal_replay(cx);
            }
            ReplayEvent::Exited => {
                self.set_replay_pick(false, cx);
                self.drawings
                    .update(cx, |drawings, cx| drawings.set_horizon(None, cx));
                for slot in &self.slots {
                    slot.chart.update(cx, Chart::exit_replay);
                }
                cx.notify();
            }
            ReplayEvent::Failed(message) => {
                cx.emit(MultiChartEvent::ReplayFailed(message.clone()));
            }
        }
    }

    /// Tells the replay what the charts need it to hold. Called before a replay starts, so the
    /// start can look at the symbols and timeframes of the charts.
    pub fn publish_replay_demand(&self, cx: &mut Context<Self>) {
        let demand: Vec<Demand> = self
            .slots
            .iter()
            .filter_map(|slot| slot.chart.read(cx).replay_demand())
            .collect();
        self.replay
            .update(cx, |replay, cx| replay.set_demand(demand, cx));
    }

    /// A chart changed symbol or timeframe, or one was added: while a replay runs, the replay
    /// must hold other bars and the charts show them.
    pub(super) fn replay_changed(&mut self, cx: &mut Context<Self>) {
        if self.replay.read(cx).is_on() {
            self.publish_replay_demand(cx);
            self.reveal_replay(cx);
        }
    }

    /// Writes into every chart what the replay cursor shows.
    fn reveal_replay(&self, cx: &mut Context<Self>) {
        let replay = self.replay.clone();
        for slot in &self.slots {
            slot.chart
                .update(cx, |chart, cx| chart.replay_to(&replay, cx));
        }
        // Drawings that start after the cursor would give away what comes next.
        let horizon = {
            let held = self.replay.read(cx);
            held.cursor_ms().filter(|_| !held.shows_future_drawings())
        };
        self.drawings
            .update(cx, |drawings, cx| drawings.set_horizon(horizon, cx));
    }
}

impl MultiChart {
    fn set_replay_pick(&self, pick: bool, cx: &mut Context<Self>) {
        for slot in &self.slots {
            slot.chart
                .update(cx, |chart, cx| chart.set_replay_pick(pick, cx));
        }
    }

    /// The user clicked the bar a replay starts from.
    pub(super) fn replay_picked(&mut self, time_ms: i64, cx: &mut Context<Self>) {
        if !self.replay.read(cx).is_picking() {
            return;
        }
        self.publish_replay_demand(cx);
        self.replay
            .update(cx, |replay, cx| replay.start_at(time_ms, cx));
    }
}
