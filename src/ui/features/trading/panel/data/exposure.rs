//! The rows of the exposure tab: what is held, by symbol.

use super::{Cell, Ctx, Row, RowKind, Table, Tone, Unit, finish, money_text, price_cell};

use crate::app::account::Account;
use crate::app::prefs::panel::Tab;
use crate::domain::trading::Position;
use crate::domain::trading::math;
use crate::ui::features::trading::panel::stats;
use std::collections::BTreeMap;

/// What the account holds on one symbol.
#[derive(Default)]
pub(super) struct Held {
    long: i64,
    short: i64,
    positions: usize,
    fills: Vec<(f64, i64)>,
    profit: Option<f64>,
}

pub(super) fn exposure(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let mut held: BTreeMap<i64, Held> = BTreeMap::new();
    for position in account.book.positions.values() {
        let entry = held.entry(position.trade_data.symbol_id).or_default();
        add_position(entry, position, account);
    }
    let mut rows = Vec::new();
    for (symbol, held) in held {
        let contract = account.book.contract(symbol);
        let long = contract.lots_of_volume(held.long);
        let short = contract.lots_of_volume(held.short);
        let net = long - short;
        let cells = vec![
            Cell::text(account.book.name(symbol)),
            Cell::text(format!("{net:+.2}")).tone(if net == 0.0 {
                Tone::Plain
            } else if net > 0.0 {
                Tone::Up
            } else {
                Tone::Down
            }),
            Cell::text(math::format_lots(long))
                .tone(Tone::Muted)
                .adds(long, Unit::Lots),
            Cell::text(math::format_lots(short))
                .tone(Tone::Muted)
                .adds(short, Unit::Lots),
            Cell::text(held.positions.to_string()),
            price_cell(&contract, stats::weighted_entry(&held.fills)),
            match held.profit {
                Some(p) => Cell::text(money_text(p, ctx.currency()))
                    .tone(Tone::of(p))
                    .adds(p, Unit::Money),
                None => Cell::dash(),
            },
        ];
        rows.push(Row {
            key: format!("exposure-{symbol}"),
            symbol: Some(symbol),
            kind: RowKind::Plain,
            cells,
        });
    }
    finish(ctx, Tab::Exposure, rows)
}

pub(super) fn add_position(held: &mut Held, position: &Position, account: &Account) {
    let volume = position.trade_data.volume;
    if position.trade_data.is_buy() {
        held.long += volume;
    } else {
        held.short += volume;
    }
    held.positions += 1;
    if let Some(price) = position.price {
        held.fills.push((price, volume));
    }
    if let Some(profit) = account.net_profit(position.position_id) {
        *held.profit.get_or_insert(0.0) += profit;
    }
}
