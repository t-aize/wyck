//! The rows of the exposure tab: what is held, by symbol.

use super::{Cell, Ctx, Row, RowKind, Table, Tone, Unit, finish, price_cell};

use crate::app::account::Account;
use crate::app::prefs::columns::ExposureCol;
use crate::app::prefs::panel::Tab;
use crate::app::prefs::ticket::Slot;
use crate::domain::trading::math;
use crate::domain::trading::math::format_money;
use crate::domain::trading::{Position, money};
use crate::ui::features::trading::panel::stats::{self};
use std::collections::BTreeMap;

/// What the account holds on one symbol.
#[derive(Default)]
pub(super) struct Held {
    long: i64,
    short: i64,
    positions: usize,
    orders: usize,
    fills: Vec<(f64, i64)>,
    profit: Option<f64>,
    margin: f64,
}

pub(super) fn exposure(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let digits = account.book.money_digits();
    let mut held: BTreeMap<i64, Held> = BTreeMap::new();
    for position in account.book.positions.values() {
        let entry = held.entry(position.trade_data.symbol_id).or_default();
        add_position(entry, position, account, digits);
    }
    for order in account.book.orders.values() {
        held.entry(order.trade_data.symbol_id).or_default().orders += 1;
    }
    let unfiltered = held.len();
    let mut rows = Vec::new();
    for (symbol, held) in held {
        let name = account.book.name(symbol);
        if !ctx.keeps(symbol, None, &[&name]) {
            continue;
        }
        let contract = account.book.contract(symbol);
        let long = contract.lots_of_volume(held.long);
        let short = contract.lots_of_volume(held.short);
        let net = long - short;
        let average = stats::weighted_entry(&held.fills);
        let market = {
            let (bid, ask) = account.quote(symbol);
            match (bid, ask) {
                (Some(b), Some(a)) => Some((b + a) / 2.0),
                (b, a) => b.or(a),
            }
        };
        let cells = ExposureCol::ALL
            .iter()
            .map(|col| match col {
                ExposureCol::Symbol => Cell::text(name.clone()),
                ExposureCol::Net => Cell::num(format!("{net:+.2}"), net).tone(if net == 0.0 {
                    Tone::Plain
                } else if net > 0.0 {
                    Tone::Up
                } else {
                    Tone::Down
                }),
                ExposureCol::Long => Cell::num(math::format_lots(long), long)
                    .tone(Tone::Muted)
                    .adds(long, Unit::Lots),
                ExposureCol::Short => Cell::num(math::format_lots(short), short)
                    .tone(Tone::Muted)
                    .adds(short, Unit::Lots),
                ExposureCol::Positions => {
                    Cell::num(held.positions.to_string(), held.positions as f64)
                }
                ExposureCol::Orders => {
                    Cell::num(held.orders.to_string(), held.orders as f64).tone(Tone::Muted)
                }
                ExposureCol::Entry => price_cell(&contract, average),
                ExposureCol::Price => price_cell(&contract, market),
                ExposureCol::Margin => Cell::num(format_money(held.margin, ""), held.margin)
                    .adds(held.margin, Unit::Money),
                ExposureCol::Profit => match held.profit {
                    Some(p) => Cell::num(format_money(p, ctx.currency()), p)
                        .tone(Tone::of(p))
                        .adds(p, Unit::Money),
                    None => Cell::dash(),
                },
            })
            .collect();
        rows.push(Row {
            key: format!("exposure-{symbol}"),
            symbol: Some(symbol),
            profit: held.profit,
            kind: RowKind::Exposure,
            cells,
        });
    }
    finish(ctx, Tab::Exposure, &prefs.exposure, rows, unfiltered, None)
}

pub(super) fn add_position(
    held: &mut Held,
    position: &Position,
    account: &Account,
    digits: Option<u32>,
) {
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
    held.margin += position.used_margin.map_or(0.0, |m| money(m, digits));
}
