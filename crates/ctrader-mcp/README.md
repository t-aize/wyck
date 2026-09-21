# ctrader-mcp

A Rust client for cTrader's Local and Remote MCP servers. It provides typed calls for
account, market data, and trading tools, plus conversion helpers and recovery patterns
for server behavior observed in practice. Trading calls can change a live account.

The two MCP servers have different tools and wire units. This crate does not make
them interchangeable. For ticks, live price streams, and history below M1, use
[`ctrader-openapi`](../ctrader-openapi/README.md).

## Which server to use

| | Local | Remote |
|---|---|---|
| Endpoint | cTrader Desktop MCP server, usually `http://127.0.0.1:9876/mcp/` | `https://mcp.ctrader.com/trading/mcp` or a self-hosted proxy |
| Authentication | Usually no token | Bearer token |
| Symbol | String name such as `EURUSD` | Numeric `symbolId` |
| Volume | Broker-defined units | Integer cents, 100 per unit |
| Price | Display price | Market data in integer pipettes; order prices in display units |
| Tool names | Mostly snake_case, with some camelCase names such as `listPlugins` | Snake_case |
| Mutating tools | Orders, positions, charts, watchlists, alerts, plugins | Orders and positions |
| Typed coverage | Account, trading, market data, chart, drawing, and indicator calls. Some workspace, watchlist, template, and alert methods use inferred names and return JSON. | Documented Remote tools have request and response DTOs. A few replies remain JSON values. |

Read the live symbol's lot size, volume limits, and price precision before
converting a volume or price. Local order volume uses broker-defined units;
Remote order volume uses hundredths of a unit. Remote market data prices use
integer pipettes while Remote order prices use display decimals. The `math`
module provides conversion and risk sizing helpers. Validate values against
the selected symbol before placing an order.
Lot and pip distance conversions return `Result` for invalid or overflowing
inputs; handle the error before constructing an order.

## Quick start

```rust
use ctrader_mcp::{ConnectionConfig, RemoteClient};
use ctrader_mcp::workflows::bootstrap_remote;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("CTRADER_REMOTE_TOKEN")?;
    let config = ConnectionConfig::new("https://mcp.ctrader.com/trading/mcp")
        .with_bearer_token(token);
    let client = RemoteClient::connect(&config).await?;
    let context = bootstrap_remote(&client).await?;
    println!("{} symbols", context.symbols.len());
    Ok(())
}
```

`ConnectionConfig` keeps the token in a `SecretString`, so its `Debug` output redacts
the value. Keep tokens out of command lines and repository files.

For Local, enable the MCP server in cTrader Desktop, read its port from the
application, then connect with `LocalClient::connect` and a token-free
`ConnectionConfig`. The default address is usually
`http://127.0.0.1:9876/mcp/`. Call `list_tool_names` after connecting: Local
builds may expose a different set of desktop tools.

## Calling tools safely

`LocalClient` and `RemoteClient` separate read calls from mutations. Read calls
use bounded retries for transient transport failures. Mutations are sent once.
A lost response does not prove the server rejected an order. Reconcile positions,
orders, and deals before deciding whether to submit it again. Do not retry a
mutation merely because it timed out.

`RemoteClient::has_trading_profile` checks whether the connected Remote server
advertises its mutating tools. `CreateOrderParams::validate` rejects known bad
field combinations and non-finite prices before transmission. It does not
replace a live quote, symbol limits, a margin check, or an account-level risk
policy. Use a label to help reconcile an uncertain order outcome.

Local watchlist, workspace, chart-template, and price-alert wrappers include
inferred tool names. Inspect the live `tools/list` name and input schema before
using these methods in production. If a name or field differs, use
`McpSession::call_raw` only after validating that server's schema and record the
server build with the correction.

## Recovery patterns

The implementations and their preconditions are in [`src/quirks.rs`](src/quirks.rs).

| Pattern | Purpose |
|---|---|
| P-AMEND-SAFE | Read a position's current SL and TP, send both legs on amendment, and check the response. |
| P-REMOTE-MARKET-RELATIVE | Put relative SL and TP on a Remote market order in one call. |
| P-REMOTE-MARKET-2STEP | Open a Remote market position, then amend it with absolute SL and TP. This leaves a brief unprotected interval. |
| P-REMOTE-HISTORY-CHUNK | Split Remote history requests into windows under the 720-hour limit. |
| P-LOCAL-OLDEST-FIRST | Reverse Local indicator values into newest-first order. |

`workflows` contains session bootstrap, risk sizing, trendbar backfill, pre-trade
briefing, cost comparison, and safe flatten. Entry and amendment patterns are called
from `quirks` directly.

`backfill_trendbars` returns an error if a full page makes no progress. An
error leaves the caller without a complete history; it must not be treated as
an empty range.

## Examples

| Example | Command | Environment |
|---|---|---|
| Remote probe | `cargo run -p ctrader-mcp --example probe_remote` | `CTRADER_REMOTE_TOKEN` required; `CTRADER_REMOTE_URL` optional |
| Local probe | `cargo run -p ctrader-mcp --example probe_local` | cTrader Desktop MCP server enabled; `CTRADER_LOCAL_URL` optional |
| Raw calls | `cargo run -p ctrader-mcp --example raw_call` | `CTRADER_SERVICE=remote` or `local`; matching URL and token variables as above; tool calls read from stdin |

The probes read account and tool information by default. The Remote probe's order
round trip and mutating raw calls require `CTRADER_CONFIRM_DEMO=1`; use that only
with a demo account. The raw caller prints responses, which can contain account data.

## Production verification

1. Run the tests with `cargo test -p ctrader-mcp --all-features`.
2. Run both probe examples against the exact Local and Remote builds to be used.
   Compare `tools/list`, input schemas, units, and representative read responses.
3. On a demo account only, exercise the Remote order round trip and verify its
   final position and deal state. Check Local mutation shapes separately.
4. Record the date, server build, tool schema, and outcomes in
   [`CHANGELOG.md`](CHANGELOG.md). Repeat after server or transport upgrades.

The repository's mock tests cover transport, DTOs, retries, paging, and selected
trading workflows. They do not establish live-server compatibility or prove that
a long-running process has no resource leak. The current audit and remaining
gates are in [`docs/audits/ctrader-clients.md`](../../docs/audits/ctrader-clients.md).

## Tests and verification

The behavioral notes in the source mirror the `ctrader-mcp-servers` skill,
fully audited against Remote `rest-proxy 1.0.18` and a Local build observed on
2026-05-14. Selected Remote details were checked again in 2026-09. These dates do
not establish current server behavior. Run both probe examples against the servers
you will use, compare `tools/list` and the returned data with the DTOs and quirks,
then record the build, date, and results in [`CHANGELOG.md`](CHANGELOG.md).

The [2026-07-28 MCP specification](https://modelcontextprotocol.io/specification/2026-07-28)
removes the initialization handshake and protocol-level session IDs. This client
still uses the earlier session transport. Compatibility with the current cTrader
servers needs the live probe above.
