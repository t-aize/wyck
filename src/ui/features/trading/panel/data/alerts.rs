//! The rows of the alerts tab and of the alert log.

use super::{Cell, Ctx, Row, RowKind, Table, Tone, finish};

use crate::app::prefs::columns::{AlertCol, AlertLogCol};
use crate::app::prefs::panel::Tab;
use crate::app::prefs::ticket::Slot;

pub(super) fn alerts(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let book = ctx.alerts.book();
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    // Read from the account once, and only when an alert on a profit needs it.
    let mut profits: Option<crate::app::alerts::Profits> = None;
    for alert in &book.alerts {
        unfiltered += 1;
        let symbol = alert.symbol_id;
        let contract = account.book.contract(symbol);
        let digits = ctx
            .alerts
            .digits
            .get(&symbol)
            .copied()
            .unwrap_or(contract.digits);
        let now = crate::app::market_data::now_ms();
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
        let described = alert.describe(digits);
        if !ctx.keeps(
            symbol,
            None,
            &[&alert.symbol, &described, &alert.tag, state, &alert.message],
        ) {
            continue;
        }
        let (bid, ask) = account.quote(symbol);
        let market = match (bid, ask) {
            (Some(b), Some(a)) => Some((b + a) / 2.0),
            (b, a) => b.or(a),
        };
        // A spread or a profit: its value now, and the level it waits for, in its own unit.
        let measure = alert.source.is_measure() && alert.condition.needs_level();
        let current = match &alert.source {
            crate::app::alerts::Source::Spread { pip } => bid
                .zip(ask)
                .map(|(bid, ask)| crate::app::alerts::eval::spread_pips(bid, ask, *pip)),
            crate::app::alerts::Source::Pnl { scope } => profits
                .get_or_insert_with(|| account.profits())
                .of(scope, symbol),
            _ => None,
        };
        let measure_unit = match &alert.source {
            crate::app::alerts::Source::Spread { .. } => " pips",
            _ => "",
        };
        let cells = AlertCol::ALL
            .iter()
            .map(|col| match col {
                AlertCol::Symbol if !alert.source.has_symbol() => Cell::text("Account"),
                AlertCol::Symbol => Cell::text(alert.symbol.clone()),
                AlertCol::Condition => Cell::text(described.clone()).tone(Tone::Muted),
                AlertCol::Price if on_price => {
                    Cell::num(format!("{:.*}", digits as usize, alert.price), alert.price)
                }
                AlertCol::Price if measure => Cell::num(
                    format!("{}{measure_unit}", alert.level_text(digits)),
                    alert.price,
                ),
                AlertCol::Price => Cell::dash(),
                AlertCol::Distance if measure => match current {
                    Some(now) => {
                        let gap = alert.price - now;
                        let decimals = alert.source.decimals(digits) as usize;
                        Cell::num(format!("{gap:+.decimals$}{measure_unit}"), gap).tone(Tone::Muted)
                    }
                    None => Cell::dash(),
                },
                AlertCol::Distance if !on_price => Cell::dash(),
                AlertCol::Distance => match market {
                    Some(m) => {
                        let pips = contract.pips(alert.price - m);
                        Cell::num(format!("{pips:+.1} pips"), pips).tone(Tone::Muted)
                    }
                    None => Cell::dash(),
                },
                AlertCol::State => Cell::text(state).tone(if alert.active {
                    Tone::Accent
                } else {
                    Tone::Muted
                }),
                AlertCol::Message => Cell::text(alert.message.clone()).tone(Tone::Muted),
                AlertCol::Repeats => Cell::text(alert.trigger.label()).tone(Tone::Muted),
                AlertCol::Created => {
                    if alert.created_at > 0 {
                        Cell::num(ctx.time(alert.created_at), alert.created_at as f64)
                    } else {
                        Cell::dash()
                    }
                }
                AlertCol::Fired => alert.fired_at.map_or_else(Cell::dash, |t| {
                    Cell::num(
                        format!("{} ({}x)", ctx.time(t), alert.fired_count),
                        t as f64,
                    )
                }),
            })
            .collect();
        rows.push(Row {
            key: format!("alert-{}", alert.id),
            symbol: Some(symbol),
            profit: None,
            kind: RowKind::Alert {
                id: alert.id,
                active: alert.active,
            },
            cells,
        });
    }
    finish(ctx, Tab::Alerts, &prefs.alerts, rows, unfiltered, None)
}

/// The times alerts fired, newest first.
pub(super) fn alert_log(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    for (n, firing) in ctx.alerts.book().history.iter().enumerate() {
        unfiltered += 1;
        let symbol = firing.symbol_id;
        if !ctx.keeps(
            symbol,
            None,
            &[
                &firing.symbol,
                &firing.watched,
                &firing.condition,
                &firing.text,
            ],
        ) {
            continue;
        }
        let digits = if symbol > 0 {
            ctx.alerts
                .digits
                .get(&symbol)
                .copied()
                .unwrap_or(account.book.contract(symbol).digits)
        } else {
            5
        };
        let cells = AlertLogCol::ALL
            .iter()
            .map(|col| match col {
                AlertLogCol::Time => Cell::num(ctx.time(firing.at), firing.at as f64),
                AlertLogCol::Symbol => Cell::text(firing.symbol.clone()),
                AlertLogCol::Watched if firing.watched.is_empty() => Cell::dash(),
                AlertLogCol::Watched => Cell::text(firing.watched.clone()).tone(Tone::Muted),
                AlertLogCol::Condition if firing.condition.is_empty() => Cell::dash(),
                AlertLogCol::Condition => Cell::text(firing.condition.clone()).tone(Tone::Muted),
                AlertLogCol::Value => {
                    Cell::opt(firing.value, |v| format!("{v:.*}", digits as usize))
                }
                AlertLogCol::Message => Cell::text(firing.text.clone()).tone(Tone::Muted),
            })
            .collect();
        rows.push(Row {
            key: format!("firing-{n}-{}", firing.at),
            symbol: (symbol > 0).then_some(symbol),
            profit: None,
            kind: RowKind::Firing {
                alert: firing.alert,
            },
            cells,
        });
    }
    finish(
        ctx,
        Tab::AlertLog,
        &ctx.prefs.alert_log,
        rows,
        unfiltered,
        None,
    )
}

/// A word with its first letter capital.
pub(super) fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
