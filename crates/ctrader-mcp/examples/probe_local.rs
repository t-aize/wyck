//! Live smoke test for a Local connection (cTrader Desktop's built-in MCP server).
//! Connects, lists the tools the live server actually advertises, and cross-checks that
//! list against every tool name [`ctrader_mcp::local::LocalClient`] calls — this crate
//! documents several Local tool names (chart templates, workspaces, watchlists, price
//! alerts) as "best-effort inferred, not confirmed against a live server"; this is the
//! tool that confirms or disproves them.
//!
//! Enable the server first: cTrader Desktop -> Advanced -> MCP Server -> Enable MCP
//! Server.
//!
//! ```bash
//! # macOS/Linux (bash/zsh)
//! cargo run -p ctrader-mcp --example probe_local
//! # or, if you changed the port on that settings page:
//! CTRADER_LOCAL_URL=http://127.0.0.1:9999/mcp/ cargo run -p ctrader-mcp --example probe_local
//! ```
//!
//! ```powershell
//! # Windows (PowerShell)
//! cargo run -p ctrader-mcp --example probe_local
//! # or, if you changed the port on that settings page:
//! $env:CTRADER_LOCAL_URL = "http://127.0.0.1:9999/mcp/"
//! cargo run -p ctrader-mcp --example probe_local
//! ```
//!
//! ```bat
//! :: Windows (cmd.exe)
//! set CTRADER_LOCAL_URL=http://127.0.0.1:9999/mcp/
//! cargo run -p ctrader-mcp --example probe_local
//! ```

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::local::LocalClient;

const DEFAULT_URL: &str = "http://127.0.0.1:9876/mcp/";

/// Every tool name `LocalClient` calls, in the same order as `src/local/client.rs`.
/// Kept as a flat list (rather than trying to mirror the file's section comments) since
/// the point of this tool is exactly to catch this list drifting from the live server.
const EXPECTED_TOOLS: &[&str] = &[
    "ping",
    "get_server_time",
    "get_accounts_list",
    "get_balance",
    "get_account_statistics",
    "get_symbols",
    "get_symbol_details",
    "get_symbol_sessions",
    "get_spot_prices",
    "get_trendbars",
    "get_positions",
    "place_market_order",
    "amend_position",
    "close_position",
    "close_position_partial",
    "close_all_positions",
    "get_pending_orders",
    "place_limit_order",
    "place_stop_order",
    "place_stop_limit_order",
    "amend_order",
    "cancel_order",
    "cancel_all_pending_orders",
    "get_order_history",
    "get_deals",
    "list_charts",
    "focus_chart",
    "get_active_chart",
    "change_chart_symbol",
    "change_chart_timeframe",
    "get_chart_viewport",
    "scroll_chart",
    "zoom_chart",
    "add_chart_object",
    "get_chart_objects",
    "update_chart_object",
    "delete_chart_object",
    "clear_chart_objects",
    "listChartIndicators",
    "addChartIndicator",
    "removeChartIndicator",
    "update_indicator_parameters",
    "getIndicatorValues",
    // Best-effort inferred — never confirmed against a live server before now.
    "save_chart_template",
    "list_chart_templates",
    "apply_chart_template",
    "delete_chart_template",
    "save_workspace",
    "load_workspace",
    "list_workspaces",
    "delete_workspace",
    "show_notification",
    "get_watchlists",
    "create_watchlist",
    "rename_watchlist",
    "delete_watchlist",
    "add_symbol_to_watchlist",
    "remove_symbol_from_watchlist",
    "get_price_alerts",
    "create_price_alert",
    "delete_price_alert",
    "listPlugins",
    "startPlugin",
    "stopPlugin",
];

#[tokio::main]
async fn main() {
    let url = std::env::var("CTRADER_LOCAL_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned());

    println!("connecting to {url} ...");
    let connection = ConnectionConfig::new(&url);

    let client = match LocalClient::connect(&connection).await {
        Ok(client) => {
            println!("connected (initialize handshake OK, no token needed)");
            client
        }
        Err(source) => {
            println!("FAILED to connect: {source}");
            println!("debug: {source:?}");
            println!(
                "is cTrader Desktop running with Advanced -> MCP Server -> Enable MCP Server \
                 checked?"
            );
            return;
        }
    };

    let live_tools = match client.session().list_tool_names().await {
        Ok(tools) => tools,
        Err(source) => {
            println!("tools/list FAILED: {source}");
            return;
        }
    };
    println!("live server advertises {} tools", live_tools.len());

    let confirmed: Vec<&str> = EXPECTED_TOOLS
        .iter()
        .copied()
        .filter(|name| live_tools.iter().any(|live| live == name))
        .collect();
    let missing: Vec<&str> = EXPECTED_TOOLS
        .iter()
        .copied()
        .filter(|name| !live_tools.iter().any(|live| live == name))
        .collect();
    let unrecognized: Vec<&str> = live_tools
        .iter()
        .map(String::as_str)
        .filter(|live| !EXPECTED_TOOLS.contains(live))
        .collect();

    println!(
        "\nconfirmed ({}/{}): every tool this crate calls that the live server also advertises",
        confirmed.len(),
        EXPECTED_TOOLS.len()
    );
    for name in &confirmed {
        println!("  OK   {name}");
    }

    if !missing.is_empty() {
        println!(
            "\nMISSING ({}): this crate calls these, but the live server does not advertise them \
             — likely a wrong inferred name in `local/client.rs`, or a build that doesn't \
             expose them",
            missing.len()
        );
        for name in &missing {
            println!("  ??   {name}");
        }
    }

    if !unrecognized.is_empty() {
        println!(
            "\nUNRECOGNIZED ({}): the live server advertises these, but this crate never calls \
             them — candidates for new `LocalClient` capabilities",
            unrecognized.len()
        );
        for name in &unrecognized {
            println!("  NEW  {name}");
        }
    }
}
