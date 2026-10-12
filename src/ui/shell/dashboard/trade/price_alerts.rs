//! The price alerts of the dashboard: adding them, feeding them prices, and what happens when one fires.

use super::Pending;

use crate::app::alerts;
use crate::app::market_data::now_ms;
use crate::domain::market::PRICE_SCALE;
use crate::ui::features::multichart::SymbolRef;
use crate::ui::features::trading::panel::Tab;
use crate::ui::kit::toast;
use crate::ui::shell::dashboard::Dashboard;
use gpui::Context;

impl Dashboard {
    /// Makes an alert on a spread or a profit, with a level that fits what it sees now, and
    /// opens it so the level and the condition can be set straight away.
    pub(super) fn add_measure_alert(
        &mut self,
        kind: crate::ui::features::trading::panel::NewAlert,
        cx: &mut Context<Self>,
    ) {
        use crate::app::alerts::model::source::PnlScope;
        use crate::ui::features::trading::panel::NewAlert;
        let now = now_ms();
        let (alert, digits) = match kind {
            NewAlert::Spread => {
                let Some(symbol) = self.multi.read(cx).active_symbol(cx) else {
                    toast::show(
                        cx,
                        toast::Kind::Info,
                        "No symbol",
                        "Open a chart to watch the spread of its symbol.",
                    );
                    return;
                };
                let pip = self.alerts.read(cx).pip_of(symbol.id);
                let (bid, ask) = self.trading.read(cx).quote(symbol.id);
                // Twice the spread it has now, a round number, as a first level.
                let level = bid.zip(ask).map_or(2.0, |(bid, ask)| {
                    alerts::eval::spread_pips(bid, ask, pip) * 2.0
                });
                let level = (level.max(1.0) * 10.0).round() / 10.0;
                (
                    alerts::Alert::spread(0, symbol.id, &symbol.name, pip, level, now),
                    symbol.digits,
                )
            }
            NewAlert::Profit => {
                // One percent of the balance lost, to start with.
                let balance = self.trading.read(cx).summary().balance;
                let level = -(balance * 0.01).round().max(1.0);
                (
                    alerts::Alert::pnl(0, 0, "", PnlScope::Account, level, now),
                    5,
                )
            }
            NewAlert::Position { id, symbol_id } => {
                let name = self.trading.read(cx).book.name(symbol_id);
                let digits = self.trading.read(cx).book.contract(symbol_id).digits;
                (
                    alerts::Alert::pnl(0, symbol_id, &name, PnlScope::Position { id }, 0.0, now),
                    digits,
                )
            }
        };
        self.insert_alert(alert, digits, cx);
    }

    /// Adds an alert made here and opens it, or says the limit is reached.
    pub(super) fn insert_alert(
        &mut self,
        alert: alerts::Alert,
        digits: u32,
        cx: &mut Context<Self>,
    ) {
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let symbol_id = alert.symbol_id;
        let added = self.alerts.update(cx, |alerts, cx| {
            if symbol_id > 0 {
                alerts.digits.entry(symbol_id).or_insert(digits);
            }
            alerts.edit(cx, |book| book.insert(alert, limit))
        });
        match added {
            Some(id) => {
                self.pending.push(Pending::EditAlert(id));
                if let Some(panel) = &self.panel {
                    panel.update(cx, |panel, cx| panel.show_tab(Tab::Alerts, cx));
                }
            }
            None => toast::show(
                cx,
                toast::Kind::Warning,
                "No alert added",
                crate::ui::kit::shortcut::text(&format!(
                    "The limit is {limit} alerts. Change it in Settings (Ctrl+,)."
                )),
            ),
        }
        cx.notify();
    }

    /// Gives the alerts what they read from the account: how the symbols they watch are written
    /// (from the contracts, which are asked for when they are not known yet) and, for the ones on
    /// a profit, what the positions make now.
    pub(in crate::ui::shell::dashboard) fn feed_alerts(&mut self, cx: &mut Context<Self>) {
        let symbols = self.alerts.read(cx).symbols();
        for id in symbols {
            self.trading
                .update(cx, |account, cx| account.ensure_contract(id, cx));
            let contract = self.trading.read(cx).book.contracts.get(&id).cloned();
            // Before the answer a contract is a placeholder: it must not hide what is known.
            let Some(contract) =
                contract.filter(|c| *c != crate::domain::trading::math::Contract::default())
            else {
                continue;
            };
            let (digits, pip) = (contract.digits, contract.pip());
            self.alerts
                .update(cx, |alerts, cx| alerts.learn(id, digits, pip, cx));
        }
        if self.alerts.read(cx).watches_profit() {
            let profits = self.trading.read(cx).profits();
            self.alerts
                .update(cx, |alerts, cx| alerts.on_profit(&profits, cx));
        }
    }

    /// An alert fired or ran out: says so, with the buttons that help.
    pub(in crate::ui::shell::dashboard) fn on_alert_event(
        &mut self,
        event: &alerts::AlertsEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            alerts::AlertsEvent::Fired(alert, text) => {
                let title = if alert.source.is_indicator() {
                    "Indicator alert"
                } else if alert.drawing().is_some() {
                    "Drawing alert"
                } else {
                    "Price alert"
                };
                let (id, symbol) = (alert.id, alert.symbol_id);
                let dashboard = cx.entity();
                let snooze = self.alerts.clone();
                let mut toast = toast::Toast::warning(title, text.clone())
                    .sticky(alert.sticky)
                    .action("Show chart", move |_, cx| {
                        dashboard.update(cx, |d, cx| d.show_symbol(symbol, cx));
                    })
                    .action("Snooze 1 h", move |_, cx| {
                        snooze.update(cx, |a, cx| a.snooze(id, now_ms() + 3_600_000, cx));
                    });
                if alert.repeats() {
                    let off = self.alerts.clone();
                    toast = toast.action("Turn off", move |_, cx| {
                        off.update(cx, |a, cx| {
                            a.edit(cx, |book| {
                                if let Some(alert) = book.get_mut(id) {
                                    alert.active = false;
                                }
                            })
                        });
                    });
                }
                if !alert.tag.is_empty() {
                    toast = toast.hint(format!("Tag: {}", alert.tag));
                }
                toast.show(cx);
                self.alert_output(alert, title, text, cx);
            }
            alerts::AlertsEvent::Expired(alert) => {
                let digits = self
                    .alerts
                    .read(cx)
                    .digits
                    .get(&alert.symbol_id)
                    .copied()
                    .unwrap_or(5);
                toast::Toast::info("Alert expired", alert.describe(digits)).show(cx);
            }
            alerts::AlertsEvent::Changed => {}
        }
    }

    /// The sound and the desktop notification of an alert that fired, as the user set them.
    pub(super) fn alert_output(
        &self,
        alert: &alerts::Alert,
        title: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let output = self.workspace.read(cx).preferences().alert_output.clone();
        // No window of the app is active when the user is elsewhere.
        let in_use = cx.active_window().is_some();
        let plan = alerts::sound::plan(&output, alert.sound, in_use);
        let speaker = alerts::sound::speaker();
        if let Some(kind) = plan.sound {
            speaker.play(kind, output.custom.as_deref(), output.volume);
        }
        if plan.notification {
            speaker.notify(&format!("{title}: {}", alert.symbol), text);
        }
    }

    /// Adds an alert on `symbol` at `price` and opens it, so its condition and message can be
    /// set straight away.
    pub(in crate::ui::shell::dashboard) fn add_alert(
        &mut self,
        symbol: &SymbolRef,
        price: f64,
        cx: &mut Context<Self>,
    ) {
        let (id, name, digits) = (symbol.id, symbol.name.to_string(), symbol.digits);
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let pip = symbol.pip_position.map(crate::domain::market::pip_size);
        let added = self.alerts.update(cx, |alerts, cx| {
            alerts.digits.insert(id, digits);
            if let Some(pip) = pip {
                alerts.set_pip(id, pip);
            }
            alerts.edit(cx, |book| {
                book.add_with_limit(
                    id,
                    &name,
                    price,
                    alerts::Condition::Crossing,
                    now_ms(),
                    limit,
                )
            })
        });
        match added {
            Some(alert) => {
                self.pending.push(Pending::EditAlert(alert));
                if let Some(panel) = &self.panel {
                    panel.update(cx, |panel, cx| panel.show_tab(Tab::Alerts, cx));
                }
            }
            None => toast::show(
                cx,
                toast::Kind::Warning,
                "No alert added",
                crate::ui::kit::shortcut::text(&format!(
                    "The limit is {limit} alerts. Change it in Settings (Ctrl+,)."
                )),
            ),
        }
        cx.notify();
    }

    /// Adds an alert on an indicator or a drawing and opens it, so its condition can be set.
    pub(in crate::ui::shell::dashboard) fn add_alert_on(
        &mut self,
        symbol: &SymbolRef,
        seed: &crate::ui::features::chart::events::AlertSeed,
        cx: &mut Context<Self>,
    ) {
        use crate::app::alerts::{Alert, Condition, Source};
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let mut alert = Alert::price(0, symbol.id, &symbol.name, 0.0, now_ms());
        match seed {
            crate::ui::features::chart::events::AlertSeed::Indicator { study, timeframe } => {
                alert.timeframe = timeframe.clone();
                let spec = study.kind.spec();
                if matches!(
                    spec.format,
                    crate::domain::indicators::spec::ValueFormat::Price
                ) {
                    // An average or a band on the prices: the price crosses it.
                    alert.versus = Some(Source::Indicator {
                        study: study.clone(),
                        plot: 0,
                    });
                } else {
                    // An oscillator: its value crosses a level, a high one to start with.
                    alert.source = Source::Indicator {
                        study: study.clone(),
                        plot: 0,
                    };
                    alert.price = spec
                        .range
                        .map_or(0.0, |(low, high)| low + (high - low) * 0.7);
                }
            }
            crate::ui::features::chart::events::AlertSeed::Drawing {
                id,
                zone,
                timeframe,
            } => {
                alert.timeframe = timeframe.clone();
                alert.versus = Some(Source::Drawing { id: *id });
                alert.condition = if *zone {
                    Condition::EntersZone
                } else {
                    Condition::Crossing
                };
            }
        }
        let digits = symbol.digits;
        let pip = symbol.pip_position.map(crate::domain::market::pip_size);
        let added = self.alerts.update(cx, |alerts, cx| {
            alerts.digits.insert(symbol.id, digits);
            if let Some(pip) = pip {
                alerts.set_pip(symbol.id, pip);
            }
            alerts.edit(cx, |book| book.insert(alert, limit))
        });
        match added {
            Some(id) => {
                self.pending.push(Pending::EditAlert(id));
                if let Some(panel) = &self.panel {
                    panel.update(cx, |panel, cx| panel.show_tab(Tab::Alerts, cx));
                }
            }
            None => toast::show(
                cx,
                toast::Kind::Warning,
                "No alert added",
                crate::ui::kit::shortcut::text(&format!(
                    "The limit is {limit} alerts. Change it in Settings (Ctrl+,)."
                )),
            ),
        }
        cx.notify();
    }

    /// An alert at the active chart's last price.
    pub(in crate::ui::shell::dashboard) fn add_alert_here(&mut self, cx: &mut Context<Self>) {
        let chart = self.multi.read(cx).active_chart().clone();
        let Some(symbol) = self.multi.read(cx).active_symbol(cx) else {
            return;
        };
        let Some(bid) = chart.read(cx).quote().0 else {
            return;
        };
        self.add_alert(&symbol, bid as f64 / PRICE_SCALE as f64, cx);
    }
}
