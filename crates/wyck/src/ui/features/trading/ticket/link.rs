//! Following a position or a drawing of the chart: the ticket takes its levels and gives them back.

use super::helpers::format_offset;
use super::{
    Before, Kind, LINE_ENTRY, LINE_STOP, LINE_TARGET, Link, OrderTicket, TicketEvent, same_shape,
};
use crate::domain::trading::math;
use crate::domain::trading::math::{Offset, Pending};
use crate::ui::features::chart::events::{PlanState, PositionPlan};
use crate::ui::kit::input::InputState;
use crate::ui::kit::number;
use gpui::{App, Context, Entity, Window};

impl OrderTicket {
    /// Starts following a position drawing: the ticket takes its side, its entry and its
    /// protection, set the way the drawing sets them, and keeps up with it until the order is
    /// sent or the link is cut.
    pub(super) fn begin_link(
        &mut self,
        plan: PositionPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn apply_link(
        &mut self,
        plan: &PositionPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn refresh_kind(&mut self, cx: &App) {
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
    pub(super) fn sync_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn release_link(
        &mut self,
        restore: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn unlink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn write(
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
    pub(super) fn set_protection(
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
}
