//! Lists the trading accounts an access token covers.
//!
//! ```sh
//! export WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=... WYCK_OPENAPI_ACCESS_TOKEN=...
//! cargo run -p wyck-openapi --example accounts
//! ```
//!
//! The variables can also sit in a `.env` file in the working directory.

use wyck_openapi::{ClientBuilder, ClientCredentials, Environment};

fn var(name: &str) -> String {
    let _ = dotenvy::dotenv();
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

#[tokio::main]
async fn main() -> wyck_openapi::Result<()> {
    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID"),
        var("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    // Demo and live are separate servers; the account list is the same on both.
    let client = ClientBuilder::new(Environment::Demo)
        .credentials(credentials)
        .connect()
        .await?;
    let accounts = client.accounts(&var("WYCK_OPENAPI_ACCESS_TOKEN")).await?;
    for account in &accounts.ctid_trader_account {
        let kind = if account.is_live == Some(true) {
            "live"
        } else {
            "demo"
        };
        println!(
            "{} ({kind}), login {:?}, broker {:?}",
            account.ctid_trader_account_id, account.trader_login, account.broker_title_short
        );
    }
    client.close().await;
    Ok(())
}
