//! The rows of the positions tab.

use super::{
    Cell, Ctx, PositionRow, Row, RowKind, Table, Tone, Unit, finish, pips_cell, price_cell,
    side_cell,
};

use crate::app::account::Busy;
use crate::app::market_data::now_ms;
use crate::app::prefs::columns::PositionCol;
use crate::app::prefs::panel::{ProfitUnit, Tab};
use crate::app::prefs::ticket::Slot;
use crate::domain::trading::math;
use crate::domain::trading::math::format_money;
use crate::domain::trading::money;
use crate::ui::features::trading::panel::stats::{self};

/// The profit of a position written in the unit the user chose.
pub(super) fn profit_cell(
    unit: ProfitUnit,
    profit: Option<f64>,
    pips: Option<f64>,
    balance: f64,
    currency: &str,
) -> Cell {
    let Some(profit) = profit else {
        return Cell::text("...").tone(Tone::Muted);
    };
    let text = match unit {
        ProfitUnit::Money => format_money(profit, currency),
        ProfitUnit::Pips => pips.map_or_else(|| "-".to_owned(), |p| format!("{p:+.1} pips")),
        ProfitUnit::Percent if balance > 0.0 => format!("{:+.2}%", profit / balance * 100.0),
        ProfitUnit::Percent => "-".to_owned(),
        ProfitUnit::MoneyPips => match pips {
            Some(p) => format!("{} ({p:+.1} pips)", format_money(profit, currency)),
            None => format_money(profit, currency),
        },
    };
    Cell::num(text, profit)
        .tone(Tone::of(profit))
        .adds(profit, Unit::Money)
}

pub(super) fn positions(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let currency = ctx.currency();
    let digits = account.book.money_digits();
    let balance = account.summary().balance;
    let now = now_ms();
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    for position in account.book.positions.values() {
        unfiltered += 1;
        let id = position.position_id;
        let symbol = position.trade_data.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = position.trade_data.is_buy();
        let name = account.book.name(symbol);
        let comment = position
            .trade_data
            .comment
            .clone()
            .or_else(|| position.trade_data.label.clone())
            .unwrap_or_default();
        let id_text = id.to_string();
        if !ctx.keeps(
            symbol,
            Some(buy),
            &[&name, if buy { "buy" } else { "sell" }, &id_text, &comment],
        ) {
            continue;
        }
        let (bid, ask) = account.quote(symbol);
        let market = if buy { bid } else { ask };
        // The direction a gain goes: up for a buy, down for a sell.
        let sign = if buy { 1.0 } else { -1.0 };
        let entry = position.price;
        let move_ = entry.zip(market).map(|(e, m)| (m - e) * sign);
        let pips = move_.map(|d| contract.pips(d));
        let profit = account.net_profit(id);
        let units = position.trade_data.units();
        let risk = entry
            .zip(position.stop_loss)
            .zip(account.rate(symbol))
            .map(|((e, s), rate)| (e - s).abs() * units * rate);
        let lots = contract.lots_of_volume(position.trade_data.volume);
        let step = contract.step_volume.max(1);
        let half = position.trade_data.volume / 2 / step * step;
        let half = (half >= contract.min_volume && half > 0 && half < position.trade_data.volume)
            .then_some(half);
        let describe = format!(
            "{} {} {name}",
            if buy { "Buy" } else { "Sell" },
            math::format_lots(lots)
        );
        let cells = PositionCol::ALL
            .iter()
            .map(|col| match col {
                PositionCol::Symbol => Cell::text(name.clone()),
                PositionCol::Side => side_cell(buy, prefs.color_side),
                PositionCol::Lots => {
                    Cell::num(math::format_lots(lots), lots).adds(lots, Unit::Lots)
                }
                PositionCol::Entry => price_cell(&contract, entry),
                PositionCol::Price => price_cell(&contract, market),
                PositionCol::StopLoss => {
                    price_cell(&contract, position.stop_loss).tone(Tone::Muted)
                }
                PositionCol::TakeProfit => {
                    price_cell(&contract, position.take_profit).tone(Tone::Muted)
                }
                PositionCol::SlPips => pips_cell(
                    &contract,
                    position.stop_loss.zip(market).map(|(s, m)| (m - s) * sign),
                ),
                PositionCol::TpPips => pips_cell(
                    &contract,
                    position
                        .take_profit
                        .zip(market)
                        .map(|(t, m)| (t - m) * sign),
                ),
                PositionCol::Pips => match pips {
                    Some(p) => Cell::num(format!("{p:+.1}"), p).tone(Tone::of(p)),
                    None => Cell::dash(),
                },
                PositionCol::Swap => Cell::opt(position.swap.map(|s| money(s, digits)), |s| {
                    format_money(s, "")
                })
                .tone(Tone::Muted),
                PositionCol::Commission => {
                    let value = position.commission.map(|c| money(c, digits));
                    let cell = Cell::opt(value, |c| format_money(c, "")).tone(Tone::Muted);
                    match value {
                        Some(v) => cell.adds(v, Unit::Money),
                        None => cell,
                    }
                }
                PositionCol::Margin => {
                    let value = position.used_margin.map(|m| money(m, digits));
                    let cell = Cell::opt(value, |m| format_money(m, ""));
                    match value {
                        Some(v) => cell.adds(v, Unit::Money),
                        None => cell,
                    }
                }
                PositionCol::Risk => {
                    let cell = Cell::opt(risk, |r| format_money(r, ""));
                    match risk {
                        Some(r) => cell.adds(r, Unit::Money),
                        None => cell,
                    }
                }
                PositionCol::Profit => {
                    profit_cell(prefs.profit_unit, profit, pips, balance, currency)
                }
                PositionCol::ProfitPercent => match profit {
                    Some(p) if balance > 0.0 => {
                        let share = p / balance * 100.0;
                        Cell::num(format!("{share:+.2}%"), share)
                            .tone(Tone::of(p))
                            .adds(share, Unit::Percent)
                    }
                    _ => Cell::dash(),
                },
                PositionCol::RMultiple => match profit.zip(risk).filter(|(_, r)| *r > 0.0) {
                    Some((p, r)) => Cell::num(format!("{:+.2}R", p / r), p / r).tone(Tone::of(p)),
                    None => Cell::dash(),
                },
                PositionCol::Opened => position
                    .trade_data
                    .open_timestamp
                    .map_or_else(Cell::dash, |t| Cell::num(ctx.time(t), t as f64)),
                PositionCol::Age => position
                    .trade_data
                    .open_timestamp
                    .map_or_else(Cell::dash, |t| {
                        Cell::num(stats::duration_text(now - t), (now - t) as f64)
                    }),
                PositionCol::Id => Cell::num(id_text.clone(), id as f64).tone(Tone::Muted),
                PositionCol::Comment => Cell::text(comment.clone()).tone(Tone::Muted),
            })
            .collect();
        rows.push(Row {
            key: format!("position-{id}"),
            symbol: Some(symbol),
            profit,
            kind: RowKind::Position(PositionRow {
                id,
                half,
                entry,
                stop_loss: position.stop_loss,
                take_profit: position.take_profit,
                trailing: position.trailing_stop_loss.unwrap_or(false),
                busy: account.is_busy(Busy::Closing(id)) || account.is_busy(Busy::Amending(id)),
                describe,
            }),
            cells,
        });
    }
    finish(
        ctx,
        Tab::Positions,
        &prefs.positions,
        rows,
        unfiltered,
        None,
    )
}
