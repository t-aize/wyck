//! Streams live prices for one or more symbols, with the spread, for a while.
//!
//! ```text
//! cargo run -p ctrader-openapi --example stream_prices -- EURUSD GBPUSD
//! cargo run -p ctrader-openapi --example stream_prices -- EURUSD --seconds 60 --depth
//! ```
//!
//! `--seconds N` sets how long to listen (default 30). `--depth` also follows the order book of
//! the symbols and prints its best levels when the broker offers one.
//!
//! Needs `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` and `WYCK_OPENAPI_ACCESS_TOKEN`.
//! With the market closed only one price per symbol arrives (the last one).

#[path = "common/mod.rs"]
mod common;

use std::collections::HashMap;
use std::time::Duration;

use ctrader_openapi::market::{DepthBook, SpotTracker, SymbolTable, format_price};
use ctrader_openapi::{ConnectionState, Event};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names = common::positional(&args);
    if names.is_empty() {
        eprintln!("Name at least one symbol, for example: stream_prices -- EURUSD");
        std::process::exit(2);
    }
    let seconds: u64 = common::option(&args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let with_depth = args.iter().any(|a| a == "--depth");

    let setup = common::setup().await;
    let table = SymbolTable::new(
        setup
            .account
            .symbols()
            .await
            .expect("cannot list the symbols"),
    );
    let mut ids = Vec::new();
    for name in &names {
        match table.id_of(name) {
            Some(id) => ids.push(id),
            None => eprintln!("{name}: no such symbol on this account"),
        }
    }
    if ids.is_empty() {
        std::process::exit(2);
    }

    // The decimals of each symbol, so prices are shown the way the broker quotes them.
    let digits: HashMap<i64, u32> = setup
        .account
        .symbol_details(&ids)
        .await
        .expect("cannot read the symbol details")
        .into_iter()
        .map(|d| (d.symbol_id, u32::try_from(d.digits).unwrap_or(5)))
        .collect();

    // Subscribe after asking for the events, so the first price is not missed.
    let mut events = setup.client.events();
    setup
        .account
        .subscribe_spots(&ids)
        .await
        .expect("cannot follow the prices");
    if with_depth && let Err(error) = setup.account.subscribe_depth(&ids).await {
        eprintln!("no order book: {error}");
    }

    let mut tracker = SpotTracker::new();
    let mut books: HashMap<i64, DepthBook> = HashMap::new();
    let mut count = 0u64;
    let deadline = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(deadline);
    println!(
        "{:<10} {:>10} {:>10} {:>8}",
        "symbol", "bid", "ask", "spread"
    );
    loop {
        tokio::select! {
            () = &mut deadline => break,
            event = events.recv() => match event {
                Ok(Event::Spot(spot)) => {
                    let quote = tracker.apply(&spot);
                    let places = digits.get(&quote.symbol_id).copied().unwrap_or(5);
                    let show = |price: Option<i64>| price.map_or("-".to_owned(), |p| format_price(p, places));
                    let spread = match (quote.bid, quote.ask) {
                        (Some(bid), Some(ask)) => format_price(ask - bid, places),
                        _ => "-".to_owned(),
                    };
                    count += 1;
                    println!(
                        "{:<10} {:>10} {:>10} {:>8}",
                        table.name_of(quote.symbol_id).unwrap_or("?"),
                        show(quote.bid),
                        show(quote.ask),
                        spread
                    );
                }
                Ok(Event::Depth(update)) => {
                    let book = books.entry(update.symbol_id).or_default();
                    book.apply(&update);
                    let places = digits.get(&update.symbol_id).copied().unwrap_or(5);
                    if let (Some(bid), Some(ask)) = (book.best_bid(), book.best_ask()) {
                        println!(
                            "  book {:<8} {} x {}   |   {} x {}",
                            table.name_of(update.symbol_id).unwrap_or("?"),
                            format_price(bid.price, places),
                            bid.size / 100,
                            format_price(ask.price, places),
                            ask.size / 100
                        );
                    }
                }
                Ok(Event::Disconnected(reason)) => {
                    eprintln!("The connection ended: {reason:?}. (The resilient_stream example reconnects.)");
                    break;
                }
                Ok(_) => {}
                Err(error) => eprintln!("(the reader fell behind: {error})"),
            }
        }
    }
    println!("{count} price events in {seconds} seconds");
    if matches!(*setup.client.state().borrow(), ConnectionState::Connected) {
        setup.client.close().await;
    }
}
