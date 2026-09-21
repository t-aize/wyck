# ctrader-mcp

A Rust client for cTrader's Local and Remote MCP servers. It provides typed calls for
account, market data, and trading tools, plus conversion helpers and recovery patterns
for server behavior observed in practice. Trading calls can change a live account.

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

Read the live symbol's lot size and precision before converting a volume or price.
The two servers do not use interchangeable encodings. The `math` module provides
the conversions.

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

## Examples

| Example | Command | Environment |
|---|---|---|
| Remote probe | `cargo run -p ctrader-mcp --example probe_remote` | `CTRADER_REMOTE_TOKEN` required; `CTRADER_REMOTE_URL` optional |
| Local probe | `cargo run -p ctrader-mcp --example probe_local` | cTrader Desktop MCP server enabled; `CTRADER_LOCAL_URL` optional |
| Raw calls | `cargo run -p ctrader-mcp --example raw_call` | `CTRADER_SERVICE=remote` or `local`; matching URL and token variables as above; tool calls read from stdin |

The probes read account and tool information by default. The Remote probe's order
round trip and mutating raw calls require `CTRADER_CONFIRM_DEMO=1`; use that only
with a demo account. The raw caller prints responses, which can contain account data.

## Tests and verification

Run `cargo test -p ctrader-mcp --features test-support` for the in-process mock MCP
tests. The behavioral notes in the source mirror the `ctrader-mcp-servers` skill,
fully audited against Remote `rest-proxy 1.0.18` and a Local build observed on
2026-05-14. Selected Remote details were checked again in 2026-09. These dates do
not establish current server behavior. Run both probe examples against the servers
you will use, compare `tools/list` and the returned data with the DTOs and quirks,
then record the build, date, and results in [`CHANGELOG.md`](CHANGELOG.md).

The [2026-07-28 MCP specification](https://modelcontextprotocol.io/specification/2026-07-28)
removes the initialization handshake and protocol-level session IDs. This client
still uses the earlier session transport. Compatibility with the current cTrader
servers needs the live probe above.
