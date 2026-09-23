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

mod drawing_ui;
pub mod icon;
pub mod layouts;

use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Context, Entity, MouseButton, SharedString, Subscription, Window, div, px, relative};
use wyck::openapi::market::SpotEvent;
use wyck::openapi::session::Session;

use self::layouts::{LayoutKey, layout};
use super::chart::drawing::Drawings;
use super::chart::drawing::model::{Dash, Group, Tool};
use super::chart::{Chart, ChartEvent, ChartKind, LiveHub, Timeframe, Zone};
use super::text_input::TextInput;
use super::theme;
use super::workspace::{NEW_CHART_TIMEFRAMES, Workspace};

/// What is linked between the charts (saved with the layout).
pub use super::workspace::LinksPref as Links;

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
    workspace: Entity<Workspace>,
    hub: Rc<LiveHub>,
    next_id: u64,
    key: LayoutKey,
    slots: Vec<Slot>,
    active: usize,
    sync: Links,
    zone: Zone,
    symbol: Option<Symbol>,
    /// The drawings, shared by every chart.
    drawings: Entity<Drawings>,
    _drawings_observe: Subscription,
    /// The words of the selected text drawing, edited here.
    text_input: Entity<TextInput>,
    _text_observe: Subscription,
    /// The family of drawing tools that is open, if any.
    flyout: Option<Group>,
    /// Taken when a chart is clicked, so the keys (arrows, Delete, Ctrl+Z) reach the charts and
    /// not a text field that had the keyboard before.
    focus: gpui::FocusHandle,
    /// The tool each family shows on its button.
    last_tool: HashMap<Group, Tool>,
}

impl MultiChart {
    /// Starts with the layout, timeframes and links the workspace remembers.
    pub fn new(
        session: Session,
        workspace: Entity<Workspace>,
        drawings: Entity<Drawings>,
        cx: &mut Context<Self>,
    ) -> Self {
        let prefs = workspace.read(cx).preferences().clone();
        let key = prefs.layout_key();
        drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_magnet(prefs.magnet));
        });
        let text_input = cx.new(|cx| TextInput::new(cx, "Text"));
        let _text_observe = cx.observe(&text_input, |this, _input, cx| this.on_text_edited(cx));
        let _drawings_observe = cx.observe(&drawings, |this, _drawings, cx| {
            this.on_drawings_changed(cx);
        });
        let mut multi = Self {
            drawings,
            _drawings_observe,
            text_input,
            _text_observe,
            flyout: None,
            focus: cx.focus_handle(),
            last_tool: HashMap::new(),
            hub: Rc::new(LiveHub::new(session.clone())),
            session,
            workspace,
            next_id: 0,
            key,
            slots: Vec::new(),
            active: prefs.active_chart,
            sync: prefs.links,
            zone: prefs.zone,
            symbol: None,
        };
        for index in 0..layout(key).count() {
            let (timeframe, kind) = prefs.chart(index);
            multi.add_chart(timeframe, Some(kind), cx);
        }
        multi.active = multi.active.min(multi.slots.len() - 1);
        multi
    }

    /// Writes the layout, the timeframes and types, the links and the zone to the workspace,
    /// which saves them.
    fn persist(&self, cx: &mut Context<Self>) {
        let charts: Vec<(Timeframe, ChartKind)> = self
            .slots
            .iter()
            .map(|slot| {
                let chart = slot.chart.read(cx);
                (chart.timeframe(), chart.kind())
            })
            .collect();
        let (key, active, links, zone) = (self.key, self.active, self.sync, self.zone);
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| {
                prefs.set_arrangement(key, &charts, active);
                prefs.links = links;
                prefs.zone = zone;
            });
        });
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

    fn add_chart(&mut self, timeframe: Timeframe, kind: Option<ChartKind>, cx: &mut Context<Self>) {
        self.next_id += 1;
        let (session, hub, id) = (self.session.clone(), self.hub.clone(), self.next_id);
        let chart = cx.new(|_| Chart::new(session, hub, id, timeframe));
        let (zone, drawings) = (self.zone, self.drawings.clone());
        chart.update(cx, |chart, cx| {
            chart.attach_drawings(drawings, cx);
            chart.set_zone(zone, cx);
            if let Some(kind) = kind {
                chart.set_kind(kind, cx);
            }
        });
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
        self.persist(cx);
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
                Timeframe::from_code(
                    NEW_CHART_TIMEFRAMES[self.slots.len() % NEW_CHART_TIMEFRAMES.len()],
                )
                .unwrap_or(Timeframe::DEFAULT)
            };
            self.add_chart(timeframe, None, cx);
        }
        self.active = self.active.min(self.slots.len() - 1);
        self.key = key;
        self.persist(cx);
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
        self.persist(cx);
        cx.notify();
    }

    /// Switches every chart between UTC and the zone of the computer.
    fn toggle_zone(&mut self, cx: &mut Context<Self>) {
        self.zone = self.zone.toggled();
        for slot in &self.slots {
            let zone = self.zone;
            slot.chart.update(cx, |chart, cx| chart.set_zone(zone, cx));
        }
        self.persist(cx);
        cx.notify();
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.slots.len() && self.active != index {
            self.active = index;
            self.persist(cx);
            cx.notify();
        }
    }

    // ---- drawing ----

    fn symbol_name(&self) -> Option<String> {
        self.symbol.as_ref().map(|s| s.name.to_string())
    }

    pub(crate) fn pick_tool(&mut self, tool: Option<Tool>, cx: &mut Context<Self>) {
        self.flyout = None;
        if let Some(tool) = tool {
            self.last_tool.insert(tool.group(), tool);
        }
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_tool(tool))
        });
        cx.notify();
    }

    pub(crate) fn toggle_magnet(&mut self, cx: &mut Context<Self>) {
        let magnet = !self.drawings.read(cx).book().magnet();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_magnet(magnet))
        });
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.magnet = magnet);
        });
    }

    /// Escape: gives up what the drawing tools have in progress. Returns whether there was any.
    pub fn cancel_drawing(&mut self, cx: &mut Context<Self>) -> bool {
        if self.flyout.take().is_some() {
            cx.notify();
            return true;
        }
        self.drawings
            .update(cx, |drawings, cx| drawings.edit(cx, |book| book.cancel()))
    }

    fn edit_book(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut super::chart::drawing::book::Book, &str) -> bool,
    ) {
        let Some(symbol) = self.symbol_name() else {
            return;
        };
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| change(book, &symbol))
        });
    }

    pub fn delete_drawing(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.delete_selected(symbol));
    }

    pub fn undo_drawing(&mut self, cx: &mut Context<Self>) {
        self.drawings
            .update(cx, |drawings, cx| drawings.edit(cx, |book| book.undo()));
    }

    pub fn redo_drawing(&mut self, cx: &mut Context<Self>) {
        self.drawings
            .update(cx, |drawings, cx| drawings.edit(cx, |book| book.redo()));
    }

    pub fn duplicate_drawing(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.duplicate_selected(cx));
    }

    pub(crate) fn clear_drawings(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.clear(symbol));
    }

    pub(crate) fn set_drawing_color(&mut self, color: u32, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.set_color(symbol, color));
    }

    pub(crate) fn set_drawing_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.set_width(symbol, width));
    }

    pub(crate) fn set_drawing_dash(&mut self, dash: Dash, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.set_dash(symbol, dash));
    }

    pub(crate) fn toggle_drawing_fill(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.toggle_fill(symbol));
    }

    pub(crate) fn toggle_drawing_lock(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.toggle_lock(symbol));
    }

    /// The words field changed: the selected text drawing says what it says.
    fn on_text_edited(&mut self, cx: &mut Context<Self>) {
        let text = self.text_input.read(cx).text().to_owned();
        self.edit_book(cx, |book, symbol| book.set_text(symbol, &text));
    }

    /// The drawings changed: keep the words field in step with the selected text drawing.
    fn on_drawings_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(symbol) = self.symbol_name() {
            let book = self.drawings.read(cx).book();
            let words = book
                .selected()
                .and_then(|id| book.get(&symbol, id))
                .filter(|drawing| drawing.tool.has_text())
                .map(|drawing| drawing.text.clone());
            if let Some(words) = words
                && self.text_input.read(cx).text() != words
            {
                self.text_input
                    .update(cx, |input, cx| input.set_text(words, cx));
            }
        }
        cx.notify();
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
            ChartEvent::ZoneClicked => self.toggle_zone(cx),
            ChartEvent::SettingsChanged => self.persist(cx),
            ChartEvent::Hover(_) | ChartEvent::ViewChanged(_) => {}
        }
    }
}

impl Render for MultiChart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A text drawing that was just made takes the keyboard, so its words can be typed.
        if self.drawings.read(cx).book().wants_text_focus() {
            self.drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.take_text_focus());
            });
            window.focus(&gpui::Focusable::focus_handle(&self.text_input, cx), cx);
        }
        let arrangement = layout(self.key);
        let several = arrangement.count() > 1;
        let (cols, rows) = (arrangement.cols as f32, arrangement.rows as f32);

        let cells = self
            .slots
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
                        cx.listener(move |this, _event, window, cx| {
                            window.focus(&this.focus, cx);
                            this.activate(index, cx);
                        }),
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
            })
            .collect::<Vec<_>>();

        let rail = self.render_rail(cx);
        let flyout = self.render_flyout(cx);
        let style_bar = self.render_style_bar(cx);

        div()
            .relative()
            .flex()
            .flex_row()
            .flex_1()
            .w_full()
            .min_h_0()
            .bg(theme::bg())
            .track_focus(&self.focus)
            .child(rail)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .children(cells)
                    .children(style_bar),
            )
            .children(flyout)
    }
}
