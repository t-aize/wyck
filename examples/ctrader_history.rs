//! Downloads the last day of one-minute bars of a symbol and prints the newest few.
//!
//! ```sh
//! export WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=...
//! export WYCK_OPENAPI_ACCESS_TOKEN=... WYCK_OPENAPI_ACCOUNT_ID=...
//! cargo run --example ctrader_history -- EURUSD
//! ```
//!
//! [`MarketClient::bars`](wyck::infra::ctrader::market::MarketClient::bars) pages through ranges longer
//! than one request allows and keeps under the history rate limit by itself.

use std::time::{SystemTime, UNIX_EPOCH};

use wyck::infra::ctrader::market::{Period, to_price};
use wyck::infra::ctrader::{ClientBuilder, ClientCredentials, Environment, Error};

fn var(name: &str) -> String {
    let _ = dotenvy::dotenv();
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

#[tokio::main]
async fn main() -> wyck::infra::ctrader::Result<()> {
    let name = std::env::args().nth(1).unwrap_or_else(|| "EURUSD".into());
    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID"),
        var("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let client = ClientBuilder::new(Environment::Demo)
        .credentials(credentials)
        .connect()
        .await?;
    let account_id: i64 = var("WYCK_OPENAPI_ACCOUNT_ID")
        .parse()
        .map_err(|_| Error::Config("WYCK_OPENAPI_ACCOUNT_ID is not a number".into()))?;
    let account = client.account(account_id);
    account.authorize(&var("WYCK_OPENAPI_ACCESS_TOKEN")).await?;

    let market = account.market();
    let symbol = market
        .symbols()
        .await?
        .into_iter()
        .find(|s| s.symbol_name.as_deref() == Some(name.as_str()))
        .ok_or_else(|| Error::Config(format!("no symbol named {name}")))?;

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    let day_ms = 24 * 60 * 60 * 1_000;
    let bars = market
        .bars(symbol.symbol_id, Period::M1, now_ms - day_ms, now_ms)
        .await?;
    println!("{} one-minute bars of {name} in the last day", bars.len());
    for bar in bars.iter().rev().take(5) {
        println!(
            "{} open {} high {} low {} close {} ticks {}",
            bar.time_ms,
            to_price(bar.open),
            to_price(bar.high),
            to_price(bar.low),
            to_price(bar.close),
            bar.volume
        );
    }
    client.close().await;
    Ok(())
}
