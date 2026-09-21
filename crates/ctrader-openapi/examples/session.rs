//! A program that stays up: `Session` owns the connection, reconnects and renews tokens by
//! itself, and this program only reads its events and says what to follow.
//!
//! ```sh
//! WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=... WYCK_OPENAPI_ACCESS_TOKEN=... \
//! WYCK_OPENAPI_REFRESH_TOKEN=... WYCK_OPENAPI_ACCOUNT_ID=... \
//!     cargo run -p ctrader-openapi --example session
//! ```
//!
//! `WYCK_OPENAPI_SYMBOL` picks the symbol (default `EURUSD`). This example uses
//! `session::MemoryTokenStore`, which forgets the tokens when the process ends: a real program
//! implements `session::TokenStore` on top of the OS keyring or an encrypted file instead. Stop it
//! with Ctrl-C.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ctrader_openapi::auth::TokenSet;
use ctrader_openapi::session::{MemoryTokenStore, Session, SessionConfig, SessionEvent};
use ctrader_openapi::{ClientCredentials, ConnectionConfig, Environment, Event};
use secrecy::SecretString;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this example"))
}

#[tokio::main]
async fn main() -> ctrader_openapi::Result<()> {
    let credentials = ClientCredentials::new(
        env("WYCK_OPENAPI_CLIENT_ID"),
        env("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let account_id: i64 = env("WYCK_OPENAPI_ACCOUNT_ID")
        .parse()
        .expect("WYCK_OPENAPI_ACCOUNT_ID must be a number");
    let wanted = std::env::var("WYCK_OPENAPI_SYMBOL").unwrap_or_else(|_| "EURUSD".into());

    // In a real program these come from a sign in (see `examples/sign_in.rs`) followed by
    // whatever secret store keeps them between runs; here they are read straight from the
    // environment so this example needs no state of its own.
    let tokens = TokenSet {
        access_token: SecretString::from(env("WYCK_OPENAPI_ACCESS_TOKEN")),
        refresh_token: SecretString::from(env("WYCK_OPENAPI_REFRESH_TOKEN")),
        token_type: None,
        expires_in: None,
        obtained_at: SystemTime::now(),
    };

    let config = SessionConfig::new(
        ConnectionConfig::new(Environment::Demo),
        credentials,
        account_id,
    );
    let session = Session::start(config, tokens, Arc::new(MemoryTokenStore::default()))?;
    let mut events = session.events();

    println!("waiting for the session to come up...");
    let client = session.wait_ready(Duration::from_secs(30)).await?;
    let market = client.account(account_id).market();
    let symbols = market.symbols().await?;
    let symbol = symbols
        .iter()
        .find(|s| s.symbol_name.as_deref() == Some(wanted.as_str()))
        .unwrap_or_else(|| panic!("{wanted} not offered by this account"));
    session.subscribe_spots(&[symbol.symbol_id]).await?;
    println!("following {wanted}, press Ctrl-C to stop");

    while let Ok(event) = events.recv().await {
        match event {
            SessionEvent::Data(Event::Spot(spot)) => println!(
                "{wanted}: bid {:?} ask {:?}",
                spot.bid.map(ctrader_openapi::market::to_price),
                spot.ask.map(ctrader_openapi::market::to_price),
            ),
            SessionEvent::Reconnecting {
                attempt,
                retry_in,
                reason,
            } => {
                println!("reconnecting (attempt {attempt} in {retry_in:?}): {reason}");
            }
            SessionEvent::Ready => println!("ready again"),
            SessionEvent::Failed(error) => {
                eprintln!("the session ended and needs attention: {error}");
                break;
            }
            SessionEvent::Stopped => break,
            _ => {}
        }
    }
    Ok(())
}
