//! Places the smallest market buy a symbol allows on a **demo** account, then closes it.
//!
//! ```sh
//! export WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=...
//! export WYCK_OPENAPI_ACCESS_TOKEN=... WYCK_OPENAPI_ACCOUNT_ID=...
//! cargo run --example ctrader_demo_order -- EURUSD
//! ```
//!
//! The access token needs the `trading` scope. The example refuses a live account: it connects to
//! the demo server, which does not accept live accounts, and checks the account list as well.

use wyck::domain::trading::TradeSide;
use wyck::domain::trading::contract::Contract;
use wyck::infra::ctrader::trading::NewOrderReq;
use wyck::infra::ctrader::{ClientBuilder, ClientCredentials, Environment, Error};

fn var(name: &str) -> String {
    let _ = dotenvy::dotenv();
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

#[tokio::main]
async fn main() -> wyck::infra::ctrader::Result<()> {
    let name = std::env::args().nth(1).unwrap_or_else(|| "EURUSD".into());
    let access = var("WYCK_OPENAPI_ACCESS_TOKEN");
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
    let listed = client.accounts(&access).await?;
    let is_demo = listed
        .ctid_trader_account
        .iter()
        .any(|a| a.ctid_trader_account_id == account_id && a.is_live != Some(true));
    if !is_demo {
        return Err(Error::Config(format!(
            "{account_id} is not a demo account this token covers"
        )));
    }
    let account = client.account(account_id);
    account.authorize(&access).await?;

    let market = account.market();
    let symbol = market
        .symbols()
        .await?
        .into_iter()
        .find(|s| s.symbol_name.as_deref() == Some(name.as_str()))
        .ok_or_else(|| Error::Config(format!("no symbol named {name}")))?;
    let details = market.symbol_details(&[symbol.symbol_id]).await?;
    let contract = details
        .first()
        .map_or_else(Contract::default, Contract::from_symbol);
    let volume = contract.min_volume;

    let trading = account.trading();
    let opened = trading
        .new_order(NewOrderReq::market(
            symbol.symbol_id,
            TradeSide::Buy,
            volume,
        ))
        .await?;
    let position = opened
        .position
        .ok_or_else(|| Error::Protocol("the server named no position".into()))?;
    println!(
        "opened position {} ({} lots of {name})",
        position.position_id,
        contract.lots_of_volume(volume)
    );
    let closed = trading
        .close_position(position.position_id, position.trade_data.volume)
        .await?;
    println!("closed: {:?}", closed.kind());
    client.close().await;
    Ok(())
}
