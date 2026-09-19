//! Connects to a real cTrader account **read-only** and prints what the engine sees. It
//! never arms trading, so it cannot send an order.
//!
//! ```text
//! WYCK_SERVICE=remote WYCK_TOKEN=... cargo run -p wyck-engine --example headless
//! WYCK_SERVICE=local cargo run -p wyck-engine --example headless
//! ```
//!
//! Variables: `WYCK_SERVICE` (`remote` or `local`, default `remote`), `WYCK_ENDPOINT`
//! (optional override), `WYCK_TOKEN` (Remote only), `WYCK_WATCH` (comma separated symbols,
//! default `EURUSD`), `WYCK_SECONDS` (how long to run, default 30).

use std::time::Duration;

use secrecy::SecretString;
use wyck_engine::broker::{ConnectRequest, ServiceKind};
use wyck_engine::{Engine, EngineConfig, EventKind};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> wyck_engine::Result<()> {
    let service = match env("WYCK_SERVICE").as_deref() {
        Some("local") => ServiceKind::CtraderLocal,
        _ => ServiceKind::CtraderRemote,
    };
    let endpoint = env("WYCK_ENDPOINT").unwrap_or_else(|| service.default_endpoint().to_owned());
    let token = env("WYCK_TOKEN").map(SecretString::from);
    let seconds: u64 = env("WYCK_SECONDS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let engine = Engine::start(EngineConfig::default())?;
    let handle = engine.handle();
    let mut events = handle.subscribe();

    println!("connecting to {endpoint} ...");
    handle
        .connect(ConnectRequest::new(service, endpoint, token))
        .await?;
    handle.watch_symbols(
        env("WYCK_WATCH")
            .unwrap_or_else(|| "EURUSD".to_owned())
            .split(',')
            .map(str::to_owned),
    );

    let state = handle.state();
    println!("session: {:?}", state.session);
    if let Some(account) = &state.account {
        println!(
            "account {} ({:?}): balance {:?} {}, equity {:?}",
            account.account_id,
            account.kind,
            account.balance,
            account.currency.as_deref().unwrap_or("?"),
            account.equity
        );
    }
    println!("open positions: {}", state.positions.len());
    println!("mode: {:?} (this example never arms)", state.mode);

    let deadline = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = &mut deadline => break,
            event = events.recv() => match event {
                Ok(event) => match event.kind {
                    EventKind::AccountUpdated => {}
                    other => println!("event: {other:?}"),
                },
                Err(error) => println!("events: {error}"),
            },
        }
    }

    let state = handle.state();
    for quote in state.quotes.values() {
        println!("{}: bid {} ask {}", quote.symbol, quote.bid, quote.ask);
    }
    for warning in &state.warnings {
        println!("warning [{:?}]: {}", warning.kind, warning.message);
    }
    engine.shutdown().await;
    Ok(())
}
