//! Connect, sign the application and an account in, and print a symbol's prices as they arrive.
//!
//! Shows the shape the new sub-client split reads as in practice: `account.market()` for the
//! symbol lookup and the subscription, `client.events()` for the stream.
//!
//! ```sh
//! WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=... WYCK_OPENAPI_ACCESS_TOKEN=... \
//!     cargo run -p ctrader-openapi --example stream_prices
//! ```
//!
//! `WYCK_OPENAPI_SYMBOL` picks the symbol (default `EURUSD`). Runs against `demo` and reads only:
//! it places no order. Stop it with Ctrl-C.

use ctrader_openapi::{ClientBuilder, ClientCredentials, Environment, Event};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this example"))
}

#[tokio::main]
async fn main() -> ctrader_openapi::Result<()> {
    let credentials = ClientCredentials::new(
        env("WYCK_OPENAPI_CLIENT_ID"),
        env("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let access_token = env("WYCK_OPENAPI_ACCESS_TOKEN");
    let wanted = std::env::var("WYCK_OPENAPI_SYMBOL").unwrap_or_else(|_| "EURUSD".into());

    let client = ClientBuilder::new(Environment::Demo)
        .credentials(credentials)
        .connect()
        .await?;

    let accounts = client.accounts(&access_token).await?;
    let account_id = accounts
        .ctid_trader_account
        .iter()
        .find(|a| a.is_live == Some(false))
        .expect("the token covers no demo account")
        .ctid_trader_account_id;
    let account = client.account(account_id);
    account.authorize(&access_token).await?;
    println!("signed in on demo account {account_id}");

    let market = account.market();
    let symbols = market.symbols().await?;
    let symbol = symbols
        .iter()
        .find(|s| s.symbol_name.as_deref() == Some(wanted.as_str()))
        .unwrap_or_else(|| panic!("{wanted} not offered by this account"));

    let mut events = client.events();
    market.subscribe_spots(&[symbol.symbol_id]).await?;
    println!("following {wanted}, press Ctrl-C to stop");

    while let Ok(event) = events.recv().await {
        match event {
            Event::Spot(spot) => println!(
                "{wanted}: bid {:?} ask {:?}",
                spot.bid.map(ctrader_openapi::market::to_price),
                spot.ask.map(ctrader_openapi::market::to_price),
            ),
            Event::Disconnected(reason) => {
                println!("connection ended: {reason:?}");
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
