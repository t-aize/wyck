//! Several charts on one screen, arranged by a layout, sized by dragging the lines between them,
//! and linked the way the user chooses.
//!
//! Each chart has its own timeframe, type, indicators and time zone, and (when the symbol link is
//! off) its own symbol. The links are the ones a trading platform offers; their rules live in
//! [`links`], as plain data, and this view only applies what they say.
//!
//! The charts do not know about each other. Each one reports what the user did as a
//! [`ChartEvent`], and this view applies the links, which keeps a chart free of loops: a chart
//! that is made to follow another never reports it. What the rest of the app acts on (a symbol to
//! pick, an order asked from the chart, a line dragged) goes up as a [`MultiChartEvent`].

mod drawing_ui;
mod favorites;
pub mod icon;
pub use crate::app::workspace::layouts;
pub mod links;
pub mod split;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::broker::session::Session;
use crate::domain::market::{LiveBarTracker, SpotEvent};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Entity, EventEmitter, MouseButton, MouseMoveEvent, Pixels, SharedString,
    Subscription, Window, canvas, div, px, relative,
};

use self::layouts::{LayoutKey, layout};
use self::links::{Follow, Link, Links};
use self::split::{Divider, Node};
use crate::app::market_data::live::{LiveHub, LiveUpdate};
use crate::app::workspace::{
    ChartState, NEW_CHART_TIMEFRAMES, Preferences, UsageLimits, Workspace,
};
use crate::domain::chart::timeframe::Timeframe;
use crate::domain::drawings::model::{Dash, Group, MIN_LINE_OPACITY, Tool};
use crate::domain::indicators::StudyConfig;
use crate::ui::features::chart::drawing::Drawings;
use crate::ui::features::chart::{
    Chart, ChartAction, ChartEvent, ChartLine, ChartSettings, EditorRequest, LineId,
};
use crate::ui::kit::{text_input::TextInput, theme};

/// How many recent tools and colors are kept.
const MAX_RECENT_TOOLS: usize = 8;
const MAX_RECENT_COLORS: usize = 6;

/// The symbol of a chart: its id, name and number of decimals.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolRef {
    pub id: i64,
    pub name: SharedString,
    pub digits: u32,
    pub pip_position: Option<i64>,
}

/// What the rest of the app acts on.
#[derive(Debug, Clone)]
pub enum MultiChartEvent {
    /// The user asked to change the symbol of chart `n`.
    PickSymbol(usize),
    /// The active chart changed (so did the symbol and timeframe the header shows).
    ActiveChanged,
    /// Something asked from a chart, for its symbol.
    Action(SymbolRef, ChartAction),
    /// A line was dragged to a new price.
    LineMoved(LineId, f64),
    /// A line's close button was clicked.
    LineClosed(LineId),
    /// A picture of a chart, ready to save: the PNG and a file name.
    Picture(Vec<u8>, String),
    /// A chart could not be turned into a picture.
    PictureFailed(String),
    /// A chart asked for the editor of the indicator scripts.
    IndicatorEditor(EditorRequest),
}

struct Slot {
    chart: Entity<Chart>,
    /// The symbol the chart was saved with, until the symbol list can resolve it.
    wanted_symbol: Option<String>,
    _events: Subscription,
}

#[derive(Clone)]
enum ChartClipboard {
    Studies(Vec<StudyConfig>),
    Settings(Box<ChartSettings>),
}

/// A line between charts being dragged.
#[derive(Debug, Clone, Copy)]
struct SplitDrag {
    divider: Divider,
    last: (f32, f32),
}

pub struct MultiChart {
    session: Session,
    workspace: Entity<Workspace>,
    hub: Rc<LiveHub>,
    /// Corrects the live bars of the price events before the charts see them.
    tracker: LiveBarTracker,
    /// The trading hours of every symbol whose details came, for the charts showing them.
    hours: HashMap<i64, std::sync::Arc<crate::domain::market::TradingHours>>,
    next_id: u64,
    key: LayoutKey,
    tree: Node,
    slots: Vec<Slot>,
    active: usize,
    sync: Links,
    clipboard: Option<ChartClipboard>,
    syncing_studies: bool,
    /// The drawings, shared by every chart.
    drawings: Entity<Drawings>,
    _drawings_observe: Subscription,
    /// The words of the selected text drawing, edited here.
    text_input: Entity<TextInput>,
    _text_observe: Subscription,
    /// The family of drawing tools that is open, if any.
    flyout: Option<Group>,
    /// Whether the search over every tool is open, in place of a family.
    tool_search: bool,
    /// The field the tools are searched with.
    search_input: Entity<TextInput>,
    _search_observe: Subscription,
    /// The tools picked last, most recent first.
    recent_tools: Vec<Tool>,
    /// The colors set on drawings last, most recent first.
    recent_colors: Vec<u32>,
    /// Where the button of each family was last drawn, so its list opens beside it.
    rail_slots: Rc<RefCell<HashMap<Group, Bounds<Pixels>>>>,
    /// Where this whole view was last drawn.
    root_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The drawing whose color panel is open in the style bar.
    color_open: Option<u64>,
    /// Taken when a chart is clicked, so the keys (arrows, Delete, Ctrl+Z) reach the charts and
    /// not a text field that had the keyboard before.
    focus: gpui::FocusHandle,
    /// The tool each family shows on its button.
    last_tool: HashMap<Group, Tool>,
    /// The area of the charts as last drawn, for turning drags into shares of it.
    area: Rc<Cell<Option<Bounds<Pixels>>>>,
    split_drag: Option<SplitDrag>,
    /// The divider under the pointer.
    hover_divider: Option<(usize, usize)>,
    /// The lines (orders, positions, alerts) of each symbol, handed to its charts.
    lines: HashMap<i64, Vec<ChartLine>>,
}

impl EventEmitter<MultiChartEvent> for MultiChart {}

impl MultiChart {
    /// Starts with the layout, charts and links the workspace remembers.
    pub fn new(
        session: Session,
        hub: Rc<LiveHub>,
        workspace: Entity<Workspace>,
        drawings: Entity<Drawings>,
        cx: &mut Context<Self>,
    ) -> Self {
        let prefs = workspace.read(cx).preferences().clone();
        let key = prefs.layout_key();
        drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| {
                book.set_magnet(prefs.magnet);
                book.set_keep_tool(prefs.keep_drawing);
                book.set_drawing_limit(prefs.limits.drawings_per_symbol);
            });
        });
        let text_input = cx.new(|cx| TextInput::new(cx, "Text"));
        let _text_observe = cx.observe(&text_input, |this, _input, cx| this.on_text_edited(cx));
        let search_input = cx.new(|cx| TextInput::new(cx, "Search the tools"));
        let _search_observe = cx.observe(&search_input, |_, _input, cx| cx.notify());
        let _drawings_observe = cx.observe(&drawings, |this, _drawings, cx| {
            this.on_drawings_changed(cx);
        });
        let mut multi = Self {
            drawings,
            _drawings_observe,
            text_input,
            _text_observe,
            flyout: None,
            tool_search: false,
            search_input,
            _search_observe,
            recent_tools: Vec::new(),
            recent_colors: Vec::new(),
            rail_slots: Rc::default(),
            root_bounds: Rc::new(Cell::new(None)),
            color_open: None,
            focus: cx.focus_handle(),
            last_tool: HashMap::new(),
            hub,
            tracker: LiveBarTracker::new(),
            hours: HashMap::new(),
            session,
            workspace,
            next_id: 0,
            key,
            tree: tree_for(key, &prefs),
            slots: Vec::new(),
            active: prefs.active_chart,
            sync: prefs.links,
            clipboard: None,
            syncing_studies: false,
            area: Rc::new(Cell::new(None)),
            split_drag: None,
            hover_divider: None,
            lines: HashMap::new(),
        };
        for index in 0..layout(key).count() {
            multi.add_chart(prefs.chart(index), cx);
        }
        multi.active = multi.active.min(multi.slots.len() - 1);
        multi.tell_layout_size(cx);
        multi
    }

    /// Tells every chart how many charts the layout holds.
    fn tell_layout_size(&self, cx: &mut Context<Self>) {
        let count = self.slots.len();
        for slot in &self.slots {
            slot.chart
                .update(cx, |chart, cx| chart.set_layout_charts(count, cx));
        }
    }

    /// Writes the layout, what each chart keeps, the links and the line positions to the
    /// workspace, which saves them.
    fn persist(&self, cx: &mut Context<Self>) {
        let charts: Vec<ChartState> = self
            .slots
            .iter()
            .map(|slot| {
                let chart = slot.chart.read(cx);
                ChartState {
                    timeframe: chart.timeframe(),
                    settings: chart.settings().clone(),
                    symbol: chart
                        .symbol()
                        .map(|s| s.name.to_string())
                        .or_else(|| slot.wanted_symbol.clone()),
                }
            })
            .collect();
        let (key, active, links) = (self.key, self.active, self.sync);
        let weights = self.tree.weights();
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| {
                prefs.set_arrangement(key, &charts, active);
                prefs.links = links;
                prefs.splits.insert(Preferences::split_key(key), weights);
            });
        });
    }

    pub fn layout_key(&self) -> LayoutKey {
        self.key
    }

    pub fn sync(&self) -> Links {
        self.sync
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    /// The chart the header's timeframe buttons and the keys act on.
    /// How many charts the layout holds.
    pub fn chart_count(&self) -> usize {
        self.slots.len()
    }

    pub fn active_chart(&self) -> &Entity<Chart> {
        &self.slots[self.active].chart
    }

    pub fn has_chart_focus(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    /// Ctrl+C: copies the selected drawings, or else the indicators of the chart.
    pub fn copy_active_indicators(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        if chart.update(cx, |chart, cx| chart.copy_selected_drawings(cx)) {
            // What was copied last is what Ctrl+V pastes.
            self.clipboard = None;
            return;
        }
        self.copy_chart(self.active, false, cx);
    }

    pub fn copy_active_settings(&mut self, cx: &mut Context<Self>) {
        self.copy_chart(self.active, true, cx);
    }

    /// Ctrl+V: the drawings copied last, or else the indicators or settings copied.
    pub fn paste_active(&mut self, cx: &mut Context<Self>) {
        if self.clipboard.is_none() {
            let chart = self.active_chart().clone();
            if chart.update(cx, |chart, cx| chart.paste_drawings(cx)) {
                return;
            }
        }
        self.paste_chart(self.active, cx);
    }

    /// Ctrl+A on the charts: selects every drawing of the symbol.
    pub fn select_all_drawings(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.select_all_drawings(cx));
    }

    /// An arrow key: moves the selected drawings. Returns whether there were any to move.
    pub fn nudge_drawings(&mut self, bars: f64, steps: f64, cx: &mut Context<Self>) -> bool {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.nudge_selected(bars, steps, cx))
    }

    /// The key of a tool: picks it, or goes back to the pointer when it is in hand already.
    pub fn pick_tool_key(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let current = self.drawings.read(cx).book().tool();
        self.pick_tool(
            if current == Some(tool) {
                None
            } else {
                Some(tool)
            },
            cx,
        );
    }

    fn copy_chart(&mut self, index: usize, settings: bool, cx: &mut Context<Self>) {
        let Some(slot) = self.slots.get(index) else {
            return;
        };
        let source = slot.chart.read(cx).settings().clone();
        self.clipboard = Some(if settings {
            ChartClipboard::Settings(Box::new(source))
        } else {
            ChartClipboard::Studies(source.studies)
        });
    }

    fn paste_chart(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(payload) = self.clipboard.clone() else {
            return;
        };
        let Some(slot) = self.slots.get(index) else {
            return;
        };
        let chart = slot.chart.clone();
        match payload {
            ChartClipboard::Studies(studies) => {
                let limit = chart.read(cx).max_studies();
                if studies.len() > limit {
                    crate::ui::kit::toast::show(
                        cx,
                        crate::ui::kit::toast::Kind::Warning,
                        "Indicator limit reached",
                        format!(
                            "The copied chart has {} indicators; this chart allows {limit}.",
                            studies.len()
                        ),
                    );
                    return;
                }
                chart.update(cx, |chart, cx| {
                    chart.edit_settings(cx, |settings| settings.studies = studies)
                });
            }
            ChartClipboard::Settings(settings) => {
                chart.update(cx, |chart, cx| {
                    chart.edit_settings(cx, |target| {
                        let studies = std::mem::take(&mut target.studies);
                        *target = *settings;
                        target.studies = studies;
                    })
                });
            }
        }
        self.persist(cx);
    }

    fn share_studies(&mut self, from: usize, cx: &mut Context<Self>) {
        if self.syncing_studies {
            return;
        }
        let Some(slot) = self.slots.get(from) else {
            return;
        };
        let studies = slot.chart.read(cx).settings().studies.clone();
        self.syncing_studies = true;
        for (index, slot) in self.slots.iter().enumerate() {
            if index != from {
                let studies = studies.clone();
                slot.chart.update(cx, |chart, cx| {
                    chart.edit_settings(cx, |s| s.studies = studies)
                });
            }
        }
        self.syncing_studies = false;
    }

    pub fn active_timeframe(&self, cx: &gpui::App) -> Timeframe {
        self.active_chart().read(cx).timeframe()
    }

    /// The symbol of the active chart.
    pub fn active_symbol(&self, cx: &gpui::App) -> Option<SymbolRef> {
        self.active_chart().read(cx).symbol().map(|s| SymbolRef {
            id: s.id,
            name: s.name.clone(),
            digits: s.digits,
            pip_position: s.pip_position,
        })
    }

    /// The saved symbol names of the charts that have none yet, with the chart index.
    pub fn wanted_symbols(&self, cx: &gpui::App) -> Vec<(usize, Option<String>)> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.chart.read(cx).symbol().is_none())
            .map(|(i, slot)| (i, slot.wanted_symbol.clone()))
            .collect()
    }

    /// Every symbol a chart shows.
    pub fn symbol_ids(&self, cx: &gpui::App) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .slots
            .iter()
            .filter_map(|slot| slot.chart.read(cx).symbol().map(|s| s.id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn add_chart(&mut self, state: ChartState, cx: &mut Context<Self>) {
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
        for index in links::symbol_targets(&self.sync, target, self.slots.len()) {
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
    /// A symbol's trading hours arrived.
    pub fn set_hours(
        &mut self,
        id: i64,
        hours: crate::domain::market::TradingHours,
        cx: &mut Context<Self>,
    ) {
        self.hours.insert(id, std::sync::Arc::new(hours));
        self.give_hours(cx);
    }

    /// Hands each chart the hours of the symbol it shows.
    fn give_hours(&self, cx: &mut Context<Self>) {
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
        for index in links::timeframe_targets(&self.sync, self.active, self.slots.len()) {
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
            let settings = ChartSettings {
                zone: self.active_chart().read(cx).settings().zone,
                studies: if self.sync.studies {
                    self.active_chart().read(cx).settings().studies.clone()
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            // A new chart shows the active chart's symbol even when symbols are not linked.
            let symbol = self.active_chart().read(cx).symbol().cloned();
            self.add_chart(
                ChartState {
                    timeframe,
                    settings,
                    symbol: None,
                },
                cx,
            );
            if let Some(symbol) = symbol
                && let Some(slot) = self.slots.last()
                && slot.chart.read(cx).symbol().is_none()
            {
                slot.chart.update(cx, |chart, cx| {
                    chart.set_symbol(
                        symbol.id,
                        symbol.name,
                        symbol.digits,
                        symbol.pip_position,
                        cx,
                    );
                });
            }
        }
        self.active = self.active.min(self.slots.len() - 1);
        self.key = key;
        self.tell_layout_size(cx);
        let prefs = self.workspace.read(cx).preferences().clone();
        self.tree = tree_for(key, &prefs);
        self.persist(cx);
        cx.emit(MultiChartEvent::ActiveChanged);
        cx.notify();
    }

    /// Puts every line between the charts back where the grid has it.
    pub fn reset_splits(&mut self, cx: &mut Context<Self>) {
        self.tree.reset(layout(self.key));
        self.persist(cx);
        cx.notify();
    }

    pub fn toggle_link(&mut self, link: Link, cx: &mut Context<Self>) {
        link.toggle(&mut self.sync);
        match link {
            // Linking the interval brings every chart to the timeframe of the active one.
            Link::Interval if self.sync.interval => {
                let timeframe = self.active_timeframe(cx);
                self.set_timeframe(timeframe, cx);
            }
            // Linking the symbol brings every chart to the symbol of the active one.
            Link::Symbol if self.sync.symbol => {
                if let Some(symbol) = self.active_symbol(cx) {
                    let active = self.active;
                    self.set_symbol(active, symbol, cx);
                }
            }
            // Unlinking the crosshair clears the ones drawn from other charts.
            Link::Crosshair if !self.sync.crosshair => {
                for slot in &self.slots {
                    slot.chart
                        .update(cx, |chart, cx| chart.show_remote_pointer(None, cx));
                }
            }
            Link::Studies if self.sync.studies => self.share_studies(self.active, cx),
            _ => {}
        }
        self.persist(cx);
        cx.notify();
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.slots.len() && self.active != index {
            self.active = index;
            self.persist(cx);
            cx.emit(MultiChartEvent::ActiveChanged);
            cx.notify();
        }
    }

    pub fn set_usage_limits(&mut self, limits: UsageLimits, cx: &mut Context<Self>) {
        for slot in &self.slots {
            slot.chart.update(cx, |chart, cx| {
                chart.set_max_studies(limits.studies_per_chart, cx);
            });
        }
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| {
                book.set_drawing_limit(limits.drawings_per_symbol)
            });
        });
        cx.notify();
    }

    fn symbol_name(&self, cx: &gpui::App) -> Option<String> {
        self.active_chart()
            .read(cx)
            .symbol()
            .map(|s| s.name.to_string())
    }

    pub(crate) fn pick_tool(&mut self, tool: Option<Tool>, cx: &mut Context<Self>) {
        self.flyout = None;
        self.tool_search = false;
        if let Some(tool) = tool {
            self.last_tool.insert(tool.group(), tool);
            self.recent_tools.retain(|other| *other != tool);
            self.recent_tools.insert(0, tool);
            self.recent_tools.truncate(MAX_RECENT_TOOLS);
        }
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_tool(tool))
        });
        cx.notify();
    }

    /// Opens the search over every tool, with the keyboard in its field.
    pub fn open_tool_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flyout = None;
        self.tool_search = true;
        self.search_input
            .update(cx, |input, cx| input.set_text("", cx));
        window.focus(&gpui::Focusable::focus_handle(&self.search_input, cx), cx);
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

    pub(crate) fn toggle_keep_drawing(&mut self, cx: &mut Context<Self>) {
        let keep = !self.drawings.read(cx).book().keep_tool();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_keep_tool(keep))
        });
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.keep_drawing = keep);
        });
    }

    /// Writes what waits to be saved now: the drawings and the workspace, for a backup.
    pub fn flush_documents(&self, cx: &gpui::App) {
        self.drawings.read(cx).flush();
        self.workspace.read(cx).flush();
    }

    /// Whether a drawing is being made on any chart, for the keys that only mean something then.
    pub fn drawing_in_progress(&self, cx: &gpui::App) -> bool {
        self.drawings.read(cx).book().is_creating()
    }

    /// Enter: ends an arrow path with the points it has. Returns whether it did.
    pub fn finish_drawing(&mut self, cx: &mut Context<Self>) -> bool {
        self.drawings
            .update(cx, |drawings, cx| drawings.edit(cx, |book| book.finish()))
    }

    /// Escape: gives up what the drawing tools have in progress. Returns whether there was any.
    pub fn cancel_drawing(&mut self, cx: &mut Context<Self>) -> bool {
        if self.flyout.take().is_some() || std::mem::take(&mut self.tool_search) {
            cx.notify();
            return true;
        }
        self.drawings
            .update(cx, |drawings, cx| drawings.edit(cx, |book| book.cancel()))
    }

    /// Opens the order ticket filled from a position drawing of the active chart.
    pub(crate) fn trade_drawing(&mut self, id: u64, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.trade_drawing(id, cx));
    }

    pub(crate) fn drawing_command(
        &mut self,
        id: u64,
        command: crate::ui::features::chart::DrawingCommand,
        cx: &mut Context<Self>,
    ) {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.drawing_command(id, command, cx));
    }

    pub(crate) fn edit_book(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut crate::domain::drawings::book::Book, &str) -> bool,
    ) {
        let Some(symbol) = self.symbol_name(cx) else {
            return;
        };
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| change(book, &symbol))
        });
    }

    /// Delete and Backspace: take back the last point of a drawing being made, or else delete the
    /// selected drawing.
    pub fn delete_drawing(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| {
            book.remove_last_point() || book.delete_selected(symbol)
        });
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
        self.recent_colors.retain(|other| *other != color);
        self.recent_colors.insert(0, color);
        self.recent_colors.truncate(MAX_RECENT_COLORS);
        self.edit_book(cx, |book, symbol| book.set_color(symbol, color));
    }

    /// The opacity bar of the color panel: the lines of the selected drawings, from 0 to 1.
    pub(crate) fn set_drawing_opacity(&mut self, opacity: f32, cx: &mut Context<Self>) {
        let opacity = opacity.clamp(MIN_LINE_OPACITY, 1.0);
        self.edit_book(cx, |book, symbol| {
            book.edit_style(symbol, |s| s.opacity = opacity)
        });
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
        if let Some(symbol) = self.symbol_name(cx) {
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

    /// Takes a picture of the active chart.
    pub fn picture(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        match chart.read(cx).picture(cx) {
            Ok((png, name)) => cx.emit(MultiChartEvent::Picture(png, name)),
            Err(error) => cx.emit(MultiChartEvent::PictureFailed(error)),
        }
    }

    /// Takes a picture of every chart of the layout, in one image laid out as on screen.
    pub fn picture_all(&mut self, cx: &mut Context<Self>) {
        if self.slots.len() < 2 {
            self.picture(cx);
            return;
        }
        use crate::ui::features::chart::raster::PICTURE_SCALE;
        let (rects, _) = self.tree.place(self.slots.len());
        let mut tiles = Vec::new();
        let mut background = None;
        let (mut width, mut height) = self.area.get().map_or((0.0, 0.0), |a| {
            (f32::from(a.size.width), f32::from(a.size.height))
        });
        for (slot, rect) in self.slots.iter().zip(&rects) {
            let chart = slot.chart.read(cx);
            let (pixmap, bg) = match chart.picture_pixels(cx) {
                Ok(done) => done,
                Err(error) => {
                    cx.emit(MultiChartEvent::PictureFailed(error));
                    return;
                }
            };
            background.get_or_insert(bg);
            let (tile_w, tile_h) = (
                pixmap.width() as f32 / PICTURE_SCALE,
                pixmap.height() as f32 / PICTURE_SCALE,
            );
            // The area was not measured yet (nothing was drawn): the charts say how big it is.
            width = width.max(tile_w / rect.w.max(f32::EPSILON));
            height = height.max(tile_h / rect.h.max(f32::EPSILON));
            tiles.push((rect.x, rect.y, pixmap));
        }
        let tiles: Vec<_> = tiles
            .into_iter()
            .map(|(x, y, pixmap)| (x * width, y * height, pixmap))
            .collect();
        let Some(background) = background else { return };
        match crate::ui::features::chart::raster::compose_png(
            width,
            height,
            PICTURE_SCALE,
            background,
            &tiles,
        ) {
            Ok(png) => {
                let stamp = crate::app::market_data::now_ms();
                let name = format!("wyck-layout-{}-charts-{stamp}.png", self.slots.len());
                cx.emit(MultiChartEvent::Picture(png, name));
            }
            Err(error) => cx.emit(MultiChartEvent::PictureFailed(error)),
        }
    }

    fn on_chart_event(
        &mut self,
        source: &Entity<Chart>,
        event: &ChartEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.slots.iter().position(|s| &s.chart == source) else {
            return;
        };
        match event {
            ChartEvent::Activated => self.activate(from, cx),
            ChartEvent::Hover(_) | ChartEvent::ViewChanged(_) | ChartEvent::TimePicked(_) => {
                let symbols: Vec<Option<i64>> = self
                    .slots
                    .iter()
                    .map(|slot| slot.chart.read(cx).symbol().map(|s| s.id))
                    .collect();
                for (index, follow) in links::route(&self.sync, from, &symbols, event) {
                    self.slots[index]
                        .chart
                        .update(cx, |chart, cx| match follow {
                            Follow::Pointer(pointer) => chart.show_remote_pointer(pointer, cx),
                            Follow::Span(span) => chart.follow_span(span, cx),
                            Follow::RightEdge(right) => chart.follow_right_edge(right, cx),
                            Follow::PickedTime(time) => chart.follow_picked_time(time, cx),
                        });
                }
            }
            ChartEvent::SettingsChanged => {
                if self.sync.studies && !self.syncing_studies {
                    self.share_studies(from, cx);
                }
                self.persist(cx);
                if from == self.active {
                    cx.emit(MultiChartEvent::ActiveChanged);
                }
            }
            ChartEvent::CopyIndicators => self.copy_chart(from, false, cx),
            ChartEvent::CopySettings => self.copy_chart(from, true, cx),
            ChartEvent::Paste => self.paste_chart(from, cx),
            ChartEvent::PickSymbol => {
                self.activate(from, cx);
                cx.emit(MultiChartEvent::PickSymbol(from));
            }
            ChartEvent::Action(action) => {
                if let Some(symbol) = source.read(cx).symbol() {
                    let symbol = SymbolRef {
                        id: symbol.id,
                        name: symbol.name.clone(),
                        digits: symbol.digits,
                        pip_position: symbol.pip_position,
                    };
                    cx.emit(MultiChartEvent::Action(symbol, action.clone()));
                }
            }
            ChartEvent::LineMoved(id, price) => cx.emit(MultiChartEvent::LineMoved(*id, *price)),
            ChartEvent::IndicatorEditor(request) => {
                cx.emit(MultiChartEvent::IndicatorEditor(request.clone()));
            }
            ChartEvent::LineClosed(id) => cx.emit(MultiChartEvent::LineClosed(*id)),
            ChartEvent::Screenshot => {
                self.activate(from, cx);
                self.picture(cx);
            }
            ChartEvent::ScreenshotAll => {
                self.activate(from, cx);
                self.picture_all(cx);
            }
        }
    }

    fn on_area_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(mut drag) = self.split_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.split_drag = None;
            self.persist(cx);
            cx.notify();
            return;
        }
        let Some(area) = self.area.get() else {
            return;
        };
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let (w, h) = (
            f32::from(area.size.width).max(1.0),
            f32::from(area.size.height).max(1.0),
        );
        let delta = if drag.divider.vertical {
            (y - drag.last.1) / h
        } else {
            (x - drag.last.0) / w
        };
        drag.last = (x, y);
        self.split_drag = Some(drag);
        self.tree.drag(
            drag.divider.split,
            drag.divider.index,
            delta,
            drag.divider.span,
        );
        cx.notify();
    }

    fn end_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            self.persist(cx);
            cx.notify();
        }
    }
}

/// The split tree of a layout, with the weights saved for it.
fn tree_for(key: LayoutKey, prefs: &Preferences) -> Node {
    let mut tree = split::tree(layout(key));
    if let Some(saved) = prefs.splits.get(&Preferences::split_key(key)) {
        tree.apply_weights(saved);
    }
    tree
}

/// How wide the hit zone of a line between charts is.
const DIVIDER_HIT: f32 = 7.0;

impl Render for MultiChart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A text drawing that was just made takes the keyboard, so its words can be typed.
        if self.drawings.read(cx).book().wants_text_focus() {
            self.drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.take_text_focus());
            });
            window.focus(&gpui::Focusable::focus_handle(&self.text_input, cx), cx);
        }
        // A chart that changed symbol gets that symbol's hours (a no-op when it has them).
        self.give_hours(cx);
        let several = self.slots.len() > 1;
        let floor = if self.favorites_shown(cx) {
            favorites::PILL_TOP
        } else {
            0.0
        };
        for (index, slot) in self.slots.iter().enumerate() {
            let selected = !several || index == self.active;
            slot.chart.update(cx, |chart, cx| {
                chart.set_selected(selected, cx);
                chart.set_hint_floor(floor, cx);
            });
        }
        let (rects, dividers) = self.tree.place(self.slots.len());
        // The bar of the selected drawing floats over the chart it is being edited on.
        let mut style_bar = self.render_style_bar(window, cx);
        let gap = if several { 1.0 } else { 0.0 };

        let cells = self
            .slots
            .iter()
            .zip(&rects)
            .enumerate()
            .map(|(index, (slot, r))| {
                let active = several && index == self.active;
                div()
                    .id(("chart-cell", index))
                    .absolute()
                    .left(relative(r.x))
                    .top(relative(r.y))
                    .w(relative(r.w))
                    .h(relative(r.h))
                    .p(px(gap))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            window.focus(&this.focus, cx);
                            this.activate(index, cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _event, _window, cx| this.activate(index, cx)),
                    )
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .rounded(px(if several { 3.0 } else { 0.0 }))
                            .overflow_hidden()
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
                    .children(if index == self.active {
                        style_bar.take()
                    } else {
                        None
                    })
            })
            .collect::<Vec<_>>();

        let dragging = self.split_drag.map(|d| (d.divider.split, d.divider.index));
        let divider_elements = dividers
            .iter()
            .copied()
            .map(|divider| {
                let key = (divider.split, divider.index);
                let lit = dragging == Some(key) || self.hover_divider == Some(key);
                let base = div()
                    .id(SharedString::from(format!(
                        "chart-divider-{}-{}",
                        divider.split, divider.index
                    )))
                    .absolute()
                    .flex()
                    .items_center()
                    .justify_center()
                    .occlude()
                    .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                        let next = if *hovered { Some(key) } else { None };
                        if this.hover_divider != next && this.split_drag.is_none() {
                            this.hover_divider = next;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                            cx.stop_propagation();
                            if event.click_count >= 2 {
                                this.reset_splits(cx);
                                return;
                            }
                            this.split_drag = Some(SplitDrag {
                                divider,
                                last: (f32::from(event.position.x), f32::from(event.position.y)),
                            });
                            cx.notify();
                        }),
                    );
                let line = div().bg(if lit {
                    theme::accent()
                } else {
                    gpui::rgba(0x00000000)
                });
                if divider.vertical {
                    base.left(relative(divider.from))
                        .w(relative(divider.to - divider.from))
                        .top(relative(divider.at))
                        .mt(px(-DIVIDER_HIT / 2.0))
                        .h(px(DIVIDER_HIT))
                        .cursor_row_resize()
                        .child(line.w_full().h(px(if lit { 3.0 } else { 1.0 })))
                } else {
                    base.top(relative(divider.from))
                        .h(relative(divider.to - divider.from))
                        .left(relative(divider.at))
                        .ml(px(-DIVIDER_HIT / 2.0))
                        .w(px(DIVIDER_HIT))
                        .cursor_col_resize()
                        .child(line.h_full().w(px(if lit { 3.0 } else { 1.0 })))
                }
            })
            .collect::<Vec<_>>();

        let rail = self.render_rail(cx);
        let favorites = self.render_favorites(window, cx);
        let flyout = self.render_flyout(cx);
        let area_cell = self.area.clone();
        let root_cell = self.root_bounds.clone();

        div()
            .relative()
            .flex()
            .flex_row()
            .flex_1()
            .w_full()
            .min_h_0()
            .bg(theme::bg())
            .track_focus(&self.focus)
            .child(
                canvas(
                    move |bounds, _window, _cx| root_cell.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(rail)
            .child(
                div().flex_1().h_full().min_w_0().flex().flex_col().child(
                    div()
                        .id("chart-area")
                        .relative()
                        .flex_1()
                        .w_full()
                        .min_h_0()
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                            this.on_area_move(event, cx);
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| this.end_split_drag(cx)),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| this.end_split_drag(cx)),
                        )
                        .child(
                            canvas(
                                move |bounds, _window, _cx| area_cell.set(Some(bounds)),
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .children(cells)
                        .children(divider_elements)
                        .children(favorites),
                ),
            )
            .children(flyout)
    }
}
