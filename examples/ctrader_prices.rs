//! Follows the live bid and ask of one symbol on a demo account, for twenty updates.
//!
//! ```sh
//! export WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=...
//! export WYCK_OPENAPI_ACCESS_TOKEN=... WYCK_OPENAPI_ACCOUNT_ID=...
//! cargo run --example ctrader_prices -- EURUSD
//! ```

use wyck::domain::market::format_price;
use wyck::infra::ctrader::{ClientBuilder, ClientCredentials, Environment, Error, Event};

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
    let details = market.symbol_details(&[symbol.symbol_id]).await?;
    let digits = details
        .first()
        .and_then(|d| u32::try_from(d.digits).ok())
        .unwrap_or(5);

    // Subscribe after taking the receiver, so no update is missed.
    let mut events = client.events();
    market.subscribe_spots(&[symbol.symbol_id]).await?;
    let mut seen = 0;
    while let Ok(event) = events.recv().await {
        match event {
            Event::Spot(spot) if spot.symbol_id == symbol.symbol_id => {
                let show = |raw: Option<i64>| raw.map_or("-".into(), |p| format_price(p, digits));
                println!("{name} bid {} ask {}", show(spot.bid), show(spot.ask));
                seen += 1;
                if seen == 20 {
                    break;
                }
            }
            Event::Disconnected(reason) => {
                eprintln!("disconnected: {reason:?}");
                break;
            }
            _ => {}
        }
    }
    client.close().await;
    Ok(())
}
