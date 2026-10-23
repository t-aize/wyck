//! What a click in the ticket changes: the side, the kind, the protections, the units and the size.

use super::helpers::{nice, offset_kind, side_of, size_kind};
use super::{Kind, OrderTicket, TicketEvent};
use crate::kit::number;
use gpui::{Context, Window};
use wyck_app::account::Busy;
use wyck_core::math::{f64_to_usize, i64_to_f64};
use wyck_trading::ticket;
use wyck_trading::ticket::{Offset, SizeMode};

impl OrderTicket {
    /// Whether a side can be picked: a followed drawing is a long or a short, not both.
    pub(super) fn side_allowed(&self, buy: bool) -> bool {
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

    pub(super) fn set_side(&mut self, buy: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.side_allowed(buy) {
            return;
        }
        self.buy = buy;
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Picks the kind of order. A pending one starts at the market price of its side, or at the
    /// entry of the drawing that is followed.
    pub(super) fn set_kind(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn price_at_market(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (bid, ask) = self.quote(cx);
        if let Some(price) = if self.buy { ask } else { bid } {
            let value = self.contract(cx).format_price(price);
            Self::write(&self.price.clone(), value, window, cx);
        }
    }

    /// Turns a stop loss or take profit on or off. One turned on empty starts at the distance of
    /// the defaults: some pips from the entry for a stop loss, and a multiple of the risk (or of
    /// those pips) for a take profit.
    pub(super) fn toggle_protection(
        &mut self,
        stop: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn fill_protection(
        &mut self,
        stop: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            let price = entry + ticket::protection_side(self.buy, stop) * distance;
            self.set_protection(stop, price, window, cx);
        }
    }

    /// Starts the protections the defaults ask for, once a price is known.
    pub(super) fn apply_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn set_unit(
        &mut self,
        stop: bool,
        unit: Offset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        if let Some(price) = price {
            self.set_protection(stop, price, window, cx);
        } else {
            let state = if stop {
                self.stop_loss.clone()
            } else {
                self.take_profit.clone()
            };
            Self::write(&state, String::new(), window, cx);
        }
        self.apply_steps(window, cx);
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    pub(super) fn set_atr_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                .find(|study| study.kind == wyck_indicators::kind::StudyKind::Atr)
        {
            self.atr.length = f64_to_usize(study.input("length")).clamp(1, 1_000);
            self.atr.smoothing = match f64_to_usize(study.input("smoothing")) {
                1 => wyck_trading::atr_stop::Smoothing::Sma,
                2 => wyck_trading::atr_stop::Smoothing::Ema,
                3 => wyck_trading::atr_stop::Smoothing::Wma,
                _ => wyck_trading::atr_stop::Smoothing::Rma,
            };
            Self::write(
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
            Self::write(&self.take_profit.clone(), "2".to_owned(), window, cx);
        }
        self.request_atr(cx);
        self.settings_changed(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Gives the take profit as a multiple of the risk, turning the stop loss on if it is off
    /// (a multiple of nothing is no price).
    pub(super) fn set_target_ratio(
        &mut self,
        ratio: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.link.is_some() {
            return;
        }
        if !self.stop_on {
            self.toggle_protection(true, window, cx);
        }
        self.target_on = true;
        self.target_unit = Offset::Ratio;
        Self::write(
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

    /// Turns on or off the following of the ATR by the stop of a pending order.
    pub(super) fn set_follow_atr(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.follow_atr != on {
            self.follow_atr = on;
            self.settings_changed(cx);
            cx.notify();
        }
    }

    /// Sets how many ATRs away the ATR stop sits.
    pub(super) fn set_atr_multiplier(
        &mut self,
        multiplier: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.link.is_some() {
            return;
        }
        Self::write(
            &self.atr_multiplier.clone(),
            number::format(multiplier, 2),
            window,
            cx,
        );
    }

    /// Sizes the volume another way, keeping the volume where it can.
    pub(super) fn set_size_mode(
        &mut self,
        mode: SizeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            SizeMode::Units => volume.map(|v| i64_to_f64(v) / 100.0),
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
            SizeMode::Units => i64_to_f64(contract.lot_size) / 1_000.0,
            SizeMode::RiskBalance | SizeMode::RiskEquity => 1.0,
            SizeMode::RiskMoney => nice(summary.balance / 100.0),
            SizeMode::FreeMargin => 10.0,
        };
        let value = value
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(fallback);
        self.size_mode = mode;
        let decimals = if mode == SizeMode::Units { 0 } else { 2 };
        Self::write(
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
    pub(super) fn apply_steps(&self, window: &mut Window, cx: &mut Context<Self>) {
        let contract = self.contract(cx);
        number::set_kind(&self.size, size_kind(self.size_mode, &contract), window, cx);
        for (state, unit) in [
            (&self.stop_loss, self.stop_unit),
            (&self.take_profit, self.target_unit),
        ] {
            number::set_kind(state, offset_kind(unit, &contract), window, cx);
        }
    }
}
