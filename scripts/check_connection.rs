//! A fast sanity check for a `.env`: connects, signs the application in, authorizes the account,
//! and reads one thing from each of the four sub-clients. Takes a couple of seconds, so run this
//! before the much slower `cargo test --test live -- --ignored --nocapture` to catch a wrong
//! client id, an expired token or a wrong account id early.
//!
//! ```sh
//! cargo run --bin check-connection
//! ```

use clap::Parser;
use wyck::openapi::ClientBuilder;
use wyck::openapi::config::{ClientCredentials, Environment};

/// Checks that the credentials in `.env` (or given as flags) actually work.
#[derive(Parser)]
#[command(
    version,
    about = "Sanity-checks a set of cTrader Open API credentials against the real server",
    long_about = None
)]
struct Args {
    /// The registered application's client id.
    #[arg(long, env = "WYCK_OPENAPI_CLIENT_ID")]
    client_id: String,

    /// The registered application's client secret.
    #[arg(long, env = "WYCK_OPENAPI_CLIENT_SECRET")]
    client_secret: String,

    /// An access token covering the account below.
    #[arg(long, env = "WYCK_OPENAPI_ACCESS_TOKEN")]
    access_token: String,

    /// Which account to authorize. Omit it to use the first demo account the token covers.
    #[arg(long, env = "WYCK_OPENAPI_ACCOUNT_ID")]
    account_id: Option<i64>,

    /// demo or live.
    #[arg(long, env = "WYCK_OPENAPI_ENVIRONMENT", default_value = "demo")]
    environment: EnvironmentArg,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum EnvironmentArg {
    Demo,
    Live,
}

impl From<EnvironmentArg> for Environment {
    fn from(value: EnvironmentArg) -> Self {
        match value {
            EnvironmentArg::Demo => Self::Demo,
            EnvironmentArg::Live => Self::Live,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();
    let environment: Environment = args.environment.into();
    let is_live = matches!(environment, Environment::Live);

    print!("connecting to {environment:?}... ");
    let client = ClientBuilder::new(environment)
        .credentials(ClientCredentials::new(args.client_id, args.client_secret))
        .connect()
        .await?;
    println!("ok (proxy version {:?})", client.version().await.ok());

    print!("listing the accounts this token covers... ");
    let accounts = client.accounts(&args.access_token).await?;
    println!("ok ({} account(s))", accounts.ctid_trader_account.len());

    let account_info = match args.account_id {
        Some(id) => accounts
            .ctid_trader_account
            .iter()
            .find(|a| a.ctid_trader_account_id == id)
            .ok_or_else(|| format!("account {id} is not covered by this token"))?,
        None => accounts
            .ctid_trader_account
            .iter()
            .find(|a| a.is_live == Some(is_live))
            .ok_or_else(|| {
                format!(
                    "the token covers no {} account; pass --account-id explicitly",
                    if is_live { "live" } else { "demo" }
                )
            })?,
    };
    let account_id = account_info.ctid_trader_account_id;
    print!(
        "authorizing account {account_id} ({:?})... ",
        account_info.broker_title_short.as_deref().unwrap_or("?")
    );
    let account = client.account(account_id);
    account.authorize(&args.access_token).await?;
    println!("ok");

    print!("reading the account's own data... ");
    let trader = account.account_data().trader().await?;
    println!(
        "ok (balance {:.2}, leverage {:?})",
        trader.balance_amount(),
        trader.leverage()
    );

    print!("reading the symbol list... ");
    let symbols = account.market().symbols().await?;
    println!("ok ({} symbols)", symbols.len());

    print!("reading the margin call thresholds... ");
    let margin_calls = account.margin().margin_calls().await?;
    println!("ok ({} threshold(s))", margin_calls.len());

    client.close().await;
    println!("\neverything answered: this .env is good to run the live tests with.");
    Ok(())
}
