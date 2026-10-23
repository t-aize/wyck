//! The rows of the positions tab.

use super::{
    Cell, Ctx, PositionRow, Row, RowKind, Table, Tone, Unit, finish, money_text, price_cell,
    side_cell,
};

use wyck_app::account::Busy;
use wyck_app::prefs::panel::Tab;
use wyck_domain::trading::contract;

/// The profit of a position, or a wait while the quote it depends on has not come.
fn profit_cell(profit: Option<f64>, currency: &str) -> Cell {
    let Some(profit) = profit else {
        return Cell::text("...").tone(Tone::Muted);
    };
    Cell::text(money_text(profit, currency))
        .tone(Tone::of(profit))
        .adds(profit, Unit::Money)
}

pub(super) fn positions(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let currency = ctx.currency();
    let mut rows = Vec::new();
    for position in account.book.positions.values() {
        let id = position.position_id;
        let symbol = position.trade_data.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = position.trade_data.is_buy();
        let name = account.book.name(symbol);
        let (bid, ask) = account.quote(symbol);
        let market = if buy { bid } else { ask };
        let lots = contract.lots_of_volume(position.trade_data.volume);
        let describe = format!(
            "{} {} {name}",
            if buy { "Buy" } else { "Sell" },
            contract::format_lots(lots)
        );
        let cells = vec![
            Cell::text(name),
            side_cell(buy),
            Cell::text(contract::format_lots(lots)).adds(lots, Unit::Lots),
            price_cell(&contract, position.price),
            price_cell(&contract, market),
            price_cell(&contract, position.stop_loss).tone(Tone::Muted),
            price_cell(&contract, position.take_profit).tone(Tone::Muted),
            profit_cell(account.net_profit(id), currency),
        ];
        rows.push(Row {
            key: format!("position-{id}"),
            symbol: Some(symbol),
            kind: RowKind::Position(PositionRow {
                id,
                busy: account.is_busy(Busy::Closing(id)) || account.is_busy(Busy::Amending(id)),
                describe,
            }),
            cells,
        });
    }
    finish(ctx, Tab::Positions, rows)
}
