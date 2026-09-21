# cTrader client audit

Date: 2026-09-21

## Verdict

The two crates have broad local test coverage and a reasonable division of work:
`ctrader-mcp` wraps the two MCP server families, while `ctrader-openapi` owns the
JSON WebSocket connection, OAuth flow, market data, account data, and trading.
Their transports and wire DTOs cannot be merged without hiding real protocol
differences. The APIs make most unit differences and uncertain order outcomes
visible in their documentation.

They are not yet verified for unattended live trading. No MCP token, approved
Open API credentials, or running Local MCP endpoint was available for this audit.
The Open API order, margin, account, and reconnecting session paths have not all
been exercised against a real server. The Local wrappers for some desktop tools
still rely on inferred names. A passing mock test cannot close these gaps.

## Findings and changes

| Priority | Finding | Evidence and action |
|---|---|---|
| High, fixed | An abandoned Open API request could leave its entry in the pending-response map until an answer or disconnection. Repeated cancelled calls to a silent server could retain memory. | `transport::connection::Client::exchange` now removes the entry on every exit, including future cancellation. A regression test cancels 20 silent calls and checks that no waiter remains. |
| High, fixed | A full outgoing queue could delay `Client::close`, and a stalled socket write could keep the connection task alive. | Shutdown now has a separate watch signal. Socket writes and close-frame delivery are bounded; a queued request has its own send timeout. Existing lifecycle tests and the new cancellation test pass. |
| High, fixed | Tick and bar history could return a partial vector after the page cap or a page that claimed more data without advancing. | Both history paths now return a protocol error when they cannot finish paging. Mock tests cover empty pages with `hasMore: true`. Remote MCP backfill also errors on a full page with no new timestamp. |
| High, fixed | Stopping a session could cancel token refresh after the server rotated the refresh token but before the new pair was saved. | Refresh and persistence now finish before stop takes effect. The new pair remains available through `Session::tokens` if the store fails. Persistence has a 30-second bound. A test stops during a delayed save and checks the stored pair. A process crash between server rotation and local persistence remains unavoidable. |
| High, fixed | NaN, infinity, and overflowing risk calculations could pass comparisons and saturate on conversion to integer volume. Some order prices could serialize as null. | MCP risk sizing rejects non-finite inputs and unrepresentable results. Lot and pip distance conversions now return `Result` on invalid or overflowing inputs. MCP Remote and Open API new-order requests reject non-finite prices and invalid positive fields before send. |
| Medium, fixed | An Open API `max_retry_wait` below 250 ms could panic in `Duration::clamp`; zero timeouts were also accepted. | `ConnectionConfig::validate` rejects these settings before connecting. Rates above the documented defaults remain configurable for local stress tests and custom proxies. |
| High, open | Local workspace, watchlist, chart-template, and price-alert wrappers use inferred tool names and JSON replies. | Compare each name and input schema with a live Local `tools/list`. Correct any mismatch and add a contract test before calling these wrappers production-ready. |
| High, open | The MCP client still uses an initialization handshake and session transport. | The [2026-07-28 MCP specification](https://modelcontextprotocol.io/specification/2026-07-28) removes those protocol steps. Probe current cTrader servers and plan a transport upgrade if they move to the new revision. |
| High, open | Real Open API execution, account, margin, OAuth callback, and session reconnect behavior is incompletely checked. | Run the gated demo tests and a connection-loss exercise with an approved application. Record server version, account environment, request, response, and outcome. Never infer order failure solely from a timeout. |

## API review

- MCP reads use retry policies; trading mutations use one attempt. `RemoteClient`
  checks its advertised trading profile. `CreateOrderParams::validate` catches
  common schema errors, and the caller still owns account limits and risk rules.
- Open API `Client` is one connection; `Session` supervises reconnection and
  subscriptions. Events are bounded by broadcast capacity, and outgoing messages
  use a bounded queue. `TradingClient` returns the first execution event; the
  caller must follow later events or reconcile account state. An accepted order
  is not necessarily filled.
- The exposed `NewOrderReq` and MCP DTO fields remain mutable. Their local
  validation covers known dangerous values but cannot verify live market price,
  margin, account permissions, or broker symbol rules. Lower-level display
  money and price helpers still require finite, in-range caller inputs.
- Both crates contain price and volume helpers because their encodings differ.
  The separate rate limiters reflect different server limits. No duplicated
  implementation was removed merely for sharing a name.

## Verification record

- `cargo test --workspace --all-features`, `cargo fmt --all -- --check`, and
  `cargo clippy -p ctrader-mcp -p ctrader-openapi --all-targets --all-features
  -- -D warnings` passed locally after the corrections.
- The Open API test suite uses an in-process WebSocket server and property tests.
  The MCP suite uses an in-process MCP server. Neither proves that a live server
  still accepts the current DTOs or that process memory stays flat over days.
- Current cTrader Open API documentation states [50 standard and 5 historical
  requests per second](https://help.ctrader.com/open-api/), a heartbeat at least
  every [10 seconds](https://help.ctrader.com/open-api/faq/), and a [one-week tick
  request limit with `hasMore` paging](https://help.ctrader.com/open-api/symbol-data/).

## Release gates

1. Run `probe_local` and `probe_remote` against the intended server builds, and
   save the `tools/list` names and schemas. Test Local inferred wrappers only
   after their names have been confirmed.
2. With an approved Open API application and a demo account, run the read-only
   live test while the market is open. Check account, margin, tick and bar
   pagination, OAuth callback state, and token renewal.
3. On a demo account, run the separately gated order test and reconcile the
   resulting order, position, and deal. Cut the network during a `Session` run,
   then verify reconnection and restored subscriptions.
4. Run repeated connection, cancellation, and reconnect cycles under a memory
   profiler. Confirm that pending requests and background tasks return to zero
   and that retained memory plateaus. This is evidence of bounded behavior, not
  a proof that no memory leak can exist.

## API migration

`ctrader-mcp::math::units::{lots_to_units, lots_to_cents, units_to_cents}` and
`ctrader-mcp::math::pip::{price_to_pips, sl_tp_to_pip_distances, pips_to_points}`
now return `Result` so callers must handle invalid input and overflow.
`ctrader-mcp::quirks::market_with_relative_sl_tp` now returns
`Result<CreateOrderParams, CTraderError>` for the same reason. Open API
`NewOrderReq::validate` runs inside `TradingClient::new_order` and may return a
configuration error before network I/O for an invalid request.
