//! The rows of the orders tab and of the history of deals.

use super::{
    Cell, Ctx, Row, RowKind, Table, Tone, Unit, capitalized, finish, price_cell, side_cell,
};

use crate::app::account::Busy;
use crate::app::market_data::now_ms;
use crate::app::prefs::columns::{DealCol, OrderCol};
use crate::app::prefs::panel::{HistoryRange, Tab};
use crate::app::prefs::ticket::Slot;
use crate::domain::chart::zone::Zone;
use crate::domain::trading::math;
use crate::domain::trading::math::format_money;
use crate::domain::trading::{OrderType, money};
use crate::ui::features::trading::panel::stats::HistoryStats;
use serde_json::Value;

pub(super) fn orders(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    for order in account.book.orders.values() {
        unfiltered += 1;
        let id = order.order_id;
        let symbol = order.trade_data.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = order.trade_data.is_buy();
        let name = account.book.name(symbol);
        let kind = order.kind().map_or("order", OrderType::label);
        let kind = capitalized(kind);
        let comment = order
            .trade_data
            .comment
            .clone()
            .or_else(|| order.trade_data.label.clone())
            .unwrap_or_default();
        let id_text = id.to_string();
        if !ctx.keeps(
            symbol,
            Some(buy),
            &[
                &name,
                if buy { "buy" } else { "sell" },
                &kind,
                &id_text,
                &comment,
            ],
        ) {
            continue;
        }
        let price = order.limit_price.or(order.stop_price);
        let (bid, ask) = account.quote(symbol);
        let market = if buy { ask } else { bid };
        let lots = contract.lots_of_volume(order.trade_data.volume);
        let describe = format!(
            "{} {} {name} {}",
            if buy { "Buy" } else { "Sell" },
            math::format_lots(lots),
            kind.to_lowercase()
        );
        let cells = OrderCol::ALL
            .iter()
            .map(|col| match col {
                OrderCol::Symbol => Cell::text(name.clone()),
                OrderCol::Side => side_cell(buy, prefs.color_side),
                OrderCol::Kind => Cell::text(kind.clone()).tone(Tone::Muted),
                OrderCol::Lots => Cell::num(math::format_lots(lots), lots).adds(lots, Unit::Lots),
                OrderCol::Price => price_cell(&contract, price),
                OrderCol::Distance => match price.zip(market) {
                    Some((p, m)) => {
                        let pips = contract.pips((p - m).abs());
                        Cell::num(format!("{pips:.1} pips"), pips).tone(Tone::Muted)
                    }
                    None => Cell::dash(),
                },
                OrderCol::StopLoss => price_cell(&contract, order.stop_loss).tone(Tone::Muted),
                OrderCol::TakeProfit => price_cell(&contract, order.take_profit).tone(Tone::Muted),
                OrderCol::Expires => order.expiration_timestamp.map_or_else(
                    || Cell::text("Until cancelled").tone(Tone::Muted),
                    |t| Cell::num(ctx.time(t), t as f64),
                ),
                OrderCol::Created => order
                    .trade_data
                    .open_timestamp
                    .map_or_else(Cell::dash, |t| Cell::num(ctx.time(t), t as f64)),
                OrderCol::Id => Cell::num(id_text.clone(), id as f64).tone(Tone::Muted),
                OrderCol::Comment => Cell::text(comment.clone()).tone(Tone::Muted),
            })
            .collect();
        rows.push(Row {
            key: format!("order-{id}"),
            symbol: Some(symbol),
            profit: None,
            kind: RowKind::Order {
                id,
                busy: account.is_busy(Busy::Cancelling(id)) || account.is_busy(Busy::Amending(id)),
                describe,
            },
            cells,
        });
    }
    finish(ctx, Tab::Orders, &prefs.orders, rows, unfiltered, None)
}

/// A number the server put in the details of a closing deal, scaled by the money digits.
pub(super) fn detail_money(detail: &Value, key: &str, digits: Option<u32>) -> Option<f64> {
    detail
        .get(key)
        .and_then(Value::as_i64)
        .map(|v| money(v, digits))
}

pub(super) fn history(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let now = now_ms();
    let since = match prefs.history_range {
        // The start of today, in the zone the user reads.
        HistoryRange::Today => {
            let day = Zone::Local.day(now);
            day * 86_400_000 - Zone::Local.offset_ms(now)
        }
        HistoryRange::Day => now - 86_400_000,
        HistoryRange::ThreeDays => now - 3 * 86_400_000,
        HistoryRange::Week => now - 7 * 86_400_000,
    };
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    let mut results = Vec::new();
    for deal in &account.book.deals {
        if deal.execution_timestamp < since {
            continue;
        }
        let closing = deal.close_position_detail.is_some();
        if !closing && !prefs.history_opening {
            continue;
        }
        unfiltered += 1;
        let symbol = deal.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = deal.is_buy();
        let name = account.book.name(symbol);
        let kind = if closing { "Close" } else { "Open" };
        let digits = deal.money_digits.and_then(|d| u32::try_from(d).ok());
        let detail = deal.close_position_detail.as_ref();
        let gross = detail.and_then(|d| detail_money(d, "grossProfit", digits));
        let swap = detail.and_then(|d| detail_money(d, "swap", digits));
        let commission = detail
            .and_then(|d| detail_money(d, "commission", digits))
            .or_else(|| deal.commission.map(|c| money(c, digits)));
        let net = gross.map(|g| g + swap.unwrap_or(0.0) + commission.unwrap_or(0.0));
        let balance = detail.and_then(|d| detail_money(d, "balance", digits));
        let entry = detail
            .and_then(|d| d.get("entryPrice"))
            .and_then(Value::as_f64);
        let id_text = deal.deal_id.to_string();
        if !ctx.keeps(
            symbol,
            Some(buy),
            &[
                &name,
                if buy { "buy" } else { "sell" },
                kind,
                &id_text,
                &deal.position_id.to_string(),
            ],
        ) {
            continue;
        }
        if let Some(net) = net {
            results.push(net);
        }
        // A closing deal is on the other side of the position it closes.
        let pips = entry.zip(deal.execution_price).map(|(e, p)| {
            let sign = if buy { -1.0 } else { 1.0 };
            contract.pips((p - e) * sign)
        });
        let lots = contract.lots_of_volume(deal.filled_volume);
        let cells = DealCol::ALL
            .iter()
            .map(|col| match col {
                DealCol::Time => Cell::num(
                    ctx.time(deal.execution_timestamp),
                    deal.execution_timestamp as f64,
                )
                .tone(Tone::Muted),
                DealCol::Symbol => Cell::text(name.clone()),
                DealCol::Side => side_cell(buy, prefs.color_side),
                DealCol::Kind => Cell::text(kind).tone(Tone::Muted),
                DealCol::Lots => Cell::num(math::format_lots(lots), lots).adds(lots, Unit::Lots),
                DealCol::Entry => price_cell(&contract, entry),
                DealCol::Price => price_cell(&contract, deal.execution_price),
                DealCol::Pips => match pips {
                    Some(p) => Cell::num(format!("{p:+.1}"), p).tone(Tone::of(p)),
                    None => Cell::dash(),
                },
                DealCol::Commission => {
                    let cell = Cell::opt(commission, |c| format_money(c, "")).tone(Tone::Muted);
                    match commission {
                        Some(c) => cell.adds(c, Unit::Money),
                        None => cell,
                    }
                }
                DealCol::Swap => {
                    let cell = Cell::opt(swap, |s| format_money(s, "")).tone(Tone::Muted);
                    match swap {
                        Some(s) => cell.adds(s, Unit::Money),
                        None => cell,
                    }
                }
                DealCol::Gross => {
                    let cell = Cell::opt(gross, |g| format_money(g, "")).tone(Tone::Muted);
                    match gross {
                        Some(g) => cell.adds(g, Unit::Money),
                        None => cell,
                    }
                }
                DealCol::Profit => match net {
                    Some(n) => Cell::num(format_money(n, ctx.currency()), n)
                        .tone(Tone::of(n))
                        .adds(n, Unit::Money),
                    None => Cell::dash(),
                },
                DealCol::Balance => Cell::opt(balance, |b| format_money(b, "")).tone(Tone::Muted),
                DealCol::Id => Cell::num(id_text.clone(), deal.deal_id as f64).tone(Tone::Muted),
                DealCol::Order => {
                    Cell::num(deal.order_id.to_string(), deal.order_id as f64).tone(Tone::Muted)
                }
                DealCol::Position => {
                    Cell::num(deal.position_id.to_string(), deal.position_id as f64)
                        .tone(Tone::Muted)
                }
            })
            .collect();
        rows.push(Row {
            key: format!("deal-{}", deal.deal_id),
            symbol: Some(symbol),
            profit: net,
            kind: RowKind::Deal,
            cells,
        });
    }
    // Newest first, unless the user sorts.
    let stats = (prefs.history_stats && !results.is_empty()).then(|| HistoryStats::of(&results));
    finish(ctx, Tab::History, &prefs.history, rows, unfiltered, stats)
}
