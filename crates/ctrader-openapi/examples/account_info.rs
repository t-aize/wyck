//! Prints what the account holds: balance, open positions, working orders and recent deals.
//!
//! ```text
//! cargo run -p ctrader-openapi --example account_info
//! cargo run -p ctrader-openapi --example account_info -- --days 30
//! ```
//!
//! Read only: nothing is changed on the account. `--days N` sets how far back the deals go
//! (default 7).
//!
//! Needs `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` and `WYCK_OPENAPI_ACCESS_TOKEN`.

#[path = "common/mod.rs"]
mod common;

use ctrader_openapi::account::volume_units;
use ctrader_openapi::market::SymbolTable;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let days: i64 = common::option(&args, "--days")
        .and_then(|d| d.parse().ok())
        .unwrap_or(7);

    let setup = common::setup().await;
    let table = SymbolTable::new(
        setup
            .account
            .symbols()
            .await
            .expect("cannot list the symbols"),
    );
    let name = |id: i64| {
        table
            .name_of(id)
            .map_or_else(|| id.to_string(), str::to_owned)
    };

    let trader = setup
        .account
        .trader()
        .await
        .expect("cannot read the account");
    println!("Account {}", trader.ctid_trader_account_id);
    println!(
        "  broker      {}",
        trader.broker_name.as_deref().unwrap_or("?")
    );
    println!("  balance     {:.2}", trader.balance_amount());
    if let Some(leverage) = trader.leverage() {
        println!("  leverage    1:{leverage:.0}");
    }
    if let Some(kind) = trader.kind() {
        println!("  books       {}", kind.label());
    }
    if let Some(rights) = trader.rights() {
        println!("  rights      {}", rights.label());
    }

    let (positions, orders) = setup
        .account
        .open_positions_and_orders(false)
        .await
        .expect("cannot read the positions");
    println!("\nOpen positions: {}", positions.len());
    for position in &positions {
        let side = position.trade_data.side().map_or("?", |s| s.label());
        println!(
            "  #{:<10} {:<10} {:<5} {:>12.2} units  at {}",
            position.position_id,
            name(position.trade_data.symbol_id),
            side,
            position.trade_data.units(),
            position.price.map_or("?".to_owned(), |p| p.to_string())
        );
    }
    println!("\nWorking orders: {}", orders.len());
    for order in &orders {
        println!(
            "  #{:<10} {:<10} {:<12} {:>12.2} units  {}",
            order.order_id,
            name(order.trade_data.symbol_id),
            order.kind().map_or("?", |k| k.label()),
            order.trade_data.units(),
            order.status().map_or("?", |s| s.label())
        );
    }

    let now = common::now_ms();
    let (deals, more) = setup
        .account
        .deals(now - days * 86_400_000, now, Some(50))
        .await
        .expect("cannot read the deals");
    println!(
        "\nDeals in the last {days} days: {}{}",
        deals.len(),
        if more { " (more exist)" } else { "" }
    );
    for deal in deals.iter().take(20) {
        println!(
            "  #{:<10} {:<10} {:<5} {:>12.2} units  at {}  {}",
            deal.deal_id,
            name(deal.symbol_id),
            deal.side().map_or("?", |s| s.label()),
            volume_units(deal.filled_volume),
            deal.execution_price
                .map_or("?".to_owned(), |p| p.to_string()),
            deal.status().map_or("?", |s| s.label())
        );
    }
    setup.client.close().await;
}
