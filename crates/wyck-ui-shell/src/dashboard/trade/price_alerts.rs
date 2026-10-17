//! The price alerts of the dashboard: adding them, feeding them prices, and what happens when one fires.

use super::Pending;

use crate::dashboard::Dashboard;
use gpui::Context;
use wyck_app::alerts;
use wyck_app::market_data::now_ms;
use wyck_app::prefs::panel::Tab;
use wyck_core::math::i64_to_f64;
use wyck_market::PRICE_SCALE;
use wyck_ui_chart::multichart::SymbolRef;
use wyck_ui_kit::toast;

impl Dashboard {
    /// Gives the alerts what they read from the account: how the symbols they watch are written
    /// (from the contracts, which are asked for when they are not known yet) and, for the ones on
    /// a profit, what the positions make now.
    pub(in crate::dashboard) fn feed_alerts(&mut self, cx: &mut Context<Self>) {
        let symbols = self.alerts.read(cx).symbols();
        for id in symbols {
            self.trading
                .update(cx, |account, cx| account.ensure_contract(id, cx));
            let contract = self.trading.read(cx).book.contracts.get(&id).copied();
            // Before the answer a contract is a placeholder: it must not hide what is known.
            let Some(contract) =
                contract.filter(|c| *c != wyck_trading::contract::Contract::default())
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
    pub(in crate::dashboard) fn on_alert_event(
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
                            });
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
    pub(in crate::dashboard) fn add_alert(
        &mut self,
        symbol: &SymbolRef,
        price: f64,
        cx: &mut Context<Self>,
    ) {
        let (id, name, digits) = (symbol.id, symbol.name.to_string(), symbol.digits);
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let pip = symbol.pip_position.map(wyck_market::pip_size);
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
                format!(
                    "The limit is {limit} alerts. Change it in Settings{}.",
                    wyck_ui_kit::hints::suffix("open_settings")
                ),
            ),
        }
        cx.notify();
    }

    /// Adds an alert on an indicator or a drawing and opens it, so its condition can be set.
    pub(in crate::dashboard) fn add_alert_on(
        &mut self,
        symbol: &SymbolRef,
        seed: &wyck_ui_chart::chart::events::AlertSeed,
        cx: &mut Context<Self>,
    ) {
        use wyck_app::alerts::{Alert, Condition, Source};
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let mut alert = Alert::price(0, symbol.id, &symbol.name, 0.0, now_ms());
        match seed {
            wyck_ui_chart::chart::events::AlertSeed::Indicator { study, timeframe } => {
                alert.timeframe.clone_from(timeframe);
                let spec = study.kind.spec();
                if matches!(spec.format, wyck_indicators::spec::ValueFormat::Price) {
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
            wyck_ui_chart::chart::events::AlertSeed::Drawing {
                id,
                zone,
                timeframe,
            } => {
                alert.timeframe.clone_from(timeframe);
                alert.versus = Some(Source::Drawing { id: *id });
                alert.condition = if *zone {
                    Condition::EntersZone
                } else {
                    Condition::Crossing
                };
            }
        }
        let digits = symbol.digits;
        let pip = symbol.pip_position.map(wyck_market::pip_size);
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
                format!(
                    "The limit is {limit} alerts. Change it in Settings{}.",
                    wyck_ui_kit::hints::suffix("open_settings")
                ),
            ),
        }
        cx.notify();
    }

    /// An alert at the active chart's last price.
    pub(in crate::dashboard) fn add_alert_here(&mut self, cx: &mut Context<Self>) {
        let chart = self.multi.read(cx).active_chart().clone();
        let Some(symbol) = self.multi.read(cx).active_symbol(cx) else {
            return;
        };
        let Some(bid) = chart.read(cx).quote().0 else {
            return;
        };
        self.add_alert(&symbol, i64_to_f64(bid) / i64_to_f64(PRICE_SCALE), cx);
    }
}
