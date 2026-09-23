//! Several charts on one screen, arranged by a layout, and linked the way the user chooses.
//!
//! For now every chart shows the same symbol, but each has its own timeframe and chart type. The
//! links are the ones a trading platform offers:
//!
//! - **Interval**: one timeframe for every chart.
//! - **Crosshair**: the pointer over one chart shows as a crosshair on the others, at the same
//!   time and price.
//! - **Time**: scrolling one chart moves the others to the same time at the right edge.
//! - **Date range**: zooming and scrolling one chart gives the others the same span of time.
//!
//! The charts do not know about each other. Each one reports what the user did as a
//! [`ChartEvent`], and this view applies the links, which keeps a chart free of loops: a chart
//! that is made to follow another never reports it.

pub mod icon;
pub mod layouts;

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Context, Entity, MouseButton, SharedString, Subscription, Window, div, px, relative};
use wyck::openapi::market::SpotEvent;
use wyck::openapi::session::Session;

use self::layouts::{LayoutKey, layout};
use super::chart::{Chart, ChartEvent, LiveHub, Timeframe};
use super::theme;

/// Timeframes given to charts added by a bigger layout, when the timeframe is not linked. They
/// differ, so a new chart shows something else than the first one.
const NEW_CHART_TIMEFRAMES: [Timeframe; 8] = [
    Timeframe::Bars(wyck::openapi::market::Period::M5),
    Timeframe::Bars(wyck::openapi::market::Period::M15),
    Timeframe::Bars(wyck::openapi::market::Period::H1),
    Timeframe::Bars(wyck::openapi::market::Period::H4),
    Timeframe::Bars(wyck::openapi::market::Period::D1),
    Timeframe::Bars(wyck::openapi::market::Period::M1),
    Timeframe::Bars(wyck::openapi::market::Period::M30),
    Timeframe::Bars(wyck::openapi::market::Period::W1),
];

/// What is linked between the charts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Links {
    pub interval: bool,
    pub crosshair: bool,
    pub time: bool,
    pub range: bool,
}

impl Default for Links {
    fn default() -> Self {
        Self {
            interval: false,
            crosshair: true,
            time: true,
            range: false,
        }
    }
}

/// One of the links, for switching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Interval,
    Crosshair,
    Time,
    Range,
}

struct Slot {
    chart: Entity<Chart>,
    _events: Subscription,
}

/// The symbol every chart shows: its id, name and number of decimals.
struct Symbol {
    id: i64,
    name: SharedString,
    digits: u32,
}

pub struct MultiChart {
    session: Session,
    hub: Rc<LiveHub>,
    next_id: u64,
    key: LayoutKey,
    slots: Vec<Slot>,
    active: usize,
    sync: Links,
    symbol: Option<Symbol>,
}

impl MultiChart {
    pub fn new(session: Session, cx: &mut Context<Self>) -> Self {
        let mut multi = Self {
            hub: Rc::new(LiveHub::new(session.clone())),
            session,
            next_id: 0,
            key: LayoutKey::SINGLE,
            slots: Vec::new(),
            active: 0,
            sync: Links::default(),
            symbol: None,
        };
        multi.add_chart(Timeframe::DEFAULT, cx);
        multi
    }

    pub fn layout_key(&self) -> LayoutKey {
        self.key
    }

    pub fn sync(&self) -> Links {
        self.sync
    }

    /// The chart the header's timeframe buttons and the keys act on.
    pub fn active_chart(&self) -> &Entity<Chart> {
        &self.slots[self.active].chart
    }

    pub fn active_timeframe(&self, cx: &gpui::App) -> Timeframe {
        self.active_chart().read(cx).timeframe()
    }

    fn add_chart(&mut self, timeframe: Timeframe, cx: &mut Context<Self>) {
        self.next_id += 1;
        let (session, hub, id) = (self.session.clone(), self.hub.clone(), self.next_id);
        let chart = cx.new(|_| Chart::new(session, hub, id, timeframe));
        if let Some(symbol) = &self.symbol {
            let (sid, name, digits) = (symbol.id, symbol.name.clone(), symbol.digits);
            chart.update(cx, |chart, cx| chart.set_symbol(sid, name, digits, cx));
        }
        let events = cx.subscribe(&chart, |this, source, event: &ChartEvent, cx| {
            this.on_chart_event(&source, *event, cx);
        });
        self.slots.push(Slot {
            chart,
            _events: events,
        });
    }

    // ---- what the charts show ----

    /// Every chart shows this symbol (the symbol link is always on for now).
    pub fn set_symbol(&mut self, id: i64, name: SharedString, digits: u32, cx: &mut Context<Self>) {
        self.symbol = Some(Symbol {
            id,
            name: name.clone(),
            digits,
        });
        for slot in &self.slots {
            let name = name.clone();
            slot.chart
                .update(cx, |chart, cx| chart.set_symbol(id, name, digits, cx));
        }
    }

    pub fn set_digits(&mut self, id: i64, digits: u32, cx: &mut Context<Self>) {
        if let Some(symbol) = self.symbol.as_mut().filter(|s| s.id == id) {
            symbol.digits = digits;
        }
        for slot in &self.slots {
            slot.chart
                .update(cx, |chart, cx| chart.set_digits(id, digits, cx));
        }
    }

    /// Sets the timeframe of the active chart, or of every chart when the interval is linked.
    pub fn set_timeframe(&mut self, timeframe: Timeframe, cx: &mut Context<Self>) {
        if self.sync.interval {
            for slot in &self.slots {
                slot.chart
                    .update(cx, |chart, cx| chart.set_timeframe(timeframe, cx));
            }
        } else {
            self.active_chart()
                .clone()
                .update(cx, |chart, cx| chart.set_timeframe(timeframe, cx));
        }
        cx.notify();
    }

    pub fn on_spot(&mut self, spot: &SpotEvent, cx: &mut Context<Self>) {
        for slot in &self.slots {
            slot.chart.update(cx, |chart, cx| chart.on_spot(spot, cx));
        }
    }

    pub fn on_ready(&mut self, cx: &mut Context<Self>) {
        for slot in &self.slots {
            slot.chart.update(cx, |chart, cx| chart.on_ready(cx));
        }
    }

    // ---- the layout ----

    /// Arranges the charts as `key` says. Charts that stay keep their state, in order; new ones
    /// are added at the end; those that no longer fit are closed.
    pub fn set_layout(&mut self, key: LayoutKey, cx: &mut Context<Self>) {
        let wanted = layout(key).count();
        self.slots.truncate(wanted);
        while self.slots.len() < wanted {
            let timeframe = if self.sync.interval {
                self.active_timeframe(cx)
            } else {
                NEW_CHART_TIMEFRAMES[self.slots.len() % NEW_CHART_TIMEFRAMES.len()]
            };
            self.add_chart(timeframe, cx);
        }
        self.active = self.active.min(self.slots.len() - 1);
        self.key = key;
        cx.notify();
    }

    pub fn toggle_link(&mut self, link: Link, cx: &mut Context<Self>) {
        match link {
            Link::Interval => self.sync.interval = !self.sync.interval,
            Link::Crosshair => self.sync.crosshair = !self.sync.crosshair,
            Link::Time => self.sync.time = !self.sync.time,
            Link::Range => self.sync.range = !self.sync.range,
        }
        match link {
            // Linking the interval brings every chart to the timeframe of the active one.
            Link::Interval if self.sync.interval => {
                let timeframe = self.active_timeframe(cx);
                self.set_timeframe(timeframe, cx);
            }
            // Unlinking the crosshair clears the ones drawn from other charts.
            Link::Crosshair if !self.sync.crosshair => {
                for slot in &self.slots {
                    slot.chart
                        .update(cx, |chart, cx| chart.show_remote_pointer(None, cx));
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.slots.len() && self.active != index {
            self.active = index;
            cx.notify();
        }
    }

    // ---- the links ----

    fn on_chart_event(
        &mut self,
        source: &Entity<Chart>,
        event: ChartEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.slots.iter().position(|s| &s.chart == source) else {
            return;
        };
        match event {
            ChartEvent::Activated => self.activate(from, cx),
            ChartEvent::Hover(pointer) if self.sync.crosshair => {
                for (i, slot) in self.slots.iter().enumerate() {
                    if i != from {
                        slot.chart
                            .update(cx, |chart, cx| chart.show_remote_pointer(pointer, cx));
                    }
                }
            }
            ChartEvent::ViewChanged(span) if self.sync.range || self.sync.time => {
                for (i, slot) in self.slots.iter().enumerate() {
                    if i == from {
                        continue;
                    }
                    slot.chart.update(cx, |chart, cx| {
                        if self.sync.range {
                            chart.follow_span(span, cx);
                        } else {
                            chart.follow_right_edge(span.right_ms, cx);
                        }
                    });
                }
            }
            ChartEvent::Hover(_) | ChartEvent::ViewChanged(_) => {}
        }
    }
}

impl Render for MultiChart {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let arrangement = layout(self.key);
        let several = arrangement.count() > 1;
        let (cols, rows) = (arrangement.cols as f32, arrangement.rows as f32);

        let cells =
            self.slots
                .iter()
                .zip(&arrangement.cells)
                .enumerate()
                .map(|(index, (slot, cell))| {
                    let active = several && index == self.active;
                    div()
                        .id(("chart-cell", index))
                        .absolute()
                        .left(relative(cell.x as f32 / cols))
                        .top(relative(cell.y as f32 / rows))
                        .w(relative(cell.w as f32 / cols))
                        .h(relative(cell.h as f32 / rows))
                        .p(px(if several { 1.0 } else { 0.0 }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| this.activate(index, cx)),
                        )
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .border_1()
                                .border_color(if active {
                                    theme::accent()
                                } else if several {
                                    theme::border_hairline()
                                } else {
                                    gpui::rgba(0x00000000)
                                })
                                .child(slot.chart.clone()),
                        )
                });

        div()
            .relative()
            .flex_1()
            .w_full()
            .min_h_0()
            .bg(theme::bg())
            .children(cells)
    }
}
