//! Lists the symbols of the account, with their ids, decimals and description.
//!
//! ```text
//! cargo run -p ctrader-openapi --example list_symbols            # all of them
//! cargo run -p ctrader-openapi --example list_symbols -- EUR     # names starting with EUR
//! ```
//!
//! Needs `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` and `WYCK_OPENAPI_ACCESS_TOKEN`
//! (run the `sign_in` example to get a token). See `common/mod.rs` for the optional variables.

#[path = "common/mod.rs"]
mod common;

use ctrader_openapi::market::SymbolTable;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let prefix = common::positional(&args).into_iter().next();

    let setup = common::setup().await;
    let symbols = setup
        .account
        .symbols()
        .await
        .expect("cannot list the symbols");
    let table = SymbolTable::new(symbols);
    let chosen: Vec<_> = match &prefix {
        Some(prefix) => table.starting_with(prefix),
        None => table.iter().collect(),
    };

    // The details (decimals, pip position) are one request for up to a few hundred symbols; only
    // ask for the ones being shown.
    let ids: Vec<i64> = chosen.iter().map(|s| s.symbol_id).take(200).collect();
    let details = if ids.is_empty() {
        Vec::new()
    } else {
        setup
            .account
            .symbol_details(&ids)
            .await
            .expect("cannot read the symbol details")
    };

    println!("{:>8}  {:<14} {:>6}  description", "id", "name", "digits");
    for symbol in chosen.iter().take(200) {
        let digits = details
            .iter()
            .find(|d| d.symbol_id == symbol.symbol_id)
            .map_or_else(|| "-".to_owned(), |d| d.digits.to_string());
        println!(
            "{:>8}  {:<14} {:>6}  {}",
            symbol.symbol_id,
            symbol.symbol_name.as_deref().unwrap_or("?"),
            digits,
            symbol.description.as_deref().unwrap_or("")
        );
    }
    if chosen.len() > 200 {
        println!(
            "... and {} more; narrow it with a prefix",
            chosen.len() - 200
        );
    }
    println!(
        "{} symbols listed of {}",
        chosen.len().min(200),
        table.len()
    );
}
