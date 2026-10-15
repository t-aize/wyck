//! Several charts on one screen, arranged by a layout, sized by dragging the lines between them,
//! and linked the way the user chooses.
//!
//! Each chart has its own timeframe, type, indicators and time zone, and (when the symbol link is
//! off) its own symbol. The links are the ones a trading platform offers; their rules live in
//! [`links`], as plain data, and this view only applies what they say.
//!
//! The charts do not know about each other. Each one reports what the user did as a
//! [`ChartEvent`](crate::ui::features::chart::events::ChartEvent), and this view applies the links, which keeps a chart free of loops: a chart
//! that is made to follow another never reports it. What the rest of the app acts on (a symbol to
//! pick, an order asked from the chart, a line dragged) goes up as a [`MultiChartEvent`].

pub mod charts;
pub mod clipboard;
pub mod drawing_cmds;
mod drawing_ui;
pub mod events;
mod favorites;
pub mod icon;
pub mod layout;
pub mod links;
pub mod split;

use self::layouts::{LayoutKey, layout};
use self::links::Links;
use self::split::{Divider, Node};
use crate::app::broker::session::Session;
use crate::app::drawings::Drawings;
use crate::app::market_data::live::LiveHub;
pub use crate::app::workspace::layouts;
use crate::app::workspace::{ChartState, Preferences, UsageLimits, Workspace};
use crate::ui::features::chart::events::{ChartAction, EditorRequest};
use crate::ui::features::chart::{Chart, ChartLine, LineId};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Entity, EventEmitter, MouseButton, MouseMoveEvent, Pixels, SharedString,
    Subscription, Window, canvas, div, px, relative,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wyck_drawings::model::tool::{Group, Tool};
use wyck_indicators::config::StudyConfig;
use wyck_market::LiveBarTracker;
use wyck_market::Timeframe;
use wyck_ui_kit::{text_input::TextInput, theme};

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
    sync: Links,
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
            bar: Default::default(),
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

    /// Makes the chart `delta` places after (or before) the active one the active one, going round.
    pub fn step_active(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.slots.len();
        if count > 1 {
            let next = (self.active as isize + delta).rem_euclid(count as isize) as usize;
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
        // The symbol is named on a chart only when the charts do not all show the same one: with one
        // symbol the header says it already.
        let names: Vec<Option<String>> = self
            .slots
            .iter()
            .map(|slot| slot.chart.read(cx).symbol().map(|s| s.name.to_string()))
            .collect();
        let show_symbol = names.iter().any(|name| *name != names[0]);
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
                chart.set_show_symbol(show_symbol, cx);
            });
        }
        let (rects, dividers) = self.tree.place(self.slots.len());
        // The bar of the selected drawing floats over the whole area, under the legends of the
        // charts along the top, so a small chart low in the layout does not have to hold it.
        let home_top = self
            .slots
            .iter()
            .zip(&rects)
            .filter(|(_, r)| r.y < 0.001)
            .map(|(slot, _)| slot.chart.read(cx).legend_bottom())
            .fold(0.0, f32::max);
        self.bar.set_home(home_top);
        let style_bar = self.render_style_bar(window, cx);
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

        let bar_drag = self.bar_drag_layer(cx);
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
                        .children(style_bar)
                        .children(bar_drag)
                        .children(favorites),
                ),
            )
            .children(flyout)
    }
}
