//! The Safety page of the settings: the limits every new order is checked against (see
//! [`crate::domain::trading::guard`]). A limit of 0 is off.

use super::*;
use crate::domain::trading::guard::RiskPrefs;

/// One number of the safety limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RiskField {
    LotsPerOrder,
    LotsPerSymbol,
    OneClickLots,
    OpenPositions,
    DailyLoss,
    DailyLossShare,
    DailyTrades,
    Cooldown,
    Spread,
    Collar,
}

impl RiskField {
    pub(super) const ALL: [Self; 10] = [
        Self::LotsPerOrder,
        Self::LotsPerSymbol,
        Self::OneClickLots,
        Self::OpenPositions,
        Self::DailyLoss,
        Self::DailyLossShare,
        Self::DailyTrades,
        Self::Cooldown,
        Self::Spread,
        Self::Collar,
    ];

    pub(super) fn kind(self) -> number::Kind {
        match self {
            Self::LotsPerOrder | Self::LotsPerSymbol | Self::OneClickLots => number::Kind::Amount,
            Self::OpenPositions | Self::DailyTrades | Self::Cooldown => number::Kind::Count,
            Self::DailyLoss => number::Kind::Money,
            Self::DailyLossShare | Self::Collar => number::Kind::Percent,
            Self::Spread => number::Kind::Pips,
        }
    }

    pub(super) fn value(self, risk: &RiskPrefs) -> f64 {
        match self {
            Self::LotsPerOrder => risk.max_lots_per_order,
            Self::LotsPerSymbol => risk.max_lots_per_symbol,
            Self::OneClickLots => risk.one_click_max_lots,
            Self::OpenPositions => f64::from(risk.max_open_positions),
            Self::DailyLoss => risk.max_daily_loss,
            Self::DailyLossShare => risk.max_daily_loss_pct,
            Self::DailyTrades => f64::from(risk.max_daily_trades),
            Self::Cooldown => f64::from(risk.cooldown_minutes),
            Self::Spread => risk.max_spread_pips,
            Self::Collar => risk.price_collar_pct,
        }
    }

    fn set(self, risk: &mut RiskPrefs, value: f64) {
        let value = if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        };
        match self {
            Self::LotsPerOrder => risk.max_lots_per_order = value,
            Self::LotsPerSymbol => risk.max_lots_per_symbol = value,
            Self::OneClickLots => risk.one_click_max_lots = value,
            Self::OpenPositions => risk.max_open_positions = value as u32,
            Self::DailyLoss => risk.max_daily_loss = value,
            Self::DailyLossShare => risk.max_daily_loss_pct = value,
            Self::DailyTrades => risk.max_daily_trades = value as u32,
            Self::Cooldown => risk.cooldown_minutes = value as u32,
            Self::Spread => risk.max_spread_pips = value,
            Self::Collar => risk.price_collar_pct = value,
        }
    }
}

impl SettingsHub {
    /// Changes the safety limits and saves them.
    pub(super) fn edit_risk(&self, cx: &mut Context<Self>, change: impl FnOnce(&mut RiskPrefs)) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| {
                change(&mut prefs.risk);
                prefs.risk = std::mem::take(&mut prefs.risk).normalized();
            });
        });
    }

    pub(super) fn set_risk_field(&self, field: RiskField, value: f64, cx: &mut Context<Self>) {
        self.edit_risk(cx, |risk| field.set(risk, value));
    }

    fn risk_input(&self, field: RiskField) -> AnyElement {
        match self.risk_inputs.iter().find(|(f, _)| *f == field) {
            Some((_, state)) => number::field(state, tokens::field::number()).into_any_element(),
            None => div().into_any_element(),
        }
    }

    pub(super) fn safety_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let risk = self.workspace.read(cx).preferences().risk.clone();
        let (kill, stop) = (cx.entity(), cx.entity());
        form::page()
            .child(form::group(
                IconName::ShieldCheck,
                "Kill switch",
                [form::field(
                    "Block every new order",
                    Some("Closing positions still works. Turn it back off to trade again"),
                    controls::toggle("safety-kill", risk.kill_switch, move |on, _w, cx| {
                        kill.update(cx, |hub, cx| hub.edit_risk(cx, |r| r.kill_switch = on));
                    }),
                )],
            ))
            .child(form::group(
                IconName::Target,
                "Daily loss",
                [
                    form::field(
                        "Most to lose in a day",
                        Some("In the account's money, closed trades plus open ones. 0 is off"),
                        self.risk_input(RiskField::DailyLoss),
                    ),
                    form::field(
                        "Most to lose in a day (percent)",
                        Some("Of the balance the day started with. The lower of the two applies"),
                        self.risk_input(RiskField::DailyLossShare),
                    ),
                    form::field(
                        "Most orders in a day",
                        Some("0 is off"),
                        self.risk_input(RiskField::DailyTrades),
                    ),
                    form::field(
                        "Pause after a losing trade (minutes)",
                        Some("No new order until it is over. 0 is off"),
                        self.risk_input(RiskField::Cooldown),
                    ),
                ],
            ))
            .child(form::note(
                "When the daily limit is reached, new orders are blocked until the next day and the ticket offers to close every position. Nothing is closed for you. The day follows the time zone chosen for the charts.",
            ))
            .child(form::group(
                IconName::SlidersHorizontal,
                "Order size",
                [
                    form::field(
                        "Most lots in one order",
                        Some("0 is off"),
                        self.risk_input(RiskField::LotsPerOrder),
                    ),
                    form::field(
                        "Most lots open on one symbol",
                        Some("Counts what is open and the new order. 0 is off"),
                        self.risk_input(RiskField::LotsPerSymbol),
                    ),
                    form::field(
                        "Most lots by one click",
                        Some("A larger order needs the confirmation. 0 is off"),
                        self.risk_input(RiskField::OneClickLots),
                    ),
                    form::field(
                        "Most positions open at once",
                        Some("0 is off"),
                        self.risk_input(RiskField::OpenPositions),
                    ),
                ],
            ))
            .child(form::group(
                IconName::TriangleAlert,
                "Order checks",
                [
                    form::field(
                        "Require a stop loss",
                        Some("An order without one is refused"),
                        controls::toggle("safety-stop", risk.require_stop_loss, move |on, _w, cx| {
                            stop.update(cx, |hub, cx| hub.edit_risk(cx, |r| r.require_stop_loss = on));
                        }),
                    ),
                    form::field(
                        "Refuse a price too far from the market (percent)",
                        Some("Catches a misplaced decimal point on a pending order. 0 is off"),
                        self.risk_input(RiskField::Collar),
                    ),
                    form::field(
                        "Warn when the spread is wider than (pips)",
                        Some("Asks before sending. 0 is off"),
                        self.risk_input(RiskField::Spread),
                    ),
                ],
            ))
            .child(form::note(
                "These limits check every new order, whether it comes from the ticket, a chart or a reversal. An order that only closes a position is never blocked.",
            ))
            .into_any_element()
    }
}
