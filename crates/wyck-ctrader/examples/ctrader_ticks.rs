//! Measures what a broker keeps in ticks, to set the constants of the replay: how far back ticks
//! go, how many a busy hour holds, and how long a request takes.
//!
//! ```sh
//! export WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=...
//! export WYCK_OPENAPI_ACCESS_TOKEN=... WYCK_OPENAPI_ACCOUNT_ID=...
//! cargo run -p wyck-ctrader --example ctrader_ticks -- US100.cash
//! ```
//!
//! It only reads. Each probe asks the bid ticks of ten minutes at noon UTC of a weekday, going back
//! 1, 2, 3, 7, 14, 30, 60, 90, 180 and 365 days; the oldest probe that finds ticks is the
//! retention. Then it reads one hour of today (when the market is open) and prints the ticks it
//! holds, the time the request took and the rate this gives per day.

#![expect(clippy::print_stdout, reason = "an example talks to the terminal")]

use std::time::{Duration, Instant};

use wyck_ctrader::{ClientBuilder, ClientCredentials, Environment, Error};
use wyck_market::QuoteType;

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
const PROBES_BACK_DAYS: [i64; 10] = [1, 2, 3, 7, 14, 30, 60, 90, 180, 365];

fn var(name: &str) -> wyck_ctrader::Result<String> {
    // A `.env` file is optional: the variables may already be in the environment.
    dotenvy::dotenv().ok();
    std::env::var(name).map_err(|_| Error::Config(format!("set {name}")))
}

/// Noon UTC of the weekday on or before `days` days back from `now_ms`. The Unix epoch was a
/// Thursday, so day 2 and day 3 of the week (counting from it) are Saturday and Sunday.
fn noon_of_weekday(now_ms: i64, days: i64) -> i64 {
    let mut day = now_ms.div_euclid(DAY_MS) - days;
    match day.rem_euclid(7) {
        2 => day -= 1,
        3 => day -= 2,
        _ => {}
    }
    day * DAY_MS + 12 * HOUR_MS
}

#[tokio::main]
async fn main() -> wyck_ctrader::Result<()> {
    let name = std::env::args().nth(1).unwrap_or_else(|| "EURUSD".into());
    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID")?,
        var("WYCK_OPENAPI_CLIENT_SECRET")?,
    );
    let client = ClientBuilder::new(Environment::Demo)
        .credentials(credentials)
        .connect()
        .await?;
    let account_id: i64 = var("WYCK_OPENAPI_ACCOUNT_ID")?
        .parse()
        .map_err(|_| Error::Config("WYCK_OPENAPI_ACCOUNT_ID is not a number".into()))?;
    let account = client.account(account_id);
    account
        .authorize(&var("WYCK_OPENAPI_ACCESS_TOKEN")?)
        .await?;

    let market = account.market();
    let symbol = market
        .symbols()
        .await?
        .into_iter()
        .find(|s| s.symbol_name.as_deref() == Some(name.as_str()))
        .ok_or_else(|| Error::Config(format!("no symbol named {name}")))?;

    let now_ms = wyck_clock::unix_ms();
    println!("ten minutes of bid ticks of {name} at noon UTC, going back:");
    let mut oldest_with_ticks = None;
    for days in PROBES_BACK_DAYS {
        let from = noon_of_weekday(now_ms, days);
        let started = Instant::now();
        let ticks = market
            .ticks(
                symbol.symbol_id,
                QuoteType::Bid,
                from,
                from + 10 * MINUTE_MS,
            )
            .await?;
        println!(
            "  {days:>3} days back: {:>7} ticks in {:>5} ms",
            ticks.len(),
            started.elapsed().as_millis()
        );
        if !ticks.is_empty() {
            oldest_with_ticks = Some(days);
        }
    }
    match oldest_with_ticks {
        Some(days) => println!("ticks go back at least {days} days"),
        None => println!("no ticks found in any probe"),
    }

    let from = noon_of_weekday(now_ms, 1);
    let started = Instant::now();
    let hour = market
        .ticks(symbol.symbol_id, QuoteType::Bid, from, from + HOUR_MS)
        .await?;
    let took: Duration = started.elapsed();
    println!(
        "one busy hour (yesterday 12:00 to 13:00 UTC): {} ticks in {} ms, about {} ticks a day",
        hour.len(),
        took.as_millis(),
        hour.len() * 24
    );
    client.close();
    Ok(())
}
