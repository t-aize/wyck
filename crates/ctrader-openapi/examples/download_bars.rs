//! Downloads the bars of a symbol at a period for a time range and writes them as CSV.
//!
//! ```text
//! cargo run -p ctrader-openapi --example download_bars -- EURUSD M15 --from 2d
//! cargo run -p ctrader-openapi --example download_bars -- XAUUSD H1 --from 30d --out gold.csv
//! ```
//!
//! The period is one of `M1 M2 M3 M4 M5 M10 M15 M30 H1 H4 H12 D1 W1 MN1`. `--from` and `--to` are
//! `now`, a number of Unix milliseconds, or a time ago (`30m`, `6h`, `2d`); the default range is the
//! last day. Without `--out` the CSV goes to the standard output.
//!
//! The columns are `time_ms,open,high,low,close,ticks`: the volume of a bar is a **count of
//! ticks**, not a traded amount, and it counts changes of both the bid and the ask.
//!
//! Needs `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` and `WYCK_OPENAPI_ACCESS_TOKEN`.

#[path = "common/mod.rs"]
mod common;

use std::io::Write;

use ctrader_openapi::market::{SymbolTable, format_price};
use ctrader_openapi::types::Period;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words = common::positional(&args);
    let (Some(name), Some(period_text)) = (words.first(), words.get(1)) else {
        eprintln!("Usage: download_bars -- SYMBOL PERIOD [--from 2d] [--to now] [--out file.csv]");
        std::process::exit(2);
    };
    let Some(period) = Period::ALL
        .into_iter()
        .find(|p| p.label().eq_ignore_ascii_case(period_text))
    else {
        eprintln!("{period_text}: not a period (M1 M5 M15 M30 H1 H4 D1 W1 MN1 ...)");
        std::process::exit(2);
    };
    let (from, to) = common::range(&args, 86_400_000).unwrap_or_else(|message| {
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
    let Some(symbol_id) = table.id_of(name) else {
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

    eprintln!(
        "Downloading {name} {} bars from {from} to {to} (Unix ms)...",
        period.label()
    );
    let bars = setup
        .account
        .bars(symbol_id, period, from, to)
        .await
        .expect("cannot download the bars");

    let mut out: Box<dyn Write> = match common::option(&args, "--out") {
        Some(path) => Box::new(std::io::BufWriter::new(
            std::fs::File::create(&path).unwrap_or_else(|e| {
                eprintln!("cannot write {path}: {e}");
                std::process::exit(2);
            }),
        )),
        None => Box::new(std::io::BufWriter::new(std::io::stdout().lock())),
    };
    writeln!(out, "time_ms,open,high,low,close,ticks").unwrap();
    for bar in &bars {
        writeln!(
            out,
            "{},{},{},{},{},{}",
            bar.time_ms,
            format_price(bar.open, digits),
            format_price(bar.high, digits),
            format_price(bar.low, digits),
            format_price(bar.close, digits),
            bar.volume
        )
        .unwrap();
    }
    out.flush().unwrap();
    eprintln!("{} bars written.", bars.len());
    setup.client.close().await;
}
