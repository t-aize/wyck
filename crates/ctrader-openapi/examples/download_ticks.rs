//! Downloads the ticks of a symbol for a time range and writes them as CSV: one line per change,
//! with the bid and the ask side by side.
//!
//! ```text
//! cargo run -p ctrader-openapi --example download_ticks -- EURUSD --from 2h
//! cargo run -p ctrader-openapi --example download_ticks -- EURUSD --from 6h --to 1h --out eurusd.csv
//! ```
//!
//! `--from` and `--to` are `now`, a number of Unix milliseconds, or a time ago (`30m`, `6h`,
//! `2d`); the default range is the last hour. Without `--out` the CSV goes to the standard output
//! and the progress to the standard error.
//!
//! The columns are `time_ms,bid,ask`: prices are written with the symbol's decimals, and a side
//! that has not ticked yet is left empty. Bid and ask ticks are separate requests on the server, so
//! a long range takes a while (the limit is a few requests a second).
//!
//! Needs `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` and `WYCK_OPENAPI_ACCESS_TOKEN`.

#[path = "common/mod.rs"]
mod common;

use std::io::Write;

use ctrader_openapi::market::{SymbolTable, format_price};
use ctrader_openapi::types::{QuoteType, merge_sides};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = common::positional(&args).into_iter().next() else {
        eprintln!("Name a symbol, for example: download_ticks -- EURUSD --from 2h");
        std::process::exit(2);
    };
    let (from, to) = common::range(&args, 3_600_000).unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(2);
    });

    let setup = common::setup().await;
    let table = SymbolTable::new(
        setup
            .account
            .symbols()
            .await
            .expect("cannot list the symbols"),
    );
    let Some(symbol_id) = table.id_of(&name) else {
        eprintln!("{name}: no such symbol on this account");
        std::process::exit(2);
    };
    let digits = setup
        .account
        .symbol_details(&[symbol_id])
        .await
        .ok()
        .and_then(|d| d.first().map(|d| u32::try_from(d.digits).unwrap_or(5)))
        .unwrap_or(5);

    eprintln!("Downloading {name} ticks from {from} to {to} (Unix ms)...");
    let bids = setup
        .account
        .ticks(symbol_id, QuoteType::Bid, from, to)
        .await
        .expect("cannot download the bid ticks");
    eprintln!("  {} bid ticks", bids.len());
    let asks = setup
        .account
        .ticks(symbol_id, QuoteType::Ask, from, to)
        .await
        .expect("cannot download the ask ticks");
    eprintln!("  {} ask ticks", asks.len());
    let quotes = merge_sides(&bids, &asks);

    let mut out: Box<dyn Write> = match common::option(&args, "--out") {
        Some(path) => Box::new(std::io::BufWriter::new(
            std::fs::File::create(&path).unwrap_or_else(|e| {
                eprintln!("cannot write {path}: {e}");
                std::process::exit(2);
            }),
        )),
        None => Box::new(std::io::BufWriter::new(std::io::stdout().lock())),
    };
    writeln!(out, "time_ms,bid,ask").unwrap();
    for quote in &quotes {
        let side = |p: Option<i64>| p.map_or(String::new(), |p| format_price(p, digits));
        writeln!(
            out,
            "{},{},{}",
            quote.time_ms,
            side(quote.bid),
            side(quote.ask)
        )
        .unwrap();
    }
    out.flush().unwrap();
    eprintln!("{} quotes written.", quotes.len());
    setup.client.close().await;
}
