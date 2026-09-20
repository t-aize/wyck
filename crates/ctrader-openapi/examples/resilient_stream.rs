//! Streams live prices through a [`Session`], which reconnects, signs in again, and restores the
//! subscriptions by itself. Leave it running and cut the network: it comes back.
//!
//! ```text
//! cargo run -p ctrader-openapi --example resilient_stream -- EURUSD GBPUSD
//! ```
//!
//! It prints each price, and a line for every reconnection, token refresh or failure. Stop it with
//! Ctrl+C.
//!
//! Needs `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` and `WYCK_OPENAPI_ACCESS_TOKEN`.
//! With `WYCK_OPENAPI_REFRESH_TOKEN` set as well, the session also renews the access token before
//! it expires; the new pair is kept in memory only here (a real program stores it, see
//! `TokenStore`), so this example is not for runs longer than the token lives.

#[path = "common/mod.rs"]
mod common;

use std::sync::Arc;
use std::time::SystemTime;

use ctrader_openapi::Event;
use ctrader_openapi::auth::TokenSet;
use ctrader_openapi::config::ConnectionConfig;
use ctrader_openapi::market::{SpotTracker, SymbolTable, format_price};
use ctrader_openapi::session::{MemoryTokenStore, Session, SessionConfig, SessionEvent};
use secrecy::SecretString;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names = common::positional(&args);
    if names.is_empty() {
        eprintln!("Name at least one symbol, for example: resilient_stream -- EURUSD");
        std::process::exit(2);
    }

    // The first connection is made by hand only to learn the account and the symbol ids; the
    // session does everything from then on.
    let setup = common::setup().await;
    let account_id = setup.account.id();
    let table = SymbolTable::new(
        setup
            .account
            .symbols()
            .await
            .expect("cannot list the symbols"),
    );
    setup.client.close().await;
    let ids: Vec<i64> = names.iter().filter_map(|n| table.id_of(n)).collect();
    if ids.is_empty() {
        eprintln!("None of those symbols exist on this account.");
        std::process::exit(2);
    }

    let tokens = TokenSet {
        access_token: SecretString::from(common::need("WYCK_OPENAPI_ACCESS_TOKEN")),
        refresh_token: SecretString::from(
            common::maybe("WYCK_OPENAPI_REFRESH_TOKEN").unwrap_or_default(),
        ),
        token_type: None,
        // Unknown: without a refresh token the session never tries to refresh.
        expires_in: None,
        obtained_at: SystemTime::now(),
    };
    let config = SessionConfig::new(
        ConnectionConfig::new(common::environment()),
        common::credentials(),
        account_id,
    );
    let session = Session::start(config, tokens, Arc::new(MemoryTokenStore::default()))
        .expect("the session settings are unusable");
    let mut events = session.events();
    session
        .subscribe_spots(&ids)
        .await
        .expect("cannot follow the prices");

    let mut tracker = SpotTracker::new();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            received = events.recv() => match received {
                Ok(SessionEvent::Ready) => println!("-- ready"),
                Ok(SessionEvent::Data(Event::Spot(spot))) => {
                    let quote = tracker.apply(&spot);
                    let show = |p: Option<i64>| p.map_or("-".to_owned(), |p| format_price(p, 5));
                    println!(
                        "{:<10} {:>10} {:>10}",
                        table.name_of(quote.symbol_id).unwrap_or("?"),
                        show(quote.bid),
                        show(quote.ask)
                    );
                }
                Ok(SessionEvent::Reconnecting { attempt, retry_in, reason }) => {
                    // After a reconnect the first price of each symbol arrives again: start clean.
                    tracker.clear();
                    println!("-- down ({reason}); attempt {attempt}, trying again in {retry_in:?}");
                }
                Ok(SessionEvent::TokensRefreshed) => println!("-- tokens refreshed"),
                Ok(SessionEvent::SubscriptionFailed { what, error }) => println!("-- could not follow {what}: {error}"),
                Ok(SessionEvent::Failed(error)) => {
                    println!("-- the session ended and needs you: {error}");
                    break;
                }
                Ok(SessionEvent::Stopped) => break,
                Ok(_) => {}
                Err(error) => println!("-- (the reader fell behind: {error})"),
            }
        }
    }
    session.stop().await;
}
