//! Turning the plan into orders, asking for the go ahead and sending them.

use super::helpers::{side_of, slippage_points, stop_limit_price};
use super::{Kind, OrderTicket, Plan, Tif, problem};
use gpui::{App, Context, SharedString, Window};
use wyck_app::broker::trading::NewOrderReq;
use wyck_app::market_data::now_ms;
use wyck_trading::NewOrderType;
use wyck_trading::follow::AtrFollow;
use wyck_trading::guard::Verdict;
use wyck_trading::math;
use wyck_trading::math::TicketProblem;
use wyck_trading::plan;
use wyck_trading::{TimeInForce, TradeSide};
use wyck_ui_kit::confirm::{Details, confirm_details};
use wyck_ui_kit::number;

impl OrderTicket {
    /// The order as it would be sent, or why it cannot be.
    pub(super) fn order(&self, plan: &Plan, cx: &App) -> Result<NewOrderReq, TicketProblem> {
        self.order_with(self.buy, self.kind, plan, cx)
    }

    /// The order of a side and a kind, from a plan: the ticket's own, or its opposite in an OCO
    /// pair.
    pub(super) fn order_with(
        &self,
        buy: bool,
        kind: Kind,
        plan: &Plan,
        cx: &App,
    ) -> Result<NewOrderReq, TicketProblem> {
        if let Some(problem) = &plan.problem {
            return Err(problem.clone());
        }
        let symbol = self.symbol.as_ref().ok_or(TicketProblem::NoSymbol)?;
        let (Some(entry), Some(sized)) = (plan.entry, plan.sized) else {
            return Err(TicketProblem::NoPrice);
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
                let range = pips.ok_or(TicketProblem::NoLimitRange)?;
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
            order = match self.tif {
                Tif::GoodTillCancel => order.with_time_in_force(TimeInForce::GoodTillCancel, None),
                Tif::GoodTillDate => {
                    order.with_time_in_force(TimeInForce::GoodTillDate, self.expires_at(cx))
                }
            };
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
    pub(super) fn orders(&self, plan: &Plan, cx: &App) -> Result<Vec<NewOrderReq>, TicketProblem> {
        let time_stop = self.time_stop_now(cx).rule();
        let follow_base = self.follow_base(cx);
        if !self.exits.on {
            let mut order = self.order(plan, cx)?;
            // The rule a pending order keeps its ATR stop on, with the take profit as a multiple
            // of the risk when it is one.
            let follow = follow_base.clone().map(|atr| {
                let rr = match (plan.entry, plan.stop, plan.target) {
                    (Some(entry), Some(stop), Some(target))
                        if self.target_unit == math::Offset::Ratio && entry != stop =>
                    {
                        Some((target - entry).abs() / (entry - stop).abs())
                    }
                    _ => None,
                };
                AtrFollow { atr, rr }
            });
            // A time stop, or a rule on the ATR, needs the label to travel with the order.
            if time_stop.is_some() || follow.is_some() {
                order.label = Some(
                    plan::Label {
                        group: plan::new_group(now_ms()),
                        leg: 1,
                        of: 1,
                        break_even: None,
                        oco: false,
                        time_stop,
                        follow,
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
            return Err(TicketProblem::ExitsNeedStopLoss);
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
            return Err(TicketProblem::VolumeTooSmallForExits);
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
                return Err(TicketProblem::OcoPriceInvalid);
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
                        follow: follow_base.clone().map(|atr| AtrFollow {
                            atr,
                            rr: (target_r > 0.0).then_some(target_r),
                        }),
                    }
                    .encode(),
                );
                orders.push(order);
            }
        }
        Ok(orders)
    }

    /// The ATR a pending order with an ATR stop keeps its stop on: the one of the ticket, on the
    /// timeframe it is read on (the chart's own when none was picked). `None` when the option is
    /// off, the order is not a pending one, the stop is not an ATR one, or the ticket follows a
    /// drawing, which has its own ATR.
    fn follow_base(&self, cx: &App) -> Option<wyck_trading::atr_stop::AtrStop> {
        if !(self.follow_atr && self.stop_atr && self.kind.is_pending() && self.link.is_none()) {
            return None;
        }
        let mut atr = self.atr.clone().normalized();
        if atr.timeframe.is_none() {
            let chart = self.chart.as_ref()?;
            atr.timeframe = Some(chart.read(cx).timeframe().code());
        }
        Some(atr)
    }

    pub(super) fn describe(&self, plan: &Plan, cx: &App) -> String {
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

    pub(super) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let plan = self.plan(cx);
        let orders = match self.orders(&plan, cx) {
            Ok(orders) => orders,
            Err(found) => {
                wyck_ui_kit::toast::Toast::warning("The order is not ready", problem::text(&found))
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
                wyck_ui_kit::toast::Toast::warning("Blocked by your safety settings", reason)
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
    pub(super) fn confirm_details(
        &self,
        plan: &Plan,
        live: bool,
        warnings: Vec<String>,
        cx: &App,
    ) -> Details {
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
            ..Details::default()
        }
    }
}
