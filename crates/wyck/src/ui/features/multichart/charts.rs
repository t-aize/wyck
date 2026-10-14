//! Adding charts and giving them their symbol, timeframe, hours and data.

use super::{MultiChart, MultiChartEvent, Slot, SymbolRef};
use crate::app::market_data::live::LiveUpdate;
use crate::app::workspace::ChartState;
use crate::ui::features::chart::events::ChartEvent;
use crate::ui::features::chart::{Chart, ChartLine};
use gpui::Context;
use gpui::prelude::*;
use std::collections::HashMap;
use wyck_market::SpotEvent;
use wyck_market::Timeframe;

impl MultiChart {
    pub(super) fn add_chart(&mut self, state: ChartState, cx: &mut Context<Self>) {
        self.next_id += 1;
        let (session, hub, id) = (self.session.clone(), self.hub.clone(), self.next_id);
        let ChartState {
            timeframe,
            settings,
            symbol,
        } = state;
        let max_studies = self
            .workspace
            .read(cx)
            .preferences()
            .limits
            .studies_per_chart;
        let chart = cx.new(|cx| Chart::new(session, hub, id, timeframe, settings, max_studies, cx));
        let drawings = self.drawings.clone();
        chart.update(cx, |chart, cx| chart.attach_drawings(drawings, cx));
        if !self.symbol_ids.is_empty() {
            let ids = self.symbol_ids.clone();
            chart.update(cx, |chart, cx| chart.set_symbol_ids(ids, cx));
        }
        // A linked symbol comes from the charts already open.
        let shared = self
            .sync
            .symbol
            .then(|| {
                self.slots
                    .first()
                    .and_then(|s| s.chart.read(cx).symbol().cloned())
            })
            .flatten();
        if let Some(symbol) = &shared {
            let symbol = symbol.clone();
            chart.update(cx, |chart, cx| {
                chart.set_symbol(
                    symbol.id,
                    symbol.name,
                    symbol.digits,
                    symbol.pip_position,
                    cx,
                );
            });
        }
        let events = cx.subscribe(&chart, |this, source, event: &ChartEvent, cx| {
            this.on_chart_event(&source, event, cx);
        });
        self.slots.push(Slot {
            chart,
            wanted_symbol: if shared.is_some() { None } else { symbol },
            _events: events,
        });
    }

    /// Shows a symbol on chart `target`, or on every chart when the symbol is linked.
    pub fn set_symbol(&mut self, target: usize, symbol: SymbolRef, cx: &mut Context<Self>) {
        for index in super::links::symbol_targets(&self.sync, target, self.slots.len()) {
            let slot = &mut self.slots[index];
            slot.wanted_symbol = None;
            let symbol = symbol.clone();
            slot.chart.update(cx, |chart, cx| {
                chart.set_symbol(
                    symbol.id,
                    symbol.name,
                    symbol.digits,
                    symbol.pip_position,
                    cx,
                );
            });
            if let Some(lines) = self.lines.get(&symbol.id).cloned() {
                self.slots[index]
                    .chart
                    .update(cx, |chart, cx| chart.set_lines(lines, cx));
            }
        }
        self.persist(cx);
        cx.emit(MultiChartEvent::ActiveChanged);
        cx.notify();
    }

    /// Shows a symbol on one chart only, whatever the links (a saved symbol being restored).
    pub fn restore_symbol(&mut self, index: usize, symbol: SymbolRef, cx: &mut Context<Self>) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        slot.wanted_symbol = None;
        slot.chart.update(cx, |chart, cx| {
            chart.set_symbol(
                symbol.id,
                symbol.name,
                symbol.digits,
                symbol.pip_position,
                cx,
            );
        });
        cx.emit(MultiChartEvent::ActiveChanged);
        cx.notify();
    }

    /// The broker said how a symbol is quoted.
    /// The symbols of the account arrived: `ids` has the id of each by name (see
    /// [`wyck_indicators::custom::other::key_of`]).
    pub fn set_symbol_ids(&mut self, ids: HashMap<String, i64>, cx: &mut Context<Self>) {
        self.symbol_ids = std::sync::Arc::new(ids);
        for slot in &self.slots {
            let ids = self.symbol_ids.clone();
            slot.chart
                .update(cx, |chart, cx| chart.set_symbol_ids(ids, cx));
        }
    }

    /// A symbol's trading hours arrived.
    pub fn set_hours(&mut self, id: i64, hours: wyck_market::TradingHours, cx: &mut Context<Self>) {
        self.hours.insert(id, std::sync::Arc::new(hours));
        self.give_hours(cx);
    }

    /// Hands each chart the hours of the symbol it shows.
    pub(super) fn give_hours(&self, cx: &mut Context<Self>) {
        for slot in &self.slots {
            let id = slot.chart.read(cx).symbol().map(|s| s.id);
            if let Some(id) = id {
                let hours = self.hours.get(&id).cloned();
                slot.chart
                    .update(cx, |chart, cx| chart.set_hours(id, hours, cx));
            }
        }
    }

    pub fn set_quote_details(
        &mut self,
        id: i64,
        digits: u32,
        pip_position: i64,
        cx: &mut Context<Self>,
    ) {
        for slot in &self.slots {
            slot.chart.update(cx, |chart, cx| {
                chart.set_quote_details(id, digits, pip_position, cx)
            });
        }
    }

    /// Sets the timeframe of the active chart, or of every chart when the interval is linked.
    pub fn set_timeframe(&mut self, timeframe: Timeframe, cx: &mut Context<Self>) {
        for index in super::links::timeframe_targets(&self.sync, self.active, self.slots.len()) {
            self.slots[index]
                .chart
                .update(cx, |chart, cx| chart.set_timeframe(timeframe, cx));
        }
        self.persist(cx);
        cx.notify();
    }

    pub fn on_spot(&mut self, spot: &SpotEvent, cx: &mut Context<Self>) {
        let update = LiveUpdate::new(spot, self.tracker.apply(spot));
        for slot in &self.slots {
            slot.chart
                .update(cx, |chart, cx| chart.on_live(&update, cx));
        }
    }

    pub fn on_ready(&mut self, cx: &mut Context<Self>) {
        self.tracker.clear();
        for slot in &self.slots {
            slot.chart.update(cx, |chart, cx| chart.on_ready(cx));
        }
    }

    /// The orders, positions and alerts of each symbol, shown on its charts.
    pub fn set_lines(&mut self, lines: HashMap<i64, Vec<ChartLine>>, cx: &mut Context<Self>) {
        for slot in &self.slots {
            let symbol = slot.chart.read(cx).symbol().map(|s| s.id);
            let wanted = symbol
                .and_then(|id| lines.get(&id).cloned())
                .unwrap_or_default();
            slot.chart
                .update(cx, |chart, cx| chart.set_lines(wanted, cx));
        }
        self.lines = lines;
    }
}
