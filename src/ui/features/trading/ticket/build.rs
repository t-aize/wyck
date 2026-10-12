//! Building an order ticket: its fields, the subscriptions that keep it in step with the account,
//! and the first values it starts with.

use super::{OrderTicket, TicketEvent, TicketPrefs, helpers};
use crate::app::account::Account;
use crate::domain::indicators::atr_stop::AtrStop;
use crate::domain::trading::math::{Contract, Offset};
use crate::domain::trading::plan;
use crate::ui::kit::input::{InputEvent, InputState};
use crate::ui::kit::number;
use gpui::prelude::*;
use gpui::{Context, Entity, Window};

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
                helpers::size_kind(prefs.size_mode, &Contract::default()),
                prefs.size,
                window,
                cx,
            )
        });
        let price = cx.new(|cx| InputState::new(window, cx));
        let stop_loss = cx.new(|cx| {
            number::empty(
                helpers::offset_kind(prefs.stop_unit, &Contract::default()),
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
                helpers::offset_kind(prefs.target_unit, &Contract::default()),
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
            follow_atr: prefs.follow_atr,
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
}
