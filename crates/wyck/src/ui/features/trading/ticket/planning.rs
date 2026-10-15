//! The plan of the ticket: what the fields add up to, the lines on the chart and the margin asked.

use super::helpers::side_of;
use super::{Kind, LINE_ENTRY, LINE_STOP, LINE_TARGET, Margin, OrderTicket, Plan, Tif};
use crate::ui::features::chart::{ChartLine, LineId};
use gpui::{App, Context, Entity};
use std::time::Duration;
use wyck_app::market_data::now_ms;
use wyck_app::system::runtime;
use wyck_drawings::model::style::Dash;
use wyck_trading::math;
use wyck_trading::math::{Contract, Offset, Scale, SizeMode, Stepped, TicketProblem};
use wyck_ui_kit::input::InputState;
use wyck_ui_kit::number;

impl OrderTicket {
    pub(super) fn contract(&self, cx: &App) -> Contract {
        self.symbol
            .as_ref()
            .map(|s| {
                let mut contract = self.account.read(cx).book.contract(s.id);
                contract.digits = s.digits;
                contract
            })
            .unwrap_or_default()
    }

    pub(super) fn quote(&self, cx: &App) -> (Option<f64>, Option<f64>) {
        self.symbol
            .as_ref()
            .map_or((None, None), |s| self.account.read(cx).quote(s.id))
    }

    pub(super) fn read(state: &Entity<InputState>, cx: &App) -> Option<f64> {
        number::parse(&state.read(cx).value())
    }

    /// The pips typed in the slippage field, when there are some.
    pub(super) fn slippage_pips(&self, cx: &App) -> Option<f64> {
        Self::read(&self.slippage, cx).filter(|v| *v > 0.0)
    }

    /// When a good-till-date order would expire, in Unix milliseconds.
    pub(super) fn expires_at(&self, cx: &App) -> Option<i64> {
        Self::read(&self.expiry, cx)
            .filter(|v| *v > 0.0)
            .map(|v| now_ms() + (v * self.expiry_span.millis() as f64) as i64)
    }

    /// The price the order would enter at: its own for a pending order, the market's for a
    /// market order.
    pub(super) fn entry(&self, cx: &App) -> Option<f64> {
        match self.kind {
            Kind::Market => {
                let (bid, ask) = self.quote(cx);
                if self.buy { ask } else { bid }
            }
            Kind::Limit | Kind::Stop | Kind::StopLimit => Self::read(&self.price, cx),
        }
    }

    pub(super) fn scale(&self, plan: &Plan, cx: &App) -> Scale {
        Scale {
            pip: self.contract(cx).pip(),
            money_per_price: plan.money_per_price,
            balance: self.account.read(cx).summary().balance,
            stop_distance: plan.stop_distance,
        }
    }

    /// The price of a protection given in `unit`, from the value of its field.
    pub(super) fn protection_price(
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
    pub(super) fn plan(&self, cx: &App) -> Plan {
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
        let sized: Result<Stepped, TicketProblem> = (|| match self.size_mode {
            SizeMode::Lots => Ok(contract.volume_near(size.ok_or(TicketProblem::NoVolume)?)),
            SizeMode::Units => {
                Ok(contract.volume_near(size.ok_or(TicketProblem::NoVolume)? / lot_units))
            }
            SizeMode::RiskBalance | SizeMode::RiskEquity | SizeMode::RiskMoney => {
                let value = size.ok_or(TicketProblem::NoRisk)?;
                let money = match self.size_mode {
                    SizeMode::RiskBalance => summary.balance * value / 100.0,
                    SizeMode::RiskEquity => summary.equity * value / 100.0,
                    _ => value,
                };
                if !self.stop_on {
                    return Err(TicketProblem::StopLossOff);
                }
                let stop = early_stop.ok_or(TicketProblem::NoStopLoss)?;
                let distance = side(stop, true).ok_or(TicketProblem::NoPrice)?;
                if distance <= 0.0 {
                    return Err(TicketProblem::StopLossWrongSide);
                }
                let rate = rate.ok_or(TicketProblem::WaitingForRate)?;
                let lots = math::lots_for_risk(money, distance, rate, &contract)
                    .ok_or(TicketProblem::NoRisk)?;
                Ok(contract.volume_at_most(lots))
            }
            SizeMode::FreeMargin => {
                let value = size.ok_or(TicketProblem::NoMarginShare)?;
                let lot = self
                    .margin
                    .map(|m| side_of(m.lot, self.buy))
                    .filter(|m| *m > 0.0)
                    .ok_or(TicketProblem::WaitingForMargin)?;
                Ok(contract.volume_at_most(summary.free_margin.max(0.0) * value / 100.0 / lot))
            }
        })();
        let mut plan = Plan {
            entry,
            rate,
            ..Plan::default()
        };
        let problem = |plan: &mut Plan, found: TicketProblem| {
            plan.problem.get_or_insert(found);
        };
        match sized {
            Ok(sized) => plan.sized = Some(sized),
            Err(found) => problem(&mut plan, found),
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
                problem(&mut plan, TicketProblem::ExitsNeedStopLoss);
            }
            if let Some(found) = exits.problem() {
                problem(&mut plan, TicketProblem::Exits(found));
            }
            if exits.oco_pips > 0.0 && !matches!(self.kind, Kind::Limit | Kind::Stop) {
                problem(&mut plan, TicketProblem::OcoNeedsPending);
            }
        }

        if entry.is_none() {
            problem(&mut plan, TicketProblem::NoPrice);
        }
        if self.stop_on && plan.stop.is_none() {
            let found = if self.stop_atr {
                TicketProblem::AtrUnavailable
            } else if self.stop_unit.needs_volume() && rate.is_none() {
                TicketProblem::WaitingForRate
            } else {
                TicketProblem::NoStopLoss
            };
            problem(&mut plan, found);
        }
        if self.target_on && !self.exits.on && plan.target.is_none() {
            let found = if self.target_unit == Offset::Ratio && plan.stop_distance.is_none() {
                TicketProblem::RatioNeedsStopLoss
            } else if self.target_unit.needs_volume() && rate.is_none() {
                TicketProblem::WaitingForRate
            } else {
                TicketProblem::NoTakeProfit
            };
            problem(&mut plan, found);
        }
        if let Some(entry) = entry
            && let Err(wrong) = math::check_protection(self.buy, entry, plan.stop, plan.target)
        {
            problem(&mut plan, wrong);
        }
        if self.kind.is_pending() && self.tif == Tif::GoodTillDate && self.expires_at(cx).is_none()
        {
            problem(&mut plan, TicketProblem::NoExpiry);
        }
        if self.kind == Kind::StopLimit && self.slippage_pips(cx).is_none() {
            problem(&mut plan, TicketProblem::NoLimitRange);
        }
        if self.kind == Kind::Market && self.slippage_on && self.slippage_pips(cx).is_none() {
            problem(&mut plan, TicketProblem::NoSlippage);
        }
        if self.symbol.is_none() {
            plan.problem = Some(TicketProblem::NoSymbol);
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
        let palette_line = wyck_ui_kit::theme::colors().line;
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
        let palette = wyck_ui_kit::theme::colors();
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
    pub(super) fn request_margin(&mut self, symbol: i64, volume: i64, cx: &mut Context<Self>) {
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
                let client = session.client().ok_or(wyck_app::broker::Error::Closed)?;
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
                let money = |m: &wyck_trading::ExpectedMargin| {
                    (
                        wyck_trading::money(m.buy_margin, digits),
                        wyck_trading::money(m.sell_margin, digits),
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
}
