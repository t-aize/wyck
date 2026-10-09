//! The order ticket: the panel beside the charts where an order is put together and sent.
//!
//! It shows the live bid and ask of the symbol with the spread, the side, the kind of order
//! (market, limit, stop or stop limit) and the price of a pending order.
//!
//! The volume is typed in lots or units, or worked out from what the order may lose at its stop
//! loss (a share of the balance or of the equity, or an amount) or from a share of the free
//! margin. The stop loss and the take profit are given as a price or as a distance from the
//! entry in pips, in money or in percent of the balance, and the take profit also in multiples
//! of the risk. What the order risks and may gain, its risk to reward, its margin and what a pip
//! is worth are shown as the fields change. Money is converted into the deposit currency at live
//! prices, through the symbols the server names for it.
//!
//! A pending order can be given an expiry, a market order a largest slippage, and a stop loss can
//! trail the price or be guaranteed. An order can carry a comment. What is open on the symbol is
//! listed in the panel, with what can be done to it: close it, close half, move the stop loss to
//! the entry, trail it, reverse it.
//!
//! While a pending price or a protection is set, it shows on the chart as a line that can be
//! dragged; the ticket follows the line (see [`OrderTicket::lines`]).
//!
//! An order taken from a long or short position drawing (see [`PositionLink`]) follows that drawing
//! until it is sent. When the drawing sets its stop loss as a multiple of the ATR and its take
//! profit as a multiple of the risk, the ticket is set the same way, so the stop moves with the ATR
//! and with the entry while nothing is sent. The levels are then read-only in the ticket: they are
//! changed on the drawing, or with "Unlink" to take them over.
//!
//! An order is sent after a confirmation, unless one-click trading is on; then the send button
//! sends at once. The buy and sell buttons only pick the side.
//!
//! Everything about the panel can be changed (see [`prefs`] and [`customize`]): its side and
//! width, which blocks show and in what order, the size shortcuts and what a new order starts
//! with.

use std::time::Duration;

use crate::app::broker::trading::{NewOrderReq, NewOrderType};
use crate::domain::trading::TradeSide;
use gpui::prelude::*;
use gpui::{App, Context, Entity, EventEmitter, SharedString, Subscription, Window};
use gpui_kit::component::input::{InputEvent, InputState};

use super::account::{Account, Busy};
use super::math::{self, Contract, Offset, Pending, Scale, SizeMode, Stepped};
use crate::app::market_data::now_ms;
use crate::app::system::runtime;
use crate::domain::drawings::model::Dash;
use crate::domain::indicators::atr_stop::AtrStop;
use crate::ui::features::chart::Chart;
use crate::ui::features::chart::{ChartLine, LineId, PlanState, PositionLink, PositionPlan};
use crate::ui::features::multichart::SymbolRef;

pub mod customize;
pub mod prefs;
mod templates;
mod view;

pub use self::prefs::{Kind, Layout, TicketPrefs};
use self::prefs::{Span, Tif};
use super::guard::Verdict;
use super::plan::{self, ExitPlan};
use crate::ui::kit::confirm::{Details, confirm_details};
use crate::ui::kit::number;

/// Which line of the ticket a pending line on the chart stands for.
pub const LINE_ENTRY: u8 = 0;
pub const LINE_STOP: u8 = 1;
/// The take profit; with exits, the first of them (the next ones follow at 3 and 4).
pub const LINE_TARGET: u8 = 2;

pub enum TicketEvent {
    /// The ticket's lines on the chart changed.
    LinesChanged,
    /// How the ticket sizes orders or looks changed, to be remembered.
    Settings(Box<TicketPrefs>),
    /// The user closed the ticket.
    Close,
}

/// The margin the server gave for one lot and for the order's volume, for a buy and a sell.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Margin {
    volume: i64,
    lot: (f64, f64),
    order: (f64, f64),
}

/// A long or short position drawing the ticket follows until its order is sent. The drawing sets
/// the side, the entry, and the stop loss and take profit (as an ATR stop and a multiple of the
/// risk when it is set that way); the ticket keeps the size, the kind of order and the options.
struct Link {
    /// The drawing as the ticket last read it.
    plan: PositionPlan,
    /// How the ticket was set before, put back once the order is sent.
    before: Before,
    /// The kind of a pending order (limit or stop) follows the entry against the market.
    follow_kind: bool,
}

/// The settings of the protections a drawing takes over.
#[derive(Debug, Clone)]
struct Before {
    stop_atr: bool,
    atr: AtrStop,
    stop_unit: Offset,
    target_unit: Offset,
}

/// Whether two readings of a drawing ask the ticket for something different. The stop loss of an
/// ATR stop and the take profit of a multiple of the risk are worked out by the ticket itself, so
/// they do not count.
fn same_shape(a: &PositionPlan, b: &PositionPlan) -> bool {
    a.buy == b.buy
        && a.entry == b.entry
        && a.atr == b.atr
        && a.rr == b.rr
        && (a.atr.is_some() || a.stop_loss == b.stop_loss)
        && (a.rr.is_some() || a.take_profit == b.take_profit)
}

/// The order worked out from the fields.
#[derive(Debug, Clone, Default)]
struct Plan {
    entry: Option<f64>,
    stop: Option<f64>,
    target: Option<f64>,
    sized: Option<Stepped>,
    /// Deposit currency per unit of quote currency.
    rate: Option<f64>,
    /// What a move of 1.0 in price is worth for the volume, in the deposit currency.
    money_per_price: Option<f64>,
    stop_distance: Option<f64>,
    /// What the order loses at the stop loss and gains at the take profit.
    risk: Option<f64>,
    reward: Option<f64>,
    /// Why the order cannot be sent yet.
    problem: Option<String>,
}

pub struct OrderTicket {
    account: Entity<Account>,
    symbol: Option<SymbolRef>,
    buy: bool,
    kind: Kind,
    size_mode: SizeMode,
    size: Entity<InputState>,
    price: Entity<InputState>,
    stop_loss: Entity<InputState>,
    take_profit: Entity<InputState>,
    stop_unit: Offset,
    stop_atr: bool,
    atr: AtrStop,
    atr_seeded: bool,
    atr_length: Entity<InputState>,
    atr_multiplier: Entity<InputState>,
    chart: Option<Entity<Chart>>,
    chart_observe: Option<Subscription>,
    target_unit: Offset,
    stop_on: bool,
    target_on: bool,
    one_click: bool,
    tif: Tif,
    expiry: Entity<InputState>,
    expiry_span: Span,
    /// The largest slippage of a market order, or the range of the limit of a stop limit, in pips.
    slippage: Entity<InputState>,
    slippage_on: bool,
    trailing: bool,
    guaranteed: bool,
    comment: Entity<InputState>,
    layout: Layout,
    /// The protections the defaults ask for wait for a price to start from.
    autofill: bool,
    margin: Option<Margin>,
    /// The symbol and volume the margin was last asked for.
    margin_asked: Option<(i64, i64)>,
    /// Bumped per margin request, so an old answer is ignored.
    margin_epoch: u64,
    /// The position drawing this ticket follows.
    link: Option<Link>,
    /// Whether the settings of the ATR stop show, and the block of the options.
    atr_open: bool,
    options_open: bool,
    /// The exits of a plan (see [`plan`]). The shares and targets are typed in the inputs below;
    /// this holds the rest, and [`Self::exits_now`] reads the inputs into it.
    exits: ExitPlan,
    /// The share and the take profit (in R) typed for each of the most legs.
    leg_inputs: Vec<(Entity<InputState>, Entity<InputState>)>,
    be_offset: Entity<InputState>,
    oco_pips: Entity<InputState>,
    /// Closing the position after a time, and the number typed for it.
    time_stop: prefs::TimeStopPrefs,
    time_stop_amount: Entity<InputState>,
    /// The setups saved under a name, and the name being typed for the next one.
    plans: Vec<prefs::PlanTemplate>,
    plan_name: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TicketEvent> for OrderTicket {}

impl OrderTicket {
    pub fn new(
        account: Entity<Account>,
        prefs: TicketPrefs,
        one_click: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prefs = prefs.normalized();
        let layout = prefs.layout;
        let defaults = layout.defaults.clone();
        let size = cx.new(|cx| {
            number::state(
                size_kind(prefs.size_mode, &Contract::default()),
                prefs.size,
                window,
                cx,
            )
        });
        let price = cx.new(|cx| InputState::new(window, cx));
        let stop_loss = cx.new(|cx| {
            number::empty(
                offset_kind(prefs.stop_unit, &Contract::default()),
                window,
                cx,
            )
        });
        let atr_length = cx.new(|cx| {
            number::state(number::Kind::Count, prefs.atr.length as f64, window, cx).max(1_000.0)
        });
        let atr_multiplier =
            cx.new(|cx| number::state(number::Kind::Multiplier, prefs.atr.multiplier, window, cx));
        let take_profit = cx.new(|cx| {
            number::empty(
                offset_kind(prefs.target_unit, &Contract::default()),
                window,
                cx,
            )
        });
        let expiry = cx.new(|cx| number::state(number::Kind::Count, defaults.expiry, window, cx));
        let slippage =
            cx.new(|cx| number::state(number::Kind::Slippage, defaults.slippage_pips, window, cx));
        let comment = cx.new(|cx| InputState::new(window, cx).placeholder("Comment (optional)"));
        let mut subscriptions = vec![cx.observe(&account, |_this, _account, cx| cx.notify())];
        subscriptions.push(cx.subscribe(&size, |this, _state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.settings_changed(cx);
                cx.emit(TicketEvent::LinesChanged);
                cx.notify();
            }
        }));
        for state in [&price, &stop_loss, &take_profit, &slippage] {
            subscriptions.push(
                cx.subscribe(state, |_this, _state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    }
                }),
            );
        }
        subscriptions.push(
            cx.subscribe(&atr_length, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(value) = Self::read(&this.atr_length, cx) {
                        this.atr.length = (value as usize).clamp(1, 1_000);
                    }
                    this.settings_changed(cx);
                    cx.emit(TicketEvent::LinesChanged);
                    cx.notify();
                }
            }),
        );
        subscriptions.push(
            cx.subscribe(&atr_multiplier, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(value) = Self::read(&this.atr_multiplier, cx).filter(|v| *v > 0.0) {
                        this.atr.multiplier = value.min(1_000.0);
                    }
                    this.settings_changed(cx);
                    cx.emit(TicketEvent::LinesChanged);
                    cx.notify();
                }
            }),
        );
        for state in [&expiry, &comment] {
            subscriptions.push(
                cx.subscribe(state, |_this, _state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                }),
            );
        }
        // The inputs of the exits: one pair per leg the plan can have.
        let exits = prefs.exits.clone();
        let mut leg_inputs = Vec::new();
        for index in 0..plan::MAX_LEGS {
            let leg = exits.legs.get(index).copied().unwrap_or(plan::Leg {
                share: 0.0,
                target_r: 0.0,
            });
            let share =
                cx.new(|cx| number::state(number::Kind::Percent, leg.share, window, cx).max(100.0));
            let target =
                cx.new(|cx| number::state(number::Kind::Ratio, leg.target_r, window, cx).min(0.0));
            leg_inputs.push((share, target));
        }
        let be_offset = cx
            .new(|cx| number::state(number::Kind::Pips, exits.break_even.offset_pips, window, cx));
        let oco_pips = cx.new(|cx| number::state(number::Kind::Pips, exits.oco_pips, window, cx));
        for state in leg_inputs
            .iter()
            .flat_map(|(a, b)| [a, b])
            .chain([&be_offset, &oco_pips])
        {
            subscriptions.push(cx.subscribe(state, |this, _state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.settings_changed(cx);
                    cx.emit(TicketEvent::LinesChanged);
                    cx.notify();
                }
            }));
        }
        let time_stop_amount = cx.new(|cx| {
            number::state(number::Kind::Count, prefs.time_stop.amount, window, cx)
                .min(1.0)
                .max(f64::from(plan::MAX_TIME_STOP_MINUTES))
        });
        subscriptions.push(cx.subscribe(
            &time_stop_amount,
            |this, _state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.settings_changed(cx);
                    cx.notify();
                }
            },
        ));
        let plan_name = cx.new(|cx| InputState::new(window, cx).placeholder("Name of this setup"));
        subscriptions.push(
            cx.subscribe(&plan_name, |_this, _state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        );
        let atr_seeded = prefs.stop_atr || prefs.atr != AtrStop::default();
        let mut ticket = Self {
            account,
            symbol: None,
            buy: true,
            kind: defaults.kind,
            size_mode: prefs.size_mode,
            size,
            price,
            stop_loss,
            take_profit,
            stop_unit: prefs.stop_unit,
            stop_atr: prefs.stop_atr,
            atr: prefs.atr,
            atr_seeded,
            atr_length,
            atr_multiplier,
            chart: None,
            chart_observe: None,
            target_unit: prefs.target_unit,
            stop_on: defaults.stop_on,
            target_on: defaults.target_on,
            one_click,
            tif: defaults.tif,
            expiry,
            expiry_span: defaults.expiry_span,
            slippage,
            slippage_on: false,
            trailing: false,
            guaranteed: false,
            comment,
            autofill: defaults.stop_on || defaults.target_on,
            layout,
            margin: None,
            margin_asked: None,
            margin_epoch: 0,
            link: None,
            atr_open: false,
            options_open: false,
            exits,
            leg_inputs,
            be_offset,
            oco_pips,
            time_stop: prefs.time_stop,
            time_stop_amount,
            plans: prefs.plans.clone(),
            plan_name,
            _subscriptions: subscriptions,
        };
        // A stop loss sizing the order by risk cannot depend on the volume.
        if ticket.size_mode.is_risk() && ticket.stop_unit.needs_volume() {
            ticket.stop_unit = Offset::Pips;
        }
        if ticket.stop_unit == Offset::Ratio {
            ticket.stop_unit = Offset::Pips;
        }
        ticket.apply_steps(window, cx);
        ticket
    }

    pub fn symbol(&self) -> Option<&SymbolRef> {
        self.symbol.as_ref()
    }

    pub fn one_click(&self) -> bool {
        self.one_click
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// How the ticket sizes orders and looks now, to be remembered. What a followed drawing sets
    /// is not: it lasts as long as the link.
    pub fn prefs(&self, cx: &App) -> TicketPrefs {
        let (stop_atr, atr, stop_unit, target_unit) = match &self.link {
            Some(link) => (
                link.before.stop_atr,
                link.before.atr.clone(),
                link.before.stop_unit,
                link.before.target_unit,
            ),
            None => (
                self.stop_atr,
                self.atr.clone(),
                self.stop_unit,
                self.target_unit,
            ),
        };
        TicketPrefs {
            size_mode: self.size_mode,
            size: Self::read(&self.size, cx).unwrap_or(0.0),
            stop_unit,
            stop_atr,
            atr,
            target_unit,
            layout: self.layout.clone(),
            exits: self.exits_now(cx),
            time_stop: self.time_stop_now(cx),
            plans: self.plans.clone(),
        }
    }

    /// The plan of exits as typed now: the legs the inputs hold, and the other settings.
    pub(super) fn exits_now(&self, cx: &App) -> ExitPlan {
        let mut exits = self.exits.clone();
        let count = exits.legs.len().clamp(2, plan::MAX_LEGS);
        exits.legs = self
            .leg_inputs
            .iter()
            .take(count)
            .map(|(share, target)| plan::Leg {
                share: Self::read(share, cx).unwrap_or(0.0),
                target_r: Self::read(target, cx).unwrap_or(0.0),
            })
            .collect();
        exits.break_even.offset_pips = Self::read(&self.be_offset, cx).unwrap_or(0.0);
        exits.oco_pips = Self::read(&self.oco_pips, cx).unwrap_or(0.0);
        exits.normalized()
    }

    /// Cuts the plan into two or three exits, with the shares and targets it starts from.
    pub(super) fn set_leg_count(
        &mut self,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = count.clamp(2, plan::MAX_LEGS);
        let legs = if count == 2 {
            vec![
                plan::Leg {
                    share: 60.0,
                    target_r: 1.5,
                },
                plan::Leg {
                    share: 40.0,
                    target_r: 0.0,
                },
            ]
        } else {
            ExitPlan::default().legs
        };
        for (index, (share, target)) in self.leg_inputs.iter().enumerate() {
            let leg = legs.get(index).copied().unwrap_or(plan::Leg {
                share: 0.0,
                target_r: 0.0,
            });
            let (share, target) = (share.clone(), target.clone());
            self.write(&share, number::format(leg.share, 2), window, cx);
            self.write(&target, number::format(leg.target_r, 2), window, cx);
        }
        let mut exits = self.exits_now(cx);
        exits.legs = legs;
        self.exits = exits.normalized();
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Changes the plan of exits (its switches, its number of legs), and remembers it.
    pub(super) fn edit_exits(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut ExitPlan),
    ) {
        let mut exits = self.exits_now(cx);
        change(&mut exits);
        self.exits = exits;
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    fn settings_changed(&self, cx: &mut Context<Self>) {
        let prefs = self.prefs(cx);
        cx.emit(TicketEvent::Settings(Box::new(prefs)));
    }

    pub fn set_chart(&mut self, chart: Entity<Chart>, cx: &mut Context<Self>) {
        if self.chart.as_ref().is_some_and(|current| current == &chart) {
            return;
        }
        self.chart_observe = Some(cx.observe(&chart, |_, _, cx| {
            cx.emit(TicketEvent::LinesChanged);
            cx.notify();
        }));
        self.chart = Some(chart);
        cx.notify();
    }

    pub fn request_atr(&self, cx: &mut Context<Self>) {
        if self.stop_atr
            && self.stop_on
            && let Some(chart) = &self.chart
        {
            chart.update(cx, |chart, cx| chart.request_atr(&self.atr, cx));
        }
    }

    /// Changes how the panel looks and what it holds, and remembers it.
    pub fn edit_layout(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Layout)) {
        let mut layout = self.layout.clone();
        change(&mut layout);
        let layout = layout.normalized();
        if layout != self.layout {
            self.layout = layout;
            self.settings_changed(cx);
            cx.notify();
        }
    }

    /// Puts the look and the defaults of the panel back as they were first.
    pub fn reset_layout(&mut self, cx: &mut Context<Self>) {
        self.edit_layout(cx, |layout| *layout = Layout::default());
    }

    /// The ticket is for this symbol now.
    pub fn set_symbol(
        &mut self,
        symbol: Option<SymbolRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.symbol.as_ref().map(|s| s.id) == symbol.as_ref().map(|s| s.id) {
            // The same symbol, maybe with the decimals the broker just gave.
            if self.symbol != symbol {
                self.symbol = symbol;
                self.apply_steps(window, cx);
                cx.notify();
            }
            return;
        }
        self.symbol = symbol;
        let id = self.symbol.as_ref().map(|s| s.id);
        self.account.update(cx, |account, cx| account.focus(id, cx));
        // A drawing belongs to a symbol: the ticket lets go of it, and of what it set.
        self.release_link(true, window, cx);
        self.start_over();
        self.margin = None;
        self.margin_asked = None;
        for state in [&self.price, &self.stop_loss, &self.take_profit] {
            state.update(cx, |s, cx| s.set_value("", window, cx));
        }
        // Lots and units of one symbol mean something else on another.
        if matches!(self.size_mode, SizeMode::Lots | SizeMode::Units) {
            let contract = self.contract(cx);
            let lots = contract.lots_of_volume(contract.min_volume.max(contract.lot_size / 10));
            let value = match self.size_mode {
                SizeMode::Units => lots * contract.lot_size as f64 / 100.0,
                _ => lots,
            };
            self.write(&self.size.clone(), number::format(value, 2), window, cx);
        }
        self.apply_steps(window, cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// What a new order starts with.
    fn start_over(&mut self) {
        let defaults = &self.layout.defaults;
        self.kind = defaults.kind;
        self.tif = defaults.tif;
        self.expiry_span = defaults.expiry_span;
        self.stop_on = defaults.stop_on;
        self.target_on = defaults.target_on;
        self.autofill = self.stop_on || self.target_on;
        self.slippage_on = false;
        self.trailing = false;
        self.guaranteed = false;
    }

    /// Fills the ticket from the chart: a side, a price (a pending order, limit or stop by where
    /// it is), and protection. From a position drawing (`link`) it also follows the drawing.
    #[allow(clippy::too_many_arguments)]
    pub fn prefill(
        &mut self,
        buy: bool,
        entry: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        link: Option<PositionLink>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Some(link), Some(entry), Some(stop_loss), Some(take_profit)) =
            (link, entry, stop_loss, take_profit)
        {
            self.begin_link(
                PositionPlan {
                    drawing: link.drawing,
                    buy,
                    entry,
                    stop_loss,
                    take_profit,
                    atr: link.atr,
                    rr: link.rr,
                },
                window,
                cx,
            );
            return;
        }
        // Values of the chart's own: nothing set by an earlier drawing stays.
        self.release_link(true, window, cx);
        self.buy = buy;
        match entry {
            Some(price) => {
                let (bid, ask) = self.quote(cx);
                // A stop limit stays one when the price it is given is a stop price.
                self.kind = match math::pending_kind(buy, price, bid, ask) {
                    Pending::Limit => Kind::Limit,
                    Pending::Stop if self.kind == Kind::StopLimit => Kind::StopLimit,
                    Pending::Stop => Kind::Stop,
                };
                let value = self.contract(cx).format_price(price);
                self.write(&self.price.clone(), value, window, cx);
            }
            None => self.kind = self.layout.defaults.kind,
        }
        self.stop_on = stop_loss.is_some() || self.layout.defaults.stop_on;
        self.target_on = take_profit.is_some() || self.layout.defaults.target_on;
        self.autofill =
            (stop_loss.is_none() && self.stop_on) || (take_profit.is_none() && self.target_on);
        // The stop loss first: a volume sized by risk and a take profit in R depend on it.
        match stop_loss {
            Some(price) => self.set_protection(true, price, window, cx),
            None => self.write(&self.stop_loss.clone(), String::new(), window, cx),
        }
        match take_profit {
            Some(price) => self.set_protection(false, price, window, cx),
            None => self.write(&self.take_profit.clone(), String::new(), window, cx),
        }
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Starts following a position drawing: the ticket takes its side, its entry and its
    /// protection, set the way the drawing sets them, and keeps up with it until the order is
    /// sent or the link is cut.
    fn begin_link(&mut self, plan: PositionPlan, window: &mut Window, cx: &mut Context<Self>) {
        // A second drawing takes over from the first: what the ticket was before stays.
        let before = match self.link.take() {
            Some(link) => link.before,
            None => Before {
                stop_atr: self.stop_atr,
                atr: self.atr.clone(),
                stop_unit: self.stop_unit,
                target_unit: self.target_unit,
            },
        };
        self.start_over();
        self.link = Some(Link {
            plan: plan.clone(),
            before,
            follow_kind: true,
        });
        self.apply_link(&plan, window, cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Writes what a drawing asks into the fields.
    fn apply_link(&mut self, plan: &PositionPlan, window: &mut Window, cx: &mut Context<Self>) {
        self.buy = plan.buy;
        self.autofill = false;
        self.stop_on = true;
        match &plan.atr {
            Some(atr) => {
                self.atr = atr.clone();
                self.atr_seeded = true;
                self.stop_atr = true;
                self.write(
                    &self.atr_length.clone(),
                    self.atr.length.to_string(),
                    window,
                    cx,
                );
                self.write(
                    &self.atr_multiplier.clone(),
                    number::format(self.atr.multiplier, 4),
                    window,
                    cx,
                );
                self.request_atr(cx);
            }
            None => {
                self.stop_atr = false;
                self.stop_unit = Offset::Price;
                let text = self.contract(cx).format_price(plan.stop_loss);
                self.write(&self.stop_loss.clone(), text, window, cx);
            }
        }
        self.target_on = true;
        match plan.rr {
            Some(rr) => {
                self.target_unit = Offset::Ratio;
                self.write(&self.take_profit.clone(), number::format(rr, 4), window, cx);
            }
            None => {
                self.target_unit = Offset::Price;
                let text = self.contract(cx).format_price(plan.take_profit);
                self.write(&self.take_profit.clone(), text, window, cx);
            }
        }
        let text = self.contract(cx).format_price(plan.entry);
        self.write(&self.price.clone(), text, window, cx);
        self.refresh_kind(cx);
        self.apply_steps(window, cx);
    }

    /// A pending order at the entry of the drawing is a limit or a stop by where the market is.
    fn refresh_kind(&mut self, cx: &App) {
        let Some(link) = &self.link else { return };
        if !link.follow_kind {
            return;
        }
        let (bid, ask) = self.quote(cx);
        let kind = match math::pending_kind(link.plan.buy, link.plan.entry, bid, ask) {
            Pending::Limit => Kind::Limit,
            Pending::Stop if self.kind == Kind::StopLimit => Kind::StopLimit,
            Pending::Stop => Kind::Stop,
        };
        self.kind = kind;
    }

    /// Keeps up with the drawing: what it asks now is written in the ticket, and a drawing that
    /// was deleted lets the ticket go.
    fn sync_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(id), Some(chart), Some(symbol)) = (
            self.link.as_ref().map(|link| link.plan.drawing),
            self.chart.clone(),
            self.symbol.as_ref().map(|s| s.name.to_string()),
        ) else {
            return;
        };
        match chart.read(cx).position_plan(id, &symbol, cx) {
            PlanState::Waiting => {}
            PlanState::Gone => {
                self.release_link(true, window, cx);
                crate::ui::kit::toast::show(
                    cx,
                    crate::ui::kit::toast::Kind::Info,
                    "Drawing removed",
                    "The ticket is free again and back to your own settings.",
                );
            }
            PlanState::Ready(plan) => {
                let changed = self
                    .link
                    .as_ref()
                    .is_some_and(|link| !same_shape(&plan, &link.plan));
                if changed {
                    self.apply_link(&plan, window, cx);
                    cx.emit(TicketEvent::LinesChanged);
                }
                if let Some(link) = &mut self.link {
                    link.plan = plan;
                }
                self.refresh_kind(cx);
            }
        }
    }

    /// Lets go of the drawing. With `restore`, the ticket goes back to what it was before the
    /// drawing set it (after the order is sent, or when the drawing is gone); without, the values
    /// stay for the user to change (the "Unlink" button).
    fn release_link(&mut self, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self.link.take() else {
            return;
        };
        if restore {
            let before = link.before;
            self.stop_atr = before.stop_atr;
            self.atr = before.atr;
            self.stop_unit = before.stop_unit;
            self.target_unit = before.target_unit;
            self.write(
                &self.atr_length.clone(),
                self.atr.length.to_string(),
                window,
                cx,
            );
            self.write(
                &self.atr_multiplier.clone(),
                number::format(self.atr.multiplier, 4),
                window,
                cx,
            );
            self.start_over();
            for state in [&self.price, &self.stop_loss, &self.take_profit] {
                state.update(cx, |s, cx| s.set_value("", window, cx));
            }
            self.apply_steps(window, cx);
        } else {
            self.settings_changed(cx);
        }
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// The "Unlink" button: the values stay, and can be changed.
    fn unlink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release_link(false, window, cx);
    }

    /// A line of the ticket was dragged on the chart.
    pub fn line_moved(
        &mut self,
        line: u8,
        price: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The drawing carries the levels while it is followed; its own lines are the ones to drag.
        if self.link.is_some() {
            return;
        }
        match line {
            LINE_ENTRY => {
                let value = self.contract(cx).format_price(price);
                self.write(&self.price.clone(), value, window, cx);
            }
            LINE_STOP => {
                self.stop_atr = false;
                self.stop_unit = Offset::Price;
                self.set_protection(true, price, window, cx);
            }
            n if self.exits.on => {
                // A take profit of the exits is given in R: the line sets it from the distance.
                let plan = self.plan(cx);
                if let (Some(entry), Some(distance), Some((_, target))) = (
                    plan.entry,
                    plan.stop_distance,
                    self.leg_inputs.get(usize::from(n - LINE_TARGET)),
                ) {
                    let r = ((price - entry) * math::protection_side(self.buy, false)) / distance;
                    if r.is_finite() && r > 0.0 {
                        let target = target.clone();
                        self.write(&target, number::format(r, 2), window, cx);
                    }
                }
            }
            _ => {
                self.target_unit = Offset::Price;
                self.set_protection(false, price, window, cx);
            }
        }
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    fn write(
        &self,
        state: &Entity<InputState>,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        state.update(cx, |s, cx| s.set_value(value, window, cx));
    }

    /// Writes a stop loss or take profit at `price`, in the unit it is given in. When the unit
    /// cannot say it yet (no entry, no rate), the protection is given as a price instead.
    fn set_protection(
        &mut self,
        stop: bool,
        price: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if stop && self.stop_atr {
            self.stop_atr = false;
            self.settings_changed(cx);
        }
        let plan = self.plan(cx);
        let unit = if stop {
            self.stop_unit
        } else {
            self.target_unit
        };
        let contract = self.contract(cx);
        let value = match unit {
            Offset::Price => None,
            _ => plan.entry.and_then(|entry| {
                let distance = (price - entry) * math::protection_side(self.buy, stop);
                self.scale(&plan, cx).value(unit, distance)
            }),
        };
        let text = match value {
            Some(value) => format_offset(unit, value),
            None => {
                if unit != Offset::Price {
                    if stop {
                        self.stop_unit = Offset::Price;
                    } else {
                        self.target_unit = Offset::Price;
                    }
                    self.apply_steps(window, cx);
                    self.settings_changed(cx);
                }
                contract.format_price(price)
            }
        };
        let state = if stop {
            self.stop_loss.clone()
        } else {
            self.take_profit.clone()
        };
        self.write(&state, text, window, cx);
    }

    /// Whether a side can be picked: a followed drawing is a long or a short, not both.
    fn side_allowed(&self, buy: bool) -> bool {
        self.link.as_ref().is_none_or(|link| link.plan.buy == buy)
    }

    /// Picks a side from the keyboard.
    pub fn choose_side(&mut self, buy: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.set_side(buy, window, cx);
    }

    /// Sends from the keyboard, as the send button does. A second press while the first order is
    /// in flight does nothing.
    pub fn send_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.account.read(cx).is_busy(Busy::Placing) {
            return;
        }
        self.send(window, cx);
    }

    fn set_side(&mut self, buy: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.side_allowed(buy) {
            return;
        }
        self.buy = buy;
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Picks the kind of order. A pending one starts at the market price of its side, or at the
    /// entry of the drawing that is followed.
    fn set_kind(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.kind = kind;
        if let Some(link) = &mut self.link {
            link.follow_kind = kind.is_pending();
            self.refresh_kind(cx);
        } else if kind.is_pending() && Self::read(&self.price, cx).is_none() {
            self.price_at_market(window, cx);
        }
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Writes the market price of the side in the price of a pending order.
    fn price_at_market(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (bid, ask) = self.quote(cx);
        if let Some(price) = if self.buy { ask } else { bid } {
            let value = self.contract(cx).format_price(price);
            self.write(&self.price.clone(), value, window, cx);
        }
    }

    /// Turns a stop loss or take profit on or off. One turned on empty starts at the distance of
    /// the defaults: some pips from the entry for a stop loss, and a multiple of the risk (or of
    /// those pips) for a take profit.
    fn toggle_protection(&mut self, stop: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.link.is_some() {
            return;
        }
        let on = if stop {
            self.stop_on = !self.stop_on;
            self.stop_on
        } else {
            self.target_on = !self.target_on;
            self.target_on
        };
        if on {
            self.fill_protection(stop, window, cx);
        }
        if stop && !on {
            self.trailing = false;
            self.guaranteed = false;
        }
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Starts a protection that is on and empty at the distance of the defaults.
    fn fill_protection(&mut self, stop: bool, window: &mut Window, cx: &mut Context<Self>) {
        let state = if stop {
            self.stop_loss.clone()
        } else {
            self.take_profit.clone()
        };
        if Self::read(&state, cx).is_some() {
            return;
        }
        let plan = self.plan(cx);
        let pip = self.contract(cx).pip();
        let (stop_pips, ratio) = (
            self.layout.defaults.stop_pips,
            self.layout.defaults.target_ratio,
        );
        let distance = if stop {
            Some(stop_pips * pip)
        } else {
            plan.stop_distance
                .map(|d| d * ratio)
                .or(Some(stop_pips * ratio * pip))
        };
        if let (Some(entry), Some(distance)) = (plan.entry, distance) {
            let price = entry + math::protection_side(self.buy, stop) * distance;
            self.set_protection(stop, price, window, cx);
        }
    }

    /// Starts the protections the defaults ask for, once a price is known.
    fn apply_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.autofill = false;
        if self.stop_on {
            self.fill_protection(true, window, cx);
        }
        if self.target_on {
            self.fill_protection(false, window, cx);
        }
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Gives a protection in another unit, keeping its price.
    fn set_unit(&mut self, stop: bool, unit: Offset, window: &mut Window, cx: &mut Context<Self>) {
        if self.link.is_some() {
            return;
        }
        let plan = self.plan(cx);
        let price = if stop { plan.stop } else { plan.target };
        if stop {
            self.stop_atr = false;
            self.stop_unit = unit;
        } else {
            self.target_unit = unit;
        }
        match price {
            Some(price) => self.set_protection(stop, price, window, cx),
            None => {
                let state = if stop {
                    self.stop_loss.clone()
                } else {
                    self.take_profit.clone()
                };
                self.write(&state, String::new(), window, cx);
            }
        }
        self.apply_steps(window, cx);
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    fn set_atr_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.link.is_some() {
            return;
        }
        if !self.atr_seeded
            && let Some(chart) = &self.chart
            && let Some(study) = chart
                .read(cx)
                .settings()
                .studies
                .iter()
                .find(|study| study.kind == crate::domain::indicators::StudyKind::Atr)
        {
            self.atr.length = (study.input("length") as usize).clamp(1, 1_000);
            self.atr.smoothing = match study.input("smoothing") as usize {
                1 => crate::domain::indicators::atr_stop::Smoothing::Sma,
                2 => crate::domain::indicators::atr_stop::Smoothing::Ema,
                3 => crate::domain::indicators::atr_stop::Smoothing::Wma,
                _ => crate::domain::indicators::atr_stop::Smoothing::Rma,
            };
            self.write(
                &self.atr_length.clone(),
                self.atr.length.to_string(),
                window,
                cx,
            );
        }
        self.atr_seeded = true;
        self.stop_atr = true;
        self.stop_on = true;
        if !self.target_on || Self::read(&self.take_profit, cx).is_none() {
            self.target_on = true;
            self.target_unit = Offset::Ratio;
            self.write(&self.take_profit.clone(), "2".to_owned(), window, cx);
        }
        self.request_atr(cx);
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Gives the take profit as a multiple of the risk, turning the stop loss on if it is off
    /// (a multiple of nothing is no price).
    fn set_target_ratio(&mut self, ratio: f64, window: &mut Window, cx: &mut Context<Self>) {
        if self.link.is_some() {
            return;
        }
        if !self.stop_on {
            self.toggle_protection(true, window, cx);
        }
        self.target_on = true;
        self.target_unit = Offset::Ratio;
        self.write(
            &self.take_profit.clone(),
            number::format(ratio, 2),
            window,
            cx,
        );
        self.apply_steps(window, cx);
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Sets how many ATRs away the ATR stop sits.
    fn set_atr_multiplier(&mut self, multiplier: f64, window: &mut Window, cx: &mut Context<Self>) {
        if self.link.is_some() {
            return;
        }
        self.write(
            &self.atr_multiplier.clone(),
            number::format(multiplier, 2),
            window,
            cx,
        );
    }

    /// Sizes the volume another way, keeping the volume where it can.
    fn set_size_mode(&mut self, mode: SizeMode, window: &mut Window, cx: &mut Context<Self>) {
        if mode == self.size_mode {
            return;
        }
        let plan = self.plan(cx);
        let contract = self.contract(cx);
        let summary = self.account.read(cx).summary();
        // The stop loss of a volume sized by risk cannot depend on the volume.
        if mode.is_risk() && self.stop_unit.needs_volume() {
            let stop = plan.stop;
            self.stop_unit = Offset::Pips;
            if let Some(price) = stop {
                self.set_protection(true, price, window, cx);
            }
        }
        let volume = plan.sized.map(|s| s.volume);
        let share = |amount: f64, of: f64| (of > 0.0).then(|| amount / of * 100.0);
        let value = match mode {
            SizeMode::Lots => volume.map(|v| contract.lots_of_volume(v)),
            SizeMode::Units => volume.map(|v| v as f64 / 100.0),
            SizeMode::RiskBalance => plan.risk.and_then(|r| share(r, summary.balance)),
            SizeMode::RiskEquity => plan.risk.and_then(|r| share(r, summary.equity)),
            SizeMode::RiskMoney => plan.risk,
            SizeMode::FreeMargin => self
                .margin
                .filter(|m| Some(m.volume) == volume)
                .and_then(|m| share(side_of(m.order, self.buy), summary.free_margin)),
        };
        let fallback = match mode {
            SizeMode::Lots => 0.1,
            SizeMode::Units => contract.lot_size as f64 / 1_000.0,
            SizeMode::RiskBalance | SizeMode::RiskEquity => 1.0,
            SizeMode::RiskMoney => nice(summary.balance / 100.0),
            SizeMode::FreeMargin => 10.0,
        };
        let value = value
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(fallback);
        self.size_mode = mode;
        let decimals = if mode == SizeMode::Units { 0 } else { 2 };
        self.write(
            &self.size.clone(),
            number::format(value, decimals),
            window,
            cx,
        );
        self.apply_steps(window, cx);
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Sets how far the steppers of the fields move, for their units.
    fn apply_steps(&self, window: &mut Window, cx: &mut Context<Self>) {
        let contract = self.contract(cx);
        number::set_kind(&self.size, size_kind(self.size_mode, &contract), window, cx);
        for (state, unit) in [
            (&self.stop_loss, self.stop_unit),
            (&self.take_profit, self.target_unit),
        ] {
            number::set_kind(state, offset_kind(unit, &contract), window, cx);
        }
    }

    fn contract(&self, cx: &App) -> Contract {
        self.symbol
            .as_ref()
            .map(|s| {
                let mut contract = self.account.read(cx).book.contract(s.id);
                contract.digits = s.digits;
                contract
            })
            .unwrap_or_default()
    }

    fn quote(&self, cx: &App) -> (Option<f64>, Option<f64>) {
        self.symbol
            .as_ref()
            .map_or((None, None), |s| self.account.read(cx).quote(s.id))
    }

    fn read(state: &Entity<InputState>, cx: &App) -> Option<f64> {
        number::parse(&state.read(cx).value())
    }

    /// The pips typed in the slippage field, when there are some.
    fn slippage_pips(&self, cx: &App) -> Option<f64> {
        Self::read(&self.slippage, cx).filter(|v| *v > 0.0)
    }

    /// When a good-till-date order would expire, in Unix milliseconds.
    fn expires_at(&self, cx: &App) -> Option<i64> {
        Self::read(&self.expiry, cx)
            .filter(|v| *v > 0.0)
            .map(|v| now_ms() + (v * self.expiry_span.millis() as f64) as i64)
    }

    /// The price the order would enter at: its own for a pending order, the market's for a
    /// market order.
    fn entry(&self, cx: &App) -> Option<f64> {
        match self.kind {
            Kind::Market => {
                let (bid, ask) = self.quote(cx);
                if self.buy { ask } else { bid }
            }
            Kind::Limit | Kind::Stop | Kind::StopLimit => Self::read(&self.price, cx),
        }
    }

    fn scale(&self, plan: &Plan, cx: &App) -> Scale {
        Scale {
            pip: self.contract(cx).pip(),
            money_per_price: plan.money_per_price,
            balance: self.account.read(cx).summary().balance,
            stop_distance: plan.stop_distance,
        }
    }

    /// The price of a protection given in `unit`, from the value of its field.
    fn protection_price(
        &self,
        stop: bool,
        unit: Offset,
        entry: Option<f64>,
        scale: &Scale,
        cx: &App,
    ) -> Option<f64> {
        let state = if stop {
            &self.stop_loss
        } else {
            &self.take_profit
        };
        let value = Self::read(state, cx)?;
        match unit {
            Offset::Price => Some(value),
            _ => {
                let distance = scale.distance(unit, value)?;
                Some(entry? + math::protection_side(self.buy, stop) * distance)
            }
        }
    }

    /// Works the order out from the fields: its volume, its protection and what is at stake.
    fn plan(&self, cx: &App) -> Plan {
        let account = self.account.read(cx);
        let summary = account.summary();
        let contract = self.contract(cx);
        let rate = self.symbol.as_ref().and_then(|s| account.rate(s.id));
        let entry = self.entry(cx);
        let size = Self::read(&self.size, cx).filter(|v| *v > 0.0);
        let side = |price: f64, stop: bool| {
            entry.map(|e| (price - e) * math::protection_side(self.buy, stop))
        };
        let mut scale = Scale {
            pip: contract.pip(),
            money_per_price: None,
            balance: summary.balance,
            stop_distance: None,
        };
        // A stop loss that does not depend on the volume, which a risk needs.
        let early_stop = if self.stop_on && self.stop_atr {
            entry
                .zip(
                    self.chart
                        .as_ref()
                        .and_then(|chart| chart.read(cx).atr_value(&self.atr)),
                )
                .and_then(|(entry, atr)| self.atr.stop(entry, self.buy, atr))
        } else {
            (self.stop_on && !self.stop_unit.needs_volume())
                .then(|| self.protection_price(true, self.stop_unit, entry, &scale, cx))
                .flatten()
        };
        let lot_units = contract.lot_size as f64 / 100.0;
        let sized: Result<Stepped, String> = (|| match self.size_mode {
            SizeMode::Lots => Ok(contract.volume_near(size.ok_or("Set the volume")?)),
            SizeMode::Units => Ok(contract.volume_near(size.ok_or("Set the volume")? / lot_units)),
            SizeMode::RiskBalance | SizeMode::RiskEquity | SizeMode::RiskMoney => {
                let value = size.ok_or("Set the risk")?;
                let money = match self.size_mode {
                    SizeMode::RiskBalance => summary.balance * value / 100.0,
                    SizeMode::RiskEquity => summary.equity * value / 100.0,
                    _ => value,
                };
                if !self.stop_on {
                    return Err("Turn the stop loss on to size by risk".to_owned());
                }
                let stop = early_stop.ok_or("Set the stop loss")?;
                let distance =
                    side(stop, true).ok_or_else(|| math::TicketProblem::NoPrice.to_string())?;
                if distance <= 0.0 {
                    return Err(math::TicketProblem::StopLossWrongSide.to_string());
                }
                let rate = rate.ok_or("Waiting for the conversion rate")?;
                let lots =
                    math::lots_for_risk(money, distance, rate, &contract).ok_or("Set the risk")?;
                Ok(contract.volume_at_most(lots))
            }
            SizeMode::FreeMargin => {
                let value = size.ok_or("Set the share of the free margin")?;
                let lot = self
                    .margin
                    .map(|m| side_of(m.lot, self.buy))
                    .filter(|m| *m > 0.0)
                    .ok_or("Waiting for the margin")?;
                Ok(contract.volume_at_most(summary.free_margin.max(0.0) * value / 100.0 / lot))
            }
        })();
        let mut plan = Plan {
            entry,
            rate,
            ..Plan::default()
        };
        let problem = |plan: &mut Plan, text: String| {
            plan.problem.get_or_insert(text);
        };
        match sized {
            Ok(sized) => plan.sized = Some(sized),
            Err(text) => problem(&mut plan, text),
        }
        plan.money_per_price = plan
            .sized
            .zip(rate)
            .map(|(s, r)| s.volume as f64 / 100.0 * r);
        scale.money_per_price = plan.money_per_price;
        if self.stop_on {
            plan.stop = if self.stop_atr {
                early_stop
            } else {
                early_stop
                    .or_else(|| self.protection_price(true, self.stop_unit, entry, &scale, cx))
            };
        }
        plan.stop_distance = plan.stop.and_then(|p| side(p, true)).filter(|d| *d > 0.0);
        scale.stop_distance = plan.stop_distance;
        if self.target_on {
            plan.target = self.protection_price(false, self.target_unit, entry, &scale, cx);
        }
        let target_distance = plan
            .target
            .and_then(|p| side(p, false))
            .filter(|d| *d > 0.0);
        plan.risk = plan
            .stop_distance
            .zip(plan.money_per_price)
            .map(|(d, m)| d * m);
        plan.reward = target_distance
            .zip(plan.money_per_price)
            .map(|(d, m)| d * m);
        if self.exits.on {
            // The exits have their own take profits, in multiples of the risk: the single take
            // profit is not used, and the reward is the sum of what each exit earns.
            let exits = self.exits_now(cx);
            plan.target = None;
            plan.reward = plan.risk.map(|risk| {
                exits
                    .legs
                    .iter()
                    .map(|leg| leg.share / 100.0 * leg.target_r * risk)
                    .sum()
            });
            if !self.stop_on || plan.stop_distance.is_none() {
                problem(
                    &mut plan,
                    "Exits need a stop loss: their targets are multiples of the risk".to_owned(),
                );
            }
            if let Some(text) = exits.problem() {
                problem(&mut plan, text);
            }
            if exits.oco_pips > 0.0 && !matches!(self.kind, Kind::Limit | Kind::Stop) {
                problem(
                    &mut plan,
                    "An OCO pair needs a limit or stop order".to_owned(),
                );
            }
        }

        if entry.is_none() {
            problem(&mut plan, math::TicketProblem::NoPrice.to_string());
        }
        if self.stop_on && plan.stop.is_none() {
            let text = if self.stop_atr {
                "ATR unavailable for the selected timeframe and bar"
            } else if self.stop_unit.needs_volume() && rate.is_none() {
                "Waiting for the conversion rate"
            } else {
                "Set the stop loss"
            };
            problem(&mut plan, text.to_owned());
        }
        if self.target_on && !self.exits.on && plan.target.is_none() {
            let text = if self.target_unit == Offset::Ratio && plan.stop_distance.is_none() {
                "A take profit in R needs a stop loss"
            } else if self.target_unit.needs_volume() && rate.is_none() {
                "Waiting for the conversion rate"
            } else {
                "Set the take profit"
            };
            problem(&mut plan, text.to_owned());
        }
        if let Some(entry) = entry
            && let Err(wrong) = math::check_protection(self.buy, entry, plan.stop, plan.target)
        {
            problem(&mut plan, wrong.to_string());
        }
        if self.kind.is_pending() && self.tif == Tif::GoodTillDate && self.expires_at(cx).is_none()
        {
            problem(&mut plan, "Set when the order expires".to_owned());
        }
        if self.kind == Kind::StopLimit && self.slippage_pips(cx).is_none() {
            problem(&mut plan, "Set the range of the limit".to_owned());
        }
        if self.kind == Kind::Market && self.slippage_on && self.slippage_pips(cx).is_none() {
            problem(&mut plan, "Set the largest slippage".to_owned());
        }
        if self.symbol.is_none() {
            plan.problem = Some("Pick a symbol first".to_owned());
        }
        plan
    }

    /// The lines the ticket shows on the chart of its symbol.
    pub fn lines(&self, cx: &App) -> Vec<ChartLine> {
        // A followed drawing already shows the entry and the levels.
        if self.symbol.is_none() || self.link.is_some() {
            return Vec::new();
        }
        let plan = self.plan(cx);
        let palette_line = crate::ui::kit::theme::colors().line;
        let currency = self.account.read(cx).book.currency.clone();
        let mut lines = Vec::new();
        let line = |id: u8, price: f64, color: u32, label: &str, money: Option<f64>| ChartLine {
            id: LineId::Pending(id),
            price,
            color,
            label: label.to_owned(),
            detail: money.map(|m| (math::format_money(m, &currency), color)),
            dash: Dash::Dashed,
            draggable: true,
            closable: false,
        };
        if self.kind.is_pending()
            && let Some(price) = Self::read(&self.price, cx)
        {
            let label = match (self.buy, self.kind) {
                (true, Kind::Limit) => "Buy limit",
                (true, Kind::StopLimit) => "Buy stop limit",
                (true, _) => "Buy stop",
                (false, Kind::Limit) => "Sell limit",
                (false, Kind::StopLimit) => "Sell stop limit",
                (false, _) => "Sell stop",
            };
            lines.push(line(LINE_ENTRY, price, palette_line, label, None));
        }
        let palette = crate::ui::kit::theme::colors();
        if let Some(price) = plan.stop {
            lines.push(line(
                LINE_STOP,
                price,
                palette.down,
                "SL",
                plan.risk.map(|r| -r),
            ));
        }
        if self.exits.on {
            // One take profit line per exit that has one, each dragged to change its R.
            let exits = self.exits_now(cx);
            if let (Some(entry), Some(distance)) = (plan.entry, plan.stop_distance) {
                for (index, leg) in exits.legs.iter().enumerate() {
                    if leg.target_r <= 0.0 {
                        continue;
                    }
                    let price =
                        entry + math::protection_side(self.buy, false) * leg.target_r * distance;
                    let money = plan.risk.map(|r| r * leg.target_r * leg.share / 100.0);
                    lines.push(line(
                        LINE_TARGET + index as u8,
                        price,
                        palette.up,
                        &format!("TP{}", index + 1),
                        money,
                    ));
                }
            }
        } else if let Some(price) = plan.target {
            lines.push(line(LINE_TARGET, price, palette.up, "TP", plan.reward));
        }
        lines
    }

    /// Asks the server the margin of one lot and of the order's volume, a moment after the
    /// volume last changed.
    fn request_margin(&mut self, symbol: i64, volume: i64, cx: &mut Context<Self>) {
        self.margin_asked = Some((symbol, volume));
        self.margin_epoch += 1;
        let epoch = self.margin_epoch;
        let lot = self.contract(cx).lot_size;
        let digits = self.account.read(cx).book.money_digits();
        let session = self.account.read(cx).session();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let current = this
                .update(cx, |this, _| this.margin_epoch == epoch)
                .unwrap_or(false);
            if !current {
                return;
            }
            let answer = runtime::spawn(async move {
                let client = session.client().ok_or(crate::app::broker::Error::Closed)?;
                client
                    .account(session.account_id())
                    .margin()
                    .expected_margin(symbol, &[lot, volume])
                    .await
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.margin_epoch != epoch {
                    return;
                }
                let money = |m: &crate::domain::trading::ExpectedMargin| {
                    (
                        crate::domain::trading::money(m.buy_margin, digits),
                        crate::domain::trading::money(m.sell_margin, digits),
                    )
                };
                match answer.ok().and_then(Result::ok) {
                    Some(margins) => {
                        let of = |v: i64| margins.iter().find(|m| m.volume == v).map(money);
                        this.margin = of(lot).zip(of(volume)).map(|(lot, order)| Margin {
                            volume,
                            lot,
                            order,
                        });
                    }
                    // Asked again when the volume changes.
                    None => this.margin_asked = None,
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The order as it would be sent, or why it cannot be.
    fn order(&self, plan: &Plan, cx: &App) -> Result<NewOrderReq, String> {
        self.order_with(self.buy, self.kind, plan, cx)
    }

    /// The order of a side and a kind, from a plan: the ticket's own, or its opposite in an OCO
    /// pair.
    fn order_with(
        &self,
        buy: bool,
        kind: Kind,
        plan: &Plan,
        cx: &App,
    ) -> Result<NewOrderReq, String> {
        if let Some(problem) = &plan.problem {
            return Err(problem.clone());
        }
        let symbol = self.symbol.as_ref().ok_or("Pick a symbol first")?;
        let (Some(entry), Some(sized)) = (plan.entry, plan.sized) else {
            return Err(math::TicketProblem::NoPrice.to_string());
        };
        let contract = self.contract(cx);
        let side = if buy { TradeSide::Buy } else { TradeSide::Sell };
        let round = |p: f64| contract.round_price(p);
        let volume = sized.volume;
        let pips = self.slippage_pips(cx);
        let mut order = match kind {
            Kind::Market if self.slippage_on => {
                // A market range order takes absolute protection, and fills within the slippage.
                let mut order = NewOrderReq::market(symbol.id, side, volume)
                    .with_protection(plan.stop.map(round), plan.target.map(round));
                order.order_type = NewOrderType::MarketRange.number();
                order.base_slippage_price = Some(round(entry));
                order.slippage_in_points = pips.map(|p| slippage_points(p, &contract));
                order
            }
            Kind::Market => {
                let mut order = NewOrderReq::market(symbol.id, side, volume);
                // A market order's protection is a distance from where it fills.
                order.relative_stop_loss = plan
                    .stop
                    .map(|sl| math::relative_distance(entry - sl, contract.digits));
                order.relative_take_profit = plan
                    .target
                    .map(|tp| math::relative_distance(tp - entry, contract.digits));
                order
            }
            Kind::Limit => NewOrderReq::limit(symbol.id, side, volume, round(entry))
                .with_protection(plan.stop.map(round), plan.target.map(round)),
            Kind::Stop => NewOrderReq::stop(symbol.id, side, volume, round(entry))
                .with_protection(plan.stop.map(round), plan.target.map(round)),
            Kind::StopLimit => {
                let range = pips.ok_or("Set the range of the limit")?;
                let mut order = NewOrderReq::stop_limit(
                    symbol.id,
                    side,
                    volume,
                    round(entry),
                    round(stop_limit_price(buy, entry, range * contract.pip())),
                )
                .with_protection(plan.stop.map(round), plan.target.map(round));
                order.slippage_in_points = Some(slippage_points(range, &contract));
                order
            }
        };
        if kind.is_pending() {
            match self.tif {
                Tif::GoodTillCancel => order.time_in_force = Some(2),
                Tif::GoodTillDate => {
                    order.time_in_force = Some(1);
                    order.expiration_timestamp = self.expires_at(cx);
                }
            }
        }
        if plan.stop.is_some() {
            order.trailing_stop_loss = self.trailing.then_some(true);
            order.guaranteed_stop_loss = self.guaranteed.then_some(true);
        }
        let comment = self.comment.read(cx).value().trim().to_owned();
        if !comment.is_empty() {
            order.comment = Some(comment.chars().take(512).collect());
        }
        Ok(order)
    }

    /// Every order the ticket sends: the one order, or the legs of a plan of exits, and their
    /// opposites when an OCO pair is asked for. The orders of a plan carry a label that says
    /// which plan and leg they are (see [`plan::Label`]).
    fn orders(&self, plan: &Plan, cx: &App) -> Result<Vec<NewOrderReq>, String> {
        let time_stop = self.time_stop_now(cx).rule();
        if !self.exits.on {
            let mut order = self.order(plan, cx)?;
            // A time stop needs the label to travel with the position.
            if time_stop.is_some() {
                order.label = Some(
                    plan::Label {
                        group: plan::new_group(now_ms()),
                        leg: 1,
                        of: 1,
                        break_even: None,
                        oco: false,
                        time_stop,
                    }
                    .encode(),
                );
            }
            return Ok(vec![order]);
        }
        if let Some(problem) = &plan.problem {
            return Err(problem.clone());
        }
        let exits = self.exits_now(cx);
        let (Some(entry), Some(sized), Some(stop), Some(distance)) =
            (plan.entry, plan.sized, plan.stop, plan.stop_distance)
        else {
            return Err("Exits need a stop loss".to_owned());
        };
        let contract = self.contract(cx);
        let shares: Vec<f64> = exits.legs.iter().map(|l| l.share).collect();
        let (volumes, kept) = plan::split_volume(
            sized.volume,
            &shares,
            contract.min_volume,
            contract.step_volume,
        );
        if volumes.is_empty() {
            return Err("The volume is too small to cut into exits".to_owned());
        }
        // The break-even leg counted among the legs that are left.
        let break_even = (exits.break_even.on && kept.len() > 1).then(|| {
            let want = usize::from(exits.break_even.after_leg).saturating_sub(1);
            let at = kept.iter().rposition(|k| *k <= want).unwrap_or(0);
            let after = (at + 1).min(kept.len() - 1).max(1);
            (
                after as u8,
                (exits.break_even.offset_pips * 10.0).round() as u32,
            )
        });
        let group = plan::new_group(now_ms());
        let pip = contract.pip();
        // The sides: the ticket's own, and the opposite one of an OCO pair.
        let mut sides = vec![(self.buy, entry, stop)];
        let oco = exits.oco_pips > 0.0 && matches!(self.kind, Kind::Limit | Kind::Stop);
        if oco {
            // A stop order of the other side waits beyond the price the other way; a limit order
            // waits on the far side of it.
            let toward = if (self.kind == Kind::Stop) == self.buy {
                -1.0
            } else {
                1.0
            };
            let other = entry + toward * exits.oco_pips * pip;
            if other <= 0.0 {
                return Err("The opposite price of the OCO pair is not a price".to_owned());
            }
            let other_stop = other + math::protection_side(!self.buy, true) * distance;
            sides.push((!self.buy, other, other_stop));
        }
        let mut orders = Vec::new();
        for (buy, entry, stop) in sides {
            for (n, (volume, leg)) in volumes.iter().zip(&kept).enumerate() {
                let target_r = exits.legs[*leg].target_r;
                let mut leg_plan = plan.clone();
                leg_plan.entry = Some(entry);
                leg_plan.stop = Some(stop);
                leg_plan.target = (target_r > 0.0)
                    .then(|| entry + math::protection_side(buy, false) * target_r * distance);
                let mut sized = sized;
                sized.volume = *volume;
                leg_plan.sized = Some(sized);
                let mut order = self.order_with(buy, self.kind, &leg_plan, cx)?;
                order.trailing_stop_loss =
                    (exits.trail_last && n + 1 == kept.len()).then_some(true);
                order.label = Some(
                    plan::Label {
                        group: group.clone(),
                        leg: (n + 1) as u8,
                        of: kept.len() as u8,
                        break_even,
                        oco,
                        time_stop,
                    }
                    .encode(),
                );
                orders.push(order);
            }
        }
        Ok(orders)
    }

    fn describe(&self, plan: &Plan, cx: &App) -> String {
        let name = self
            .symbol
            .as_ref()
            .map_or_else(String::new, |s| s.name.to_string());
        let side = if self.buy { "Buy" } else { "Sell" };
        let contract = self.contract(cx);
        // The volume, when it is known, then the symbol.
        let what = match plan.sized {
            Some(s) => format!(
                "{} {name}",
                math::format_lots(contract.lots_of_volume(s.volume))
            ),
            None => name,
        };
        let price = || {
            plan.entry
                .map(|p| contract.format_price(p))
                .unwrap_or_default()
        };
        match self.kind {
            Kind::Market if self.slippage_on => format!("{side} {what} at market range"),
            Kind::Market => format!("{side} {what} at market"),
            Kind::Limit => format!("{side} {what} limit at {}", price()),
            Kind::Stop => format!("{side} {what} stop at {}", price()),
            Kind::StopLimit => format!("{side} {what} stop limit at {}", price()),
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let plan = self.plan(cx);
        let orders = match self.orders(&plan, cx) {
            Ok(orders) => orders,
            Err(message) => {
                crate::ui::kit::toast::Toast::warning("The order is not ready", message)
                    .hint("Fix it in the ticket, then send again.")
                    .sticky(false)
                    .show(cx);
                return;
            }
        };
        // The safety limits first: a hard one refuses the order here, before anything opens.
        let (verdict, live, balance) = {
            let account = self.account.read(cx);
            (
                account.assess_batch(&orders, self.one_click),
                account.is_live(),
                account.summary().balance,
            )
        };
        let mut warnings = match verdict {
            Verdict::Block(reason) => {
                crate::ui::kit::toast::Toast::warning("Blocked by your safety settings", reason)
                    .hint("Change the limits in Settings, Safety.")
                    .show(cx);
                return;
            }
            Verdict::Warn(warnings) => warnings,
            Verdict::Ok => Vec::new(),
        };
        if let Some(risk) = plan.risk
            && balance > 0.0
            && risk / balance * 100.0 > self.layout.high_risk
        {
            warnings.push(format!(
                "This order risks more than {}% of the balance.",
                number::format(self.layout.high_risk, 2)
            ));
        }
        // One click sends at once, unless something is worth a second look.
        if self.one_click && warnings.is_empty() {
            self.account
                .update(cx, |account, cx| account.place_batch(orders, true, cx));
            // The order is on its way: the drawing has done its part.
            self.release_link(true, window, cx);
            return;
        }
        let account = self.account.clone();
        let this = cx.entity();
        let one_click = self.one_click;
        let text = self.describe(&plan, cx);
        let details = self.confirm_details(&plan, live, warnings, cx);
        confirm_details(
            window,
            cx,
            "Send this order?",
            text,
            details,
            move |window, cx| {
                account.update(cx, |account, cx| {
                    account.place_batch(orders.clone(), one_click, cx)
                });
                this.update(cx, |ticket, cx| ticket.release_link(true, window, cx));
            },
        );
    }

    /// The rows the confirmation shows for an order: what it is, and what is at stake.
    fn confirm_details(&self, plan: &Plan, live: bool, warnings: Vec<String>, cx: &App) -> Details {
        let contract = self.contract(cx);
        let currency = self.account.read(cx).book.currency.clone();
        let money = |amount: f64| math::format_money(amount, &currency);
        let price = |value: Option<f64>| {
            value.map_or_else(|| "none".to_owned(), |p| contract.format_price(p))
        };
        let mut rows: Vec<(SharedString, SharedString)> = Vec::new();
        let mut row = |label: &'static str, value: String| rows.push((label.into(), value.into()));
        if let Some(sized) = plan.sized {
            row(
                "Size",
                format!(
                    "{} lots",
                    math::format_lots(contract.lots_of_volume(sized.volume))
                ),
            );
        }
        row(
            if self.kind == Kind::Market {
                "Price (market)"
            } else {
                "Price"
            },
            price(plan.entry),
        );
        row("Stop loss", price(plan.stop));
        row("Take profit", price(plan.target));
        if let Some(risk) = plan.risk {
            let balance = self.account.read(cx).summary().balance;
            let share = if balance > 0.0 {
                format!(" ({}%)", number::format(risk / balance * 100.0, 2))
            } else {
                String::new()
            };
            row("Risk", format!("{}{share}", money(risk)));
        }
        if let Some(reward) = plan.reward {
            row("Reward", money(reward));
            if let Some(risk) = plan.risk.filter(|r| *r > 0.0) {
                row(
                    "Reward to risk",
                    format!("{}R", number::format(reward / risk, 2)),
                );
            }
        }
        if let Some(margin) = self
            .margin
            .filter(|m| Some(m.volume) == plan.sized.map(|s| s.volume))
            .map(|m| side_of(m.order, self.buy))
        {
            row("Margin", money(margin));
        }
        let (bid, ask) = self.quote(cx);
        if let Some((bid, ask)) = bid.zip(ask) {
            row("Spread", format!("{:.1} pips", contract.pips(ask - bid)));
        }
        if let Some(rule) = self.time_stop_now(cx).rule() {
            row("Time stop", format!("Close {}", rule.describe()));
        }
        Details {
            rows,
            warnings: warnings.into_iter().map(Into::into).collect(),
            badge: live.then(|| "LIVE ACCOUNT".into()),
            label: Some(if self.buy { "Buy" } else { "Sell" }),
        }
    }
}

/// The margin of a side: the buy one or the sell one.
fn side_of(margins: (f64, f64), buy: bool) -> f64 {
    if buy { margins.0 } else { margins.1 }
}

/// A distance in pips as the points the server counts: the smallest step of the price.
fn slippage_points(pips: f64, contract: &Contract) -> i32 {
    let digits = i32::try_from(contract.digits).unwrap_or(5);
    (pips * contract.pip() * 10f64.powi(digits))
        .round()
        .max(0.0) as i32
}

/// Where the limit of a stop limit sits: past the stop by the range, in the direction the order
/// trades.
fn stop_limit_price(buy: bool, stop: f64, range: f64) -> f64 {
    if buy { stop + range } else { stop - range }
}

/// What the size field holds in a mode.
fn size_kind(mode: SizeMode, contract: &Contract) -> number::Kind {
    match mode {
        SizeMode::Lots => number::Kind::Volume {
            step: contract.lots_of_volume(contract.step_volume.max(1)),
        },
        SizeMode::Units => number::Kind::Volume {
            step: contract.step_volume.max(1) as f64 / 100.0,
        },
        SizeMode::RiskBalance | SizeMode::RiskEquity | SizeMode::FreeMargin => {
            number::Kind::Percent
        }
        SizeMode::RiskMoney => number::Kind::Money,
    }
}

/// What a protection field holds in its unit.
fn offset_kind(unit: Offset, contract: &Contract) -> number::Kind {
    match unit {
        Offset::Price => number::Kind::Price {
            step: contract.pip(),
        },
        Offset::Pips => number::Kind::Pips,
        Offset::Money => number::Kind::Money,
        Offset::Percent => number::Kind::Percent,
        Offset::Ratio => number::Kind::Ratio,
    }
}

/// A value of a protection written for its unit.
fn format_offset(unit: Offset, value: f64) -> String {
    let decimals = match unit {
        Offset::Pips => 1,
        _ => 2,
    };
    number::format(value, decimals)
}

/// An amount rounded to two significant digits, for a preset: 97.3 is 97, 1 234 is 1 200.
fn nice(amount: f64) -> f64 {
    if !amount.is_finite() || amount <= 0.0 {
        return 10.0;
    }
    let magnitude = 10f64.powi(amount.log10().floor() as i32 - 1);
    ((amount / magnitude).round() * magnitude).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_rounded_to_two_digits() {
        assert_eq!(nice(97.3), 97.0);
        assert_eq!(nice(1_234.0), 1_200.0);
        assert_eq!(nice(25.0), 25.0);
        assert_eq!(nice(0.0), 10.0);
        assert_eq!(format_offset(Offset::Pips, 12.345), "12.3");
        assert_eq!(format_offset(Offset::Money, 100.0), "100");
    }

    #[test]
    fn a_slippage_in_pips_is_written_in_points() {
        // A five digit pair: a pip is ten points.
        let pair = Contract::default();
        assert_eq!(slippage_points(2.0, &pair), 20);
        assert_eq!(slippage_points(0.5, &pair), 5);
        // A three digit yen pair, the pip at the second decimal.
        let yen = Contract {
            digits: 3,
            pip_position: 2,
            ..Contract::default()
        };
        assert_eq!(slippage_points(3.0, &yen), 30);
        assert_eq!(slippage_points(-1.0, &yen), 0);
    }

    fn plan(atr: Option<AtrStop>, rr: Option<f64>, stop: f64, target: f64) -> PositionPlan {
        PositionPlan {
            drawing: 7,
            buy: true,
            entry: 100.0,
            stop_loss: stop,
            take_profit: target,
            atr,
            rr,
        }
    }

    #[test]
    fn a_drawing_asks_for_a_change_only_when_its_own_values_move() {
        let fixed = plan(None, None, 98.0, 104.0);
        assert!(same_shape(&fixed, &fixed.clone()));
        assert!(
            !same_shape(&fixed, &plan(None, None, 97.0, 104.0)),
            "a dragged stop"
        );
        assert!(
            !same_shape(&fixed, &plan(None, None, 98.0, 105.0)),
            "a dragged target"
        );
        let mut flipped = fixed.clone();
        flipped.buy = false;
        assert!(!same_shape(&fixed, &flipped));
        let mut moved = fixed.clone();
        moved.entry = 101.0;
        assert!(!same_shape(&fixed, &moved));
    }

    #[test]
    fn the_levels_the_ticket_works_out_are_not_read_as_moves() {
        // The stop of an ATR stop changes with every bar, the target of a multiple of the
        // risk with the stop: the ticket works both out, so neither is a change to write.
        let atr = Some(AtrStop::default());
        let before = plan(atr.clone(), Some(2.0), 98.0, 104.0);
        let after = plan(atr.clone(), Some(2.0), 97.5, 105.0);
        assert!(same_shape(&before, &after));
        // The settings of the ATR and the multiple are.
        let wider = AtrStop {
            multiplier: 3.0,
            ..AtrStop::default()
        };
        assert!(!same_shape(
            &before,
            &plan(Some(wider), Some(2.0), 98.0, 104.0)
        ));
        assert!(!same_shape(
            &before,
            &plan(atr.clone(), Some(3.0), 98.0, 104.0)
        ));
        // An ATR stop with a fixed target: the target still counts.
        let half = plan(atr.clone(), None, 98.0, 104.0);
        assert!(same_shape(&half, &plan(atr.clone(), None, 97.0, 104.0)));
        assert!(!same_shape(&half, &plan(atr, None, 98.0, 106.0)));
    }

    #[test]
    fn the_limit_of_a_stop_limit_is_past_the_stop() {
        assert!((stop_limit_price(true, 1.1000, 0.0003) - 1.1003).abs() < 1e-12);
        assert!((stop_limit_price(false, 1.1000, 0.0003) - 1.0997).abs() < 1e-12);
    }
}
