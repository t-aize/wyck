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

use crate::ui::kit::input::{InputEvent, InputState};
use gpui::prelude::*;
use gpui::{App, Context, Entity, EventEmitter, Subscription, Window};

use crate::app::account::Account;
use crate::domain::indicators::atr_stop::AtrStop;
use crate::domain::trading::math;
use crate::domain::trading::math::{Contract, Offset, Pending, SizeMode, Stepped, TicketProblem};
use crate::ui::features::chart::Chart;
use crate::ui::features::chart::{PositionLink, PositionPlan};
use crate::ui::features::multichart::SymbolRef;

pub mod customize;
pub use crate::app::prefs::ticket as prefs;
mod edit;
mod helpers;
mod link;
mod planning;
mod problem;
mod send;
mod templates;
mod view;

use self::helpers::{
    format_offset, nice, offset_kind, side_of, size_kind, slippage_points, stop_limit_price,
};

pub use self::prefs::{Kind, Layout, TicketPrefs};
use self::prefs::{Span, Tif};
use crate::domain::trading::plan;
use crate::domain::trading::plan::ExitPlan;
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
    problem: Option<TicketProblem>,
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
}

#[cfg(test)]
#[path = "../../../../tests/unit/ui/features/trading/ticket.rs"]
mod tests;
