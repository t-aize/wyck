//! Live smoke test for a Remote connection: walks the same path `wyck` does on connect
//! (`RemoteClient::connect` + `bootstrap_remote`), printing every step to stdout as it
//! happens — useful for telling apart a bad token/endpoint from a real code bug.
//!
//! Reads connection details from the environment rather than hardcoding them, so this
//! file is safe to commit and reuse:
//!
//! ```bash
//! CTRADER_REMOTE_TOKEN=<your token> cargo run -p ctrader-mcp --example probe_remote
//! # or, against a self-hosted rest-proxy:
//! CTRADER_REMOTE_URL=https://my-proxy.example/mcp CTRADER_REMOTE_TOKEN=<token> \
//!     cargo run -p ctrader-mcp --example probe_remote
//! ```

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::remote::RemoteClient;
use ctrader_mcp::workflows::bootstrap_remote;

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
        }
        Err(source) => {
            println!("bootstrap FAILED: {source}");
            println!("debug: {source:?}");
        }
    }
}
