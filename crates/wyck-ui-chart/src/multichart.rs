//! Several charts on one screen, arranged by a layout, sized by dragging the lines between them,
//! and linked the way the user chooses.
//!
//! Each chart has its own timeframe, type, indicators and time zone, and (when the symbol link is
//! off) its own symbol. The links are the ones a trading platform offers; their rules live in
//! [`links`], as plain data, and this view only applies what they say.
//!
//! The charts do not know about each other. Each one reports what the user did as a
//! `ChartEvent`, and this view applies the links, which keeps a chart free of loops: a chart
//! that is made to follow another never reports it. What the rest of the app acts on (a symbol to
//! pick, an order asked from the chart, a line dragged) goes up as a [`MultiChartEvent`].

pub(crate) mod charts;
pub(crate) mod clipboard;
pub(crate) mod drawing_cmds;
mod drawing_ui;
pub(crate) mod events;
mod favorites;
pub mod icon;
pub(crate) mod layout;
pub mod links;
mod replay;
pub(crate) mod split;
mod view;

use self::layouts::{LayoutKey, layout};
use self::split::{Divider, Node};
use crate::chart::events::{ChartAction, EditorRequest};
use crate::chart::{Chart, ChartLine};
use gpui::prelude::*;
use gpui::{Bounds, Context, Entity, EventEmitter, Pixels, SharedString, Subscription, Window};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wyck_app::broker::session::Session;
use wyck_app::drawings::Drawings;
use wyck_app::market_data::live::LiveHub;
use wyck_app::replay::Replay;
use wyck_app::workspace::LinksPref;
use wyck_app::workspace::layouts;
use wyck_app::workspace::{ChartState, Preferences, UsageLimits, Workspace};
use wyck_chart::LineId;
use wyck_drawings::model::tool::{Group, Tool};
use wyck_indicators::config::StudyConfig;
use wyck_market::LiveBarTracker;
use wyck_market::Timeframe;
use wyck_ui_kit::text_input::TextInput;

/// How many recent tools and colors are kept.
const MAX_RECENT_TOOLS: usize = 8;
const MAX_RECENT_COLORS: usize = 6;

/// The symbol of a chart: its id, name and number of decimals.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The replay could not start or went on; the text is for the user.
    ReplayFailed(String),
    /// The user asked a chart for an order or an alert while a replay runs.
    ReplayBlocked,
}

struct Slot {
    chart: Entity<Chart>,
    /// The symbol the chart was saved with, until the symbol list can resolve it.
    wanted_symbol: Option<String>,
    _events: Subscription,
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
    hours: HashMap<i64, std::sync::Arc<wyck_market::TradingHours>>,
    /// The ids of the symbols of the account by name, for the indicators that read a second symbol.
    symbol_ids: std::sync::Arc<HashMap<String, i64>>,
    next_id: u64,
    key: LayoutKey,
    tree: Node,
    slots: Vec<Slot>,
    active: usize,
    sync: LinksPref,
    /// The indicators copied last, for Ctrl+V.
    clipboard: Option<Vec<StudyConfig>>,
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
    /// Where the style bar sits over the charts, and the drag that moves it.
    bar: drawing_ui::bar_place::BarPlace,
    /// The same for the bar of the favorite tools, which sits at the bottom.
    pill: drawing_ui::bar_place::BarPlace,
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
    /// The replay every chart follows while it runs.
    replay: Entity<Replay>,
    _replay_events: Subscription,
}

impl EventEmitter<MultiChartEvent> for MultiChart {}

impl MultiChart {
    /// Starts with the layout, charts and links the workspace remembers.
    pub fn new(
        session: Session,
        hub: Rc<LiveHub>,
        workspace: Entity<Workspace>,
        drawings: Entity<Drawings>,
        replay: Entity<Replay>,
        cx: &mut Context<Self>,
    ) -> Self {
        let replay_events = cx.subscribe(
            &replay,
            |this, _replay, event: &wyck_app::replay::ReplayEvent, cx| {
                this.on_replay_event(event, cx);
            },
        );
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
        let text_observe = cx.observe(&text_input, |this, _input, cx| this.on_text_edited(cx));
        let search_input = cx.new(|cx| TextInput::new(cx, "Search the tools"));
        let search_observe = cx.observe(&search_input, |_, _input, cx| cx.notify());
        let drawings_observe = cx.observe(&drawings, |this, _drawings, cx| {
            this.on_drawings_changed(cx);
        });
        let mut multi = Self {
            drawings,
            _drawings_observe: drawings_observe,
            text_input,
            _text_observe: text_observe,
            flyout: None,
            tool_search: false,
            search_input,
            _search_observe: search_observe,
            recent_tools: Vec::new(),
            recent_colors: Vec::new(),
            rail_slots: Rc::default(),
            root_bounds: Rc::new(Cell::new(None)),
            color_open: None,
            bar: drawing_ui::bar_place::BarPlace::default(),
            pill: drawing_ui::bar_place::BarPlace::at_bottom(
                favorites::PILL_BOTTOM,
                (prefs.favorites_place[0], prefs.favorites_place[1]),
            ),
            focus: cx.focus_handle(),
            last_tool: HashMap::new(),
            hub,
            tracker: LiveBarTracker::new(),
            hours: HashMap::new(),
            symbol_ids: std::sync::Arc::default(),
            session,
            workspace,
            next_id: 0,
            key,
            tree: layout::tree_for(key, &prefs),
            slots: Vec::new(),
            active: prefs.active_chart,
            sync: prefs.links,
            clipboard: None,
            syncing_studies: false,
            area: Rc::new(Cell::new(None)),
            split_drag: None,
            hover_divider: None,
            lines: HashMap::new(),
            replay,
            _replay_events: replay_events,
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

    pub fn sync(&self) -> LinksPref {
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

    #[expect(
        clippy::indexing_slicing,
        reason = "a layout always holds a chart and `active` is kept inside it"
    )]
    pub fn active_chart(&self) -> &Entity<Chart> {
        &self.slots[self.active].chart
    }

    pub fn has_chart_focus(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub fn active_timeframe(&self, cx: &gpui::App) -> Timeframe {
        self.active_chart().read(cx).timeframe()
    }

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

    /// Makes the chart `delta` places after (or before) the active one the active one, going round.
    pub fn step_active(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.slots.len();
        if count > 1 {
            let steps = delta.unsigned_abs() % count;
            let next = if delta >= 0 {
                (self.active + steps) % count
            } else {
                (self.active + count - steps) % count
            };
            self.activate(next, cx);
        }
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
                book.set_drawing_limit(limits.drawings_per_symbol);
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
}
