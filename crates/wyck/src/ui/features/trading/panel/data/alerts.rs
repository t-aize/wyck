//! The rows of the alerts tab and of the alert log.

use super::{Cell, Ctx, Row, RowKind, Table, Tone, finish};

use crate::app::prefs::panel::Tab;

pub(super) fn alerts(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let book = ctx.alerts.book();
    let mut rows = Vec::new();
    let now = crate::app::market_data::now_ms();
    for alert in &book.alerts {
        let symbol = alert.symbol_id;
        let contract = account.book.contract(symbol);
        let digits = ctx
            .alerts
            .digits
            .get(&symbol)
            .copied()
            .unwrap_or(contract.digits);
        let state = match (alert.active, alert.fired_at) {
            (true, _) if alert.is_snoozed(now) => "Snoozed",
            (true, _) if alert.repeats() => "Watching (repeats)",
            (true, _) => "Watching",
            (false, _) if alert.is_expired(now) => "Expired",
            (false, Some(_)) => "Fired",
            (false, None) => "Paused",
        };
        // A level in the units of the price: an alert on an indicator has its own units.
        let on_price = matches!(alert.source, crate::app::alerts::Source::Price { .. })
            && alert.versus.is_none()
            && alert.condition.needs_level();
        let measure = alert.source.is_measure() && alert.condition.needs_level();
        let measure_unit = match &alert.source {
            crate::app::alerts::Source::Spread { .. } => " pips",
            _ => "",
        };
        let level = if on_price {
            Cell::text(format!("{:.*}", digits as usize, alert.price))
        } else if measure {
            Cell::text(format!("{}{measure_unit}", alert.level_text(digits)))
        } else {
            Cell::dash()
        };
        let cells = vec![
            if alert.source.has_symbol() {
                Cell::text(alert.symbol.clone())
            } else {
                Cell::text("Account")
            },
            Cell::text(alert.describe(digits)).tone(Tone::Muted),
            level,
            Cell::text(state).tone(if alert.active {
                Tone::Accent
            } else {
                Tone::Muted
            }),
            Cell::text(alert.message.clone()).tone(Tone::Muted),
        ];
        rows.push(Row {
            key: format!("alert-{}", alert.id),
            symbol: Some(symbol),
            kind: RowKind::Alert { id: alert.id },
            cells,
        });
    }
    finish(ctx, Tab::Alerts, rows)
}

/// The times alerts fired, newest first.
pub(super) fn alert_log(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let mut rows = Vec::new();
    for (n, firing) in ctx.alerts.book().history.iter().enumerate() {
        let symbol = firing.symbol_id;
        let digits = if symbol > 0 {
            ctx.alerts
                .digits
                .get(&symbol)
                .copied()
                .unwrap_or(account.book.contract(symbol).digits)
        } else {
            5
        };
        let text_or_dash = |text: &str| {
            if text.is_empty() {
                Cell::dash()
            } else {
                Cell::text(text).tone(Tone::Muted)
            }
        };
        let cells = vec![
            Cell::text(ctx.time(firing.at)),
            Cell::text(firing.symbol.clone()),
            text_or_dash(&firing.watched),
            text_or_dash(&firing.condition),
            Cell::opt(firing.value, |v| format!("{v:.*}", digits as usize)),
            Cell::text(firing.text.clone()).tone(Tone::Muted),
        ];
        rows.push(Row {
            key: format!("firing-{n}-{}", firing.at),
            symbol: (symbol > 0).then_some(symbol),
            kind: RowKind::Plain,
            cells,
        });
    }
    finish(ctx, Tab::AlertLog, rows)
}

/// A word with its first letter capital.
pub(super) fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
