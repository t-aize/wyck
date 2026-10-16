//! The rows of the orders tab and of the history of closed trades.

use super::alerts::capitalized;
use super::{
    Cell, Ctx, Row, RowKind, Table, Tone, Unit, finish, money_text, price_cell, side_cell,
};

use serde_json::Value;
use wyck_app::account::Busy;
use wyck_app::market_data::now_ms;
use wyck_app::prefs::panel::Tab;
use wyck_trading::math;
use wyck_trading::{OrderType, money};

/// How far back the history reaches, in days.
const HISTORY_DAYS: i64 = 7;

pub(super) fn orders(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let mut rows = Vec::new();
    for order in account.book.orders.values() {
        let id = order.order_id;
        let symbol = order.trade_data.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = order.trade_data.is_buy();
        let name = account.book.name(symbol);
        let kind = capitalized(order.kind().map_or("order", OrderType::label));
        let price = order.limit_price.or(order.stop_price);
        let lots = contract.lots_of_volume(order.trade_data.volume);
        let describe = format!(
            "{} {} {name} {}",
            if buy { "Buy" } else { "Sell" },
            math::format_lots(lots),
            kind.to_lowercase()
        );
        let cells = vec![
            Cell::text(name),
            side_cell(buy),
            Cell::text(kind).tone(Tone::Muted),
            Cell::text(math::format_lots(lots)).adds(lots, Unit::Lots),
            price_cell(&contract, price),
            price_cell(&contract, order.stop_loss).tone(Tone::Muted),
            price_cell(&contract, order.take_profit).tone(Tone::Muted),
        ];
        rows.push(Row {
            key: format!("order-{id}"),
            symbol: Some(symbol),
            kind: RowKind::Order {
                id,
                busy: account.is_busy(Busy::Cancelling(id)) || account.is_busy(Busy::Amending(id)),
                describe,
            },
            cells,
        });
    }
    finish(ctx, Tab::Orders, rows)
}

/// A number the server put in the details of a closing deal, scaled by the money digits.
fn detail_money(detail: &Value, key: &str, digits: Option<u32>) -> Option<f64> {
    detail
        .get(key)
        .and_then(Value::as_i64)
        .map(|v| money(v, digits))
}

/// The trades closed in the last days, one row for each closing deal.
pub(super) fn history(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let since = now_ms() - HISTORY_DAYS * 86_400_000;
    let mut rows = Vec::new();
    for deal in &account.book.deals {
        let Some(detail) = deal.close_position_detail.as_ref() else {
            continue;
        };
        if deal.execution_timestamp < since {
            continue;
        }
        let symbol = deal.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = deal.is_buy();
        let digits = deal.money_digits.and_then(|d| u32::try_from(d).ok());
        let gross = detail_money(detail, "grossProfit", digits);
        let swap = detail_money(detail, "swap", digits);
        let commission = detail_money(detail, "commission", digits)
            .or_else(|| deal.commission.map(|c| money(c, digits)));
        let net = gross.map(|g| g + swap.unwrap_or(0.0) + commission.unwrap_or(0.0));
        let entry = detail.get("entryPrice").and_then(Value::as_f64);
        let lots = contract.lots_of_volume(deal.filled_volume);
        // A closing deal is on the other side of the position it closes.
        let cells = vec![
            Cell::text(Ctx::time(deal.execution_timestamp)).tone(Tone::Muted),
            Cell::text(account.book.name(symbol)),
            side_cell(!buy),
            Cell::text(math::format_lots(lots)).adds(lots, Unit::Lots),
            price_cell(&contract, entry),
            price_cell(&contract, deal.execution_price),
            match net {
                Some(n) => Cell::text(money_text(n, ctx.currency()))
                    .tone(Tone::of(n))
                    .adds(n, Unit::Money),
                None => Cell::dash(),
            },
        ];
        rows.push(Row {
            key: format!("deal-{}", deal.deal_id),
            symbol: Some(symbol),
            kind: RowKind::Plain,
            cells,
        });
    }
    finish(ctx, Tab::History, rows)
}
