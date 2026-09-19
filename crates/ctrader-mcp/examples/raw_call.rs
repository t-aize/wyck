//! Calls MCP tools by name and prints the raw JSON answers. Meant for looking at what a
//! server really returns, and for capturing fixtures.
//!
//! Reads one call per line on stdin, `tool_name {optional json arguments}`, and prints one
//! block per call. Tools that can change state (orders, positions, alerts, charts, ...) are
//! refused unless `CTRADER_CONFIRM_DEMO=1` is set, so only do that on a demo account.
//!
//! ```bash
//! # Remote (token from the environment, never from a file in the repository)
//! CTRADER_SERVICE=remote CTRADER_REMOTE_TOKEN=<token> cargo run -p ctrader-mcp --example raw_call <<'EOF'
//! get_server_time
//! get_symbols
//! EOF
//!
//! # Local (cTrader Desktop with the MCP server enabled)
//! CTRADER_SERVICE=local cargo run -p ctrader-mcp --example raw_call <<'EOF'
//! get_balance
//! get_symbol_details {"symbolName":"EURUSD"}
//! EOF
//! ```
//!
//! Variables: `CTRADER_SERVICE` (`remote` or `local`, default `remote`), `CTRADER_REMOTE_URL`,
//! `CTRADER_LOCAL_URL`, `CTRADER_REMOTE_TOKEN`, `CTRADER_CONFIRM_DEMO`.

use std::io::BufRead;

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::local::LocalClient;
use ctrader_mcp::remote::RemoteClient;
use ctrader_mcp::transport::McpSession;

/// Read-only tool names. Anything else needs the demo confirmation.
const READ_ONLY_PREFIXES: [&str; 5] = ["get_", "list_", "listAvailable", "calculate_", "ping"];

fn is_read_only(tool: &str) -> bool {
    READ_ONLY_PREFIXES.iter().any(|p| tool.starts_with(p))
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

async fn run(session: &McpSession, confirmed: bool) {
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (tool, args) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        println!("=== {line}");
        if tool == "tools" {
            match args.trim() {
                "" => println!("{:?}", session.list_tool_names().await),
                name => match session.peer().list_all_tools().await {
                    Ok(all) => {
                        for t in all.iter().filter(|t| t.name == name || name == "all") {
                            println!("{}", serde_json::to_string(&t).unwrap_or_default());
                        }
                    }
                    Err(e) => println!("ERROR: {e}"),
                },
            }
            continue;
        }
        if !is_read_only(tool) && !confirmed {
            println!("refused: `{tool}` may change state, set CTRADER_CONFIRM_DEMO=1 (demo only)");
            continue;
        }
        let arguments = match args.trim() {
            "" => Some(serde_json::Map::new()),
            text => match serde_json::from_str::<serde_json::Value>(text) {
                Ok(serde_json::Value::Object(map)) => Some(map),
                Ok(_) | Err(_) => {
                    println!("refused: arguments must be a JSON object");
                    continue;
                }
            },
        };
        match session.call_raw(leak(tool), arguments).await {
            Ok(value) => println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            ),
            Err(error) => println!("ERROR: {error}\n{error:?}"),
        }
    }
}

#[tokio::main]
async fn main() {
    let confirmed = std::env::var("CTRADER_CONFIRM_DEMO").is_ok_and(|v| v == "1");
    let local = std::env::var("CTRADER_SERVICE").is_ok_and(|v| v.eq_ignore_ascii_case("local"));
    if local {
        let url = std::env::var("CTRADER_LOCAL_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:9876/mcp/".to_owned());
        let client = LocalClient::connect(&ConnectionConfig::new(&url))
            .await
            .expect("connect to Local");
        run(client.session(), confirmed).await;
    } else {
        let url = std::env::var("CTRADER_REMOTE_URL")
            .unwrap_or_else(|_| "https://mcp.ctrader.com/trading/mcp".to_owned());
        let token = std::env::var("CTRADER_REMOTE_TOKEN").expect("set CTRADER_REMOTE_TOKEN");
        let client = RemoteClient::connect(&ConnectionConfig::new(&url).with_bearer_token(token))
            .await
            .expect("connect to Remote");
        run(client.session(), confirmed).await;
    }
}
