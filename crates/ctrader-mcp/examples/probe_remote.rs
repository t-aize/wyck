//! Live smoke test for a Remote connection: walks the same path `wyck` does on connect
//! (`RemoteClient::connect` + `bootstrap_remote`), printing every step to stdout as it
//! happens: useful for telling apart a bad token/endpoint from a real code bug.
//!
//! Reads connection details from the environment rather than hardcoding them, so this
//! file is safe to commit and reuse:
//!
//! ```bash
//! # macOS/Linux (bash/zsh)
//! CTRADER_REMOTE_TOKEN=<your token> cargo run -p ctrader-mcp --example probe_remote
//! # or, against a self-hosted rest-proxy:
//! CTRADER_REMOTE_URL=https://my-proxy.example/mcp CTRADER_REMOTE_TOKEN=<token> \
//!     cargo run -p ctrader-mcp --example probe_remote
//! ```
//!
//! ```powershell
//! # Windows (PowerShell)
//! $env:CTRADER_REMOTE_TOKEN = "<your token>"
//! cargo run -p ctrader-mcp --example probe_remote
//! # or, against a self-hosted rest-proxy:
//! $env:CTRADER_REMOTE_URL = "https://my-proxy.example/mcp"
//! $env:CTRADER_REMOTE_TOKEN = "<your token>"
//! cargo run -p ctrader-mcp --example probe_remote
//! ```
//!
//! Set `CTRADER_CONFIRM_DEMO=1` as well, with a **demo** token only, to also run a real order
//! round trip (open a tiny position, move its stop, close it) and print every raw answer.
//!
//! ```bat
//! :: Windows (cmd.exe)
//! set CTRADER_REMOTE_TOKEN=<your token>
//! cargo run -p ctrader-mcp --example probe_remote
//! ```

use ctrader_mcp::common::TradeSide;
use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::quirks;
use ctrader_mcp::remote::RemoteClient;
use ctrader_mcp::remote::dto::{ClosePositionParams, CreateOrderParams};
use ctrader_mcp::workflows::{RemoteSessionContext, bootstrap_remote};

const DEFAULT_URL: &str = "https://mcp.ctrader.com/trading/mcp";

#[tokio::main]
async fn main() {
    let url = std::env::var("CTRADER_REMOTE_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned());
    let Ok(token) = std::env::var("CTRADER_REMOTE_TOKEN") else {
        eprintln!(
            "set CTRADER_REMOTE_TOKEN to a Remote MCP bearer token first \
             (optionally CTRADER_REMOTE_URL to point at a self-hosted rest-proxy; \
             defaults to {DEFAULT_URL})"
        );
        std::process::exit(1);
    };

    println!("connecting to {url} ...");
    let connection = ConnectionConfig::new(&url).with_bearer_token(token);

    let client = match RemoteClient::connect(&connection).await {
        Ok(client) => {
            println!("connected (initialize handshake OK)");
            client
        }
        Err(source) => {
            println!("FAILED to connect: {source}");
            println!("debug: {source:?}");
            return;
        }
    };

    match client.session().list_tool_names().await {
        Ok(tools) => println!("advertised tools ({}): {tools:?}", tools.len()),
        Err(source) => println!("tools/list FAILED: {source}"),
    }

    match bootstrap_remote(&client).await {
        Ok(session) => {
            println!("bootstrap OK");
            println!("  trader_id: {:?}", session.trader_id);
            println!("  account_currency: {:?}", session.account_currency);
            println!("  balance: {:?}", session.balance_display);
            println!("  equity: {:?}", session.equity_display);
            println!("  has_trading_profile: {}", session.has_trading_profile);
            println!("  symbols cached: {}", session.symbols.len());
            println!("  assets cached: {}", session.assets.len());
            order_round_trip(&client, &session).await;
        }
        Err(source) => {
            println!("bootstrap FAILED: {source}");
            println!("debug: {source:?}");
        }
    }
}

/// Opens, amends and closes a tiny position, printing every raw answer. Only runs with
/// `CTRADER_CONFIRM_DEMO=1`, which is the caller's statement that the token is a **demo**
/// one (this crate cannot tell), and only on an account with no open position.
///
/// `CTRADER_PROBE_SYMBOL` (default `BTCUSD`, the symbol that trades on a weekend),
/// `CTRADER_PROBE_VOLUME_CENTS` (default `1`, that is 0.01 of a coin),
/// `CTRADER_PROBE_STOP_POINTS` (default `50000000`, that is 500.00 in units of 1e-5).
async fn order_round_trip(client: &RemoteClient, session: &RemoteSessionContext) {
    if std::env::var("CTRADER_CONFIRM_DEMO").as_deref() != Ok("1") {
        println!(
            "\norder round trip skipped: set CTRADER_CONFIRM_DEMO=1 (demo token only) to run it"
        );
        return;
    }
    if !session.has_trading_profile {
        println!("\norder round trip skipped: the connection is bound to the read-only profile");
        return;
    }
    let env_i64 = |name: &str, default: i64| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let symbol = std::env::var("CTRADER_PROBE_SYMBOL").unwrap_or_else(|_| "BTCUSD".to_owned());
    let Some(found) = session.find_symbol(&symbol) else {
        println!("\norder round trip skipped: unknown symbol {symbol}");
        return;
    };
    let volume = env_i64("CTRADER_PROBE_VOLUME_CENTS", 1);
    let stop = env_i64("CTRADER_PROBE_STOP_POINTS", 50_000_000);

    match client.get_positions().await {
        Ok(open) if open.positions.is_empty() && open.orders.is_empty() => {}
        Ok(_) => {
            println!("\norder round trip skipped: the account already has positions or orders");
            return;
        }
        Err(source) => {
            println!("\norder round trip skipped: get_positions FAILED: {source}");
            return;
        }
    }

    println!("\n== order round trip: {symbol} volume {volume} cents, stop {stop} points");
    let params = CreateOrderParams::market_with_relative_sl_tp(
        found.symbol_id,
        TradeSide::Buy,
        volume,
        stop,
        stop * 2,
    )
    .with_label(format!("{}-probe", session.idempotency_prefix));
    let created = match client.create_order(params).await {
        Ok(created) => created,
        Err(source) => {
            println!("create_order FAILED: {source}\ndebug: {source:?}");
            return;
        }
    };
    println!("create_order OK: {created:?}");
    let Some(position) = created
        .position
        .as_ref()
        .and_then(|p| p.position_id.map(|id| (id, p)))
    else {
        println!("no position id in the answer: check the account by hand");
        return;
    };
    let (position_id, position) = position;
    println!(
        "position {position_id}: entry {:?} sl {:?} tp {:?}",
        position.entry_price, position.stop_loss, position.take_profit
    );

    if let Some(entry) = position.entry_price {
        // Move the stop only: the take profit must survive (Q-R10).
        let new_stop = ((entry - stop as f64 * 1e-5 * 0.8) * 100.0).round() / 100.0;
        match quirks::amend_position_preserving_legs(client, position_id, Some(new_stop), None)
            .await
        {
            Ok(amended) => println!("amend OK: {amended:?}"),
            Err(source) => println!("amend FAILED: {source}\ndebug: {source:?}"),
        }
    }
    match client
        .close_position(ClosePositionParams {
            position_id,
            volume: position.volume.unwrap_or(volume),
        })
        .await
    {
        Ok(closed) => println!("close OK: {closed:?}"),
        Err(source) => println!("close FAILED (close it by hand!): {source}\ndebug: {source:?}"),
    }
    match client.get_positions().await {
        Ok(after) => println!("positions after: {}", after.positions.len()),
        Err(source) => println!("get_positions FAILED: {source}"),
    }
}
