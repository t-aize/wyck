//! The order ticket: the panel beside the charts where an order is put together and sent.
//!
//! It shows the live bid and ask of the symbol with the spread, the side, the kind of order
//! (market, limit, stop) and the price of a pending order.
//!
//! The volume is typed in lots or units, or worked out from what the order may lose at its stop
//! loss (a share of the balance or of the equity, or an amount) or from a share of the free
//! margin. The stop loss and the take profit are given as a price or as a distance from the
//! entry in pips, in money or in percent of the balance, and the take profit also in multiples
//! of the risk. What the order risks and may gain, its risk to reward, its margin and what a pip
//! is worth are shown as the fields change. Money is converted into the deposit currency at live
//! prices, through the symbols the server names for it.
//!
//! While a pending price or a protection is set, it shows on the chart as a line that can be
//! dragged; the ticket follows the line (see [`OrderTicket::lines`]).
//!
//! An order is sent after a confirmation, unless one-click trading is on.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{App, Context, Entity, EventEmitter, SharedString, Subscription, Window, div, px};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, InputState, NumberStep};
use serde::{Deserialize, Serialize};
use wyck::openapi::account::TradeSide;
use wyck::openapi::trading::NewOrderReq;

use super::account::Account;
use super::math::{self, Contract, Offset, Pending, Scale, SizeMode, Stepped};
use crate::app::chart::drawing::model::Dash;
use crate::app::chart::{ChartLine, LineId};
use crate::app::multichart::SymbolRef;
use crate::app::{runtime, theme, widgets};

mod view;

/// The kind of order the ticket sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Market,
    Limit,
    Stop,
}

/// Which line of the ticket a pending line on the chart stands for.
pub const LINE_ENTRY: u8 = 0;
pub const LINE_STOP: u8 = 1;
pub const LINE_TARGET: u8 = 2;

/// A share of the balance above which the ticket warns about the risk.
const HIGH_RISK: f64 = 5.0;

pub enum TicketEvent {
    /// The ticket's lines on the chart changed.
    LinesChanged,
    /// How the ticket sizes orders changed, to be remembered.
    Settings(TicketPrefs),
    /// The user closed the ticket.
    Close,
}

/// How the ticket sizes orders, kept between runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TicketPrefs {
    pub size_mode: SizeMode,
    /// The last value typed for the volume, in the unit of `size_mode`.
    pub size: f64,
    pub stop_unit: Offset,
    pub target_unit: Offset,
}

impl Default for TicketPrefs {
    fn default() -> Self {
        Self {
            size_mode: SizeMode::Lots,
            size: 0.1,
            stop_unit: Offset::Pips,
            target_unit: Offset::Pips,
        }
    }
}

/// The margin the server gave for one lot and for the order's volume, for a buy and a sell.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Margin {
    volume: i64,
    lot: (f64, f64),
    order: (f64, f64),
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
    target_unit: Offset,
    stop_on: bool,
    target_on: bool,
    one_click: bool,
    margin: Option<Margin>,
    /// The symbol and volume the margin was last asked for.
    margin_asked: Option<(i64, i64)>,
    /// Bumped per margin request, so an old answer is ignored.
    margin_epoch: u64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TicketEvent> for OrderTicket {}

fn number(value: &str, window: &mut Window, cx: &mut Context<InputState>) -> InputState {
    InputState::new(window, cx)
        .default_value(value.to_owned())
        .min(0.0)
}

impl OrderTicket {
    pub fn new(
        account: Entity<Account>,
        prefs: TicketPrefs,
        one_click: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let size = cx.new(|cx| {
            number(&widgets::format_number(prefs.size, 2), window, cx)
                .step(step_of_size(prefs.size_mode, &Contract::default()))
        });
        let price = cx.new(|cx| InputState::new(window, cx));
        let stop_loss = cx.new(|cx| number("", window, cx));
        let take_profit = cx.new(|cx| number("", window, cx));
        let mut subscriptions = vec![cx.observe(&account, |_this, _account, cx| cx.notify())];
        subscriptions.push(cx.subscribe(&size, |this, _state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.settings_changed(cx);
                cx.emit(TicketEvent::LinesChanged);
                cx.notify();
            }
        }));
        for state in [&price, &stop_loss, &take_profit] {
            subscriptions.push(
                cx.subscribe(state, |_this, _state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    }
                }),
            );
        }
        let mut ticket = Self {
            account,
            symbol: None,
            buy: true,
            kind: Kind::Market,
            size_mode: prefs.size_mode,
            size,
            price,
            stop_loss,
            take_profit,
            stop_unit: prefs.stop_unit,
            target_unit: prefs.target_unit,
            stop_on: false,
            target_on: false,
            one_click,
            margin: None,
            margin_asked: None,
            margin_epoch: 0,
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

    /// How the ticket sizes orders now, to be remembered.
    pub fn prefs(&self, cx: &App) -> TicketPrefs {
        TicketPrefs {
            size_mode: self.size_mode,
            size: Self::read(&self.size, cx).unwrap_or(0.0),
            stop_unit: self.stop_unit,
            target_unit: self.target_unit,
        }
    }

    fn settings_changed(&self, cx: &mut Context<Self>) {
        let prefs = self.prefs(cx);
        cx.emit(TicketEvent::Settings(prefs));
    }

    /// The ticket is for this symbol now.
    pub fn set_symbol(
        &mut self,
        symbol: Option<SymbolRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.symbol.as_ref().map(|s| s.id) == symbol.as_ref().map(|s| s.id) {
            self.symbol = symbol;
            return;
        }
        self.symbol = symbol;
        let id = self.symbol.as_ref().map(|s| s.id);
        self.account.update(cx, |account, cx| account.focus(id, cx));
        self.kind = Kind::Market;
        self.stop_on = false;
        self.target_on = false;
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
            self.write(
                &self.size.clone(),
                widgets::format_number(value, 2),
                window,
                cx,
            );
        }
        self.apply_steps(window, cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Fills the ticket from the chart: a side, a price (a pending order, limit or stop by where
    /// it is), and protection.
    pub fn prefill(
        &mut self,
        buy: bool,
        entry: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buy = buy;
        match entry {
            Some(price) => {
                let (bid, ask) = self.quote(cx);
                self.kind = match math::pending_kind(buy, price, bid, ask) {
                    Pending::Limit => Kind::Limit,
                    Pending::Stop => Kind::Stop,
                };
                let value = self.contract(cx).format_price(price);
                self.write(&self.price.clone(), value, window, cx);
            }
            None => self.kind = Kind::Market,
        }
        self.stop_on = stop_loss.is_some();
        self.target_on = take_profit.is_some();
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

    /// A line of the ticket was dragged on the chart.
    pub fn line_moved(
        &mut self,
        line: u8,
        price: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match line {
            LINE_ENTRY => {
                let value = self.contract(cx).format_price(price);
                self.write(&self.price.clone(), value, window, cx);
            }
            LINE_STOP => self.set_protection(true, price, window, cx),
            _ => self.set_protection(false, price, window, cx),
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

    fn set_side(&mut self, buy: bool, _window: &mut Window, cx: &mut Context<Self>) {
        self.buy = buy;
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Turns a stop loss or take profit on or off. One turned on empty starts 20 pips from the
    /// entry for a stop loss, and twice the risk (or 40 pips) for a take profit.
    fn toggle_protection(&mut self, stop: bool, window: &mut Window, cx: &mut Context<Self>) {
        let on = if stop {
            self.stop_on = !self.stop_on;
            self.stop_on
        } else {
            self.target_on = !self.target_on;
            self.target_on
        };
        let state = if stop {
            self.stop_loss.clone()
        } else {
            self.take_profit.clone()
        };
        if on && Self::read(&state, cx).is_none() {
            let plan = self.plan(cx);
            let pip = self.contract(cx).pip();
            let distance = if stop {
                Some(20.0 * pip)
            } else {
                plan.stop_distance.map(|d| d * 2.0).or(Some(40.0 * pip))
            };
            if let (Some(entry), Some(distance)) = (plan.entry, distance) {
                let price = entry + math::protection_side(self.buy, stop) * distance;
                self.set_protection(stop, price, window, cx);
            }
        }
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Gives a protection in another unit, keeping its price.
    fn set_unit(&mut self, stop: bool, unit: Offset, window: &mut Window, cx: &mut Context<Self>) {
        let plan = self.plan(cx);
        let price = if stop { plan.stop } else { plan.target };
        if stop {
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
            widgets::format_number(value, decimals),
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
        let size = step_of_size(self.size_mode, &contract);
        self.size.update(cx, |s, cx| s.set_step(size, window, cx));
        for (state, unit) in [
            (&self.stop_loss, self.stop_unit),
            (&self.take_profit, self.target_unit),
        ] {
            let step = match unit {
                Offset::Price => contract.pip(),
                Offset::Pips => 1.0,
                Offset::Money => 10.0,
                Offset::Percent => 0.25,
                Offset::Ratio => 0.5,
            };
            state.update(cx, |s, cx| s.set_step(NumberStep::Fixed(step), window, cx));
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
        widgets::parse_number(&state.read(cx).value())
    }

    /// The price the order would enter at: its own for a pending order, the market's for a
    /// market order.
    fn entry(&self, cx: &App) -> Option<f64> {
        match self.kind {
            Kind::Market => {
                let (bid, ask) = self.quote(cx);
                if self.buy { ask } else { bid }
            }
            Kind::Limit | Kind::Stop => Self::read(&self.price, cx),
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
        let early_stop = (self.stop_on && !self.stop_unit.needs_volume())
            .then(|| self.protection_price(true, self.stop_unit, entry, &scale, cx))
            .flatten();
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
            plan.stop = early_stop
                .or_else(|| self.protection_price(true, self.stop_unit, entry, &scale, cx));
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

        if entry.is_none() {
            problem(&mut plan, math::TicketProblem::NoPrice.to_string());
        }
        if self.stop_on && plan.stop.is_none() {
            let text = if self.stop_unit.needs_volume() && rate.is_none() {
                "Waiting for the conversion rate"
            } else {
                "Set the stop loss"
            };
            problem(&mut plan, text.to_owned());
        }
        if self.target_on && plan.target.is_none() {
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
        if self.symbol.is_none() {
            plan.problem = Some("Pick a symbol first".to_owned());
        }
        plan
    }

    /// The lines the ticket shows on the chart of its symbol.
    pub fn lines(&self, cx: &App) -> Vec<ChartLine> {
        if self.symbol.is_none() {
            return Vec::new();
        }
        let plan = self.plan(cx);
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
        if self.kind != Kind::Market
            && let Some(price) = Self::read(&self.price, cx)
        {
            let label = match (self.buy, self.kind) {
                (true, Kind::Limit) => "Buy limit",
                (true, _) => "Buy stop",
                (false, Kind::Limit) => "Sell limit",
                (false, _) => "Sell stop",
            };
            lines.push(line(LINE_ENTRY, price, 0x5b8def, label, None));
        }
        if let Some(price) = plan.stop {
            lines.push(line(
                LINE_STOP,
                price,
                0xef5350,
                "SL",
                plan.risk.map(|r| -r),
            ));
        }
        if let Some(price) = plan.target {
            lines.push(line(LINE_TARGET, price, 0x26a69a, "TP", plan.reward));
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
                let client = session
                    .client()
                    .ok_or(wyck::openapi::OpenApiError::Closed)?;
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
                let money = |m: &wyck::openapi::margin::ExpectedMargin| {
                    (
                        wyck::openapi::account::money(m.buy_margin, digits),
                        wyck::openapi::account::money(m.sell_margin, digits),
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
        if let Some(problem) = &plan.problem {
            return Err(problem.clone());
        }
        let symbol = self.symbol.as_ref().ok_or("Pick a symbol first")?;
        let (Some(entry), Some(sized)) = (plan.entry, plan.sized) else {
            return Err(math::TicketProblem::NoPrice.to_string());
        };
        let contract = self.contract(cx);
        let side = if self.buy {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        };
        let round = |p: f64| contract.round_price(p);
        let volume = sized.volume;
        let mut order = match self.kind {
            Kind::Market => {
                let mut order = NewOrderReq::market(symbol.id, side, volume);
                // A market order's protection is a distance from where it fills.
                order.relative_stop_loss = plan.stop.map(|sl| math::relative_distance(entry - sl));
                order.relative_take_profit =
                    plan.target.map(|tp| math::relative_distance(tp - entry));
                order
            }
            Kind::Limit => NewOrderReq::limit(symbol.id, side, volume, round(entry))
                .with_protection(plan.stop.map(round), plan.target.map(round)),
            Kind::Stop => NewOrderReq::stop(symbol.id, side, volume, round(entry))
                .with_protection(plan.stop.map(round), plan.target.map(round)),
        };
        order.time_in_force = (self.kind != Kind::Market).then_some(2);
        Ok(order)
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
        match self.kind {
            Kind::Market => format!("{side} {what} at market"),
            Kind::Limit | Kind::Stop => format!(
                "{side} {what} {} at {}",
                if self.kind == Kind::Limit {
                    "limit"
                } else {
                    "stop"
                },
                plan.entry
                    .map(|p| contract.format_price(p))
                    .unwrap_or_default()
            ),
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let plan = self.plan(cx);
        let order = match self.order(&plan, cx) {
            Ok(order) => order,
            Err(message) => {
                crate::app::toast::show(
                    cx,
                    crate::app::toast::Kind::Warning,
                    "Order not sent",
                    message,
                );
                return;
            }
        };
        if self.one_click {
            self.account
                .update(cx, |account, cx| account.place(order, cx));
            return;
        }
        let account = self.account.clone();
        let mut text = self.describe(&plan, cx);
        let currency = self.account.read(cx).book.currency.clone();
        if let Some(risk) = plan.risk {
            text.push_str(&format!(
                ", risking {}",
                math::format_money(risk, &currency)
            ));
        }
        confirm(window, cx, "Send this order?", text, move |_window, cx| {
            account.update(cx, |account, cx| account.place(order.clone(), cx));
        });
    }
}

/// The margin of a side: the buy one or the sell one.
fn side_of(margins: (f64, f64), buy: bool) -> f64 {
    if buy { margins.0 } else { margins.1 }
}

/// How far the stepper of the volume moves in a mode.
fn step_of_size(mode: SizeMode, contract: &Contract) -> NumberStep {
    NumberStep::Fixed(match mode {
        SizeMode::Lots => contract.lots_of_volume(contract.step_volume.max(1)),
        SizeMode::Units => contract.step_volume.max(1) as f64 / 100.0,
        SizeMode::RiskBalance | SizeMode::RiskEquity => 0.25,
        SizeMode::RiskMoney => 10.0,
        SizeMode::FreeMargin => 5.0,
    })
}

/// A value of a protection written for its unit.
fn format_offset(unit: Offset, value: f64) -> String {
    let decimals = match unit {
        Offset::Pips => 1,
        _ => 2,
    };
    widgets::format_number(value, decimals)
}

/// An amount rounded to two significant digits, for a preset: 97.3 is 97, 1 234 is 1 200.
fn nice(amount: f64) -> f64 {
    if !amount.is_finite() || amount <= 0.0 {
        return 10.0;
    }
    let magnitude = 10f64.powi(amount.log10().floor() as i32 - 1);
    ((amount / magnitude).round() * magnitude).max(1.0)
}

/// Asks for a confirmation before `then` runs.
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    then: impl Fn(&mut Window, &mut App) + 'static,
) {
    let (title, text) = (title.into(), text.into());
    let then = std::rc::Rc::new(then);
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let then = then.clone();
        dialog
            .title(title.clone())
            .w(px(420.))
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(theme::fg())
                    .child(text.clone()),
            )
            .footer(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("confirm-cancel")
                            .cursor_pointer()
                            .ghost()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("confirm-ok")
                            .cursor_pointer()
                            .primary()
                            .label("Confirm")
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                then(window, cx);
                            }),
                    ),
            )
    });
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
}
