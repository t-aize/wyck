# Changelog

All notable changes to `ctrader-openapi`. The crate is not published, and versions follow the
workspace, so entries are grouped by what was added rather than by release.

## Unreleased

### Changed

- **Architecture**: the crate is reorganized by domain instead of by file size.
  `transport/` holds the connection itself (`Client`, the envelope, the rate limiter, the
  connection messages); `market/`, `account/`, `trading/` and `margin/` each hold their domain's
  types and a named sub-client (`MarketClient`, `AccountDataClient`, `TradingClient`,
  `MarginClient`); `session/` and `auth/` are split into their own directories along the same
  lines. `AccountClient` used to expose about twenty methods flat (`symbols`, `subscribe_spots`,
  `new_order`, `expected_margin`, ...); it now routes to the four sub-clients above through
  `.market()`, `.account_data()`, `.trading()` and `.margin()`, each a small `Clone` value that
  still carries the connection and the account id. A new `ClientBuilder` connects and identifies
  the application in one call, and a new `prelude` module group-imports the pieces most programs
  need. The old `client.rs`, `wire.rs`, `rate_limit.rs`, `model.rs`, `account.rs`,
  `account_api.rs`, `market.rs`, `types.rs`, `history.rs`, `trading.rs`, `margin.rs`, `auth.rs`,
  `callback.rs` and `session.rs` are gone; their content moved into the directories above.
  The trading request builders (`NewOrderReq::market`, `::limit`, `::stop`, `::stop_limit`,
  `AmendOrderReq::new`, `AmendPositionSlTpReq::new`) drop their `account_id` parameter, which the
  owning sub-client always overwrote before sending anyway.

  **Nothing about the protocol changed**: the wire format (the envelope shape, camelCase field
  names, every `payloadType` number), the `OpenApiError`/`ErrorKind` variants and their meaning,
  the `Event`/`DisconnectReason` variants and when each fires, the `RateLimiter`'s behavior, and
  the OAuth flow (`authorization_url`, `TokenSet`, the refresh order, the `Auth`/`Transport`
  error split) are exactly as before. This is a reorganization of where things live and how the
  public surface is grouped, not a behavior change; no test needed to change its assertions, only
  its imports and, where a call moved to a sub-client, how it is reached.

### Added

- **Trading**: `trading` module. `NewOrderReq` (market, limit, stop, stop limit, with optional stop
  loss and take profit, built through typed constructors), `AmendOrderReq`, `CancelOrderReq`,
  `ClosePositionReq` (full or partial), `AmendPositionSLTPReq`, and decoding of `ExecutionEvent`
  (accepted, filled, partially filled, replaced, cancelled, expired, rejected, a cancel itself
  rejected, a swap, a deposit or withdrawal, a bonus deposit or withdrawal), `OrderErrorEvent` and
  `TrailingSLChangedEvent`. Documented as not idempotent: a caller that hits a timeout should check
  the account's state before retrying, not resend the order. Needs a token of the `trading`
  `auth::Scope`, which already existed.
- **Margin**: `margin` module. Expected margin for a volume before sending an order, the account's
  three margin call thresholds (read, and update one), dynamic leverage schedules, and the margin
  changed, margin call triggered and margin call updated events.
- **Account gaps**: cash flow (deposit and withdrawal) history, deals and orders by position, order
  details (an order with the deals that filled it), deal offsets, and position unrealized PnL,
  added to `account` and `account_api` next to the existing account calls.
- **Market gap**: symbols for conversion (the chain of symbols to convert one asset into another
  when no symbol quotes them directly), and the symbol changed event.
- **`Event`**: `Execution`, `OrderError`, `TrailingSlChanged`, `MarginChanged`,
  `MarginCallTriggered`, `MarginCallUpdated` and `SymbolChanged` variants, all decoded in
  `event_from`.
- **`AccountClient`**: mirrors of every new trading and margin call, and of the account gaps.
- **Wire**: payload type numbers for every message above, added to `wire::payload` under an
  Auth/Trading/Market/Account/Margin grouping, each pinned by a test against the official
  `ProtoOAPayloadType` enum. Every value of that enum (2100 to 2188) is now covered; see the
  README's "Protocol coverage" section for the full table.
- **Connection**: `Client` over the JSON WebSocket (`demo` and `live`, port 5036), requests matched by
  `clientMsgId`, a heartbeat, broadcast events, a state watch, a request timeout, two rate limiters
  (general and history) with margins under the documented limits, and automatic retries of requests
  refused for their rate.
- **Sign in**: OAuth 2 helpers (`authorization_url`, `OAuthClient::exchange_code` and `refresh`,
  `TokenSet`), a loopback `CallbackListener`, and the `sign_in` example.
- **Session**: `Session` reconnects with a jittered growing delay, signs in again, restores
  subscriptions, renews tokens before they expire (saving the new pair first through a
  `TokenStore`), and stops promptly.
- **Market data**: symbols, prices, live bars, the order book, tick history and bar history,
  `history::fetch_ticks` and `fetch_bars` paging whole ranges, and the helpers in `market`
  (`SymbolTable`, `SpotTracker`, `DepthBook`, `format_price`).
- **Account data** (read only): account details, open positions and working orders, deals and orders in
  a range, assets, asset classes, symbol categories, the cTrader ID profile, account log out, and the
  account update event.
- **`AccountClient`**: a client bound to one account.
- **Errors**: one `OpenApiError` sorted into `ErrorKind`s, with `is_retryable` and `retry_after`.
  `kind()` now names every trading `ProtoOAErrorCode` explicitly next to the `Rejected` fallback
  they already sorted into, pinned by a test, so a typo cannot silently change one's meaning.
- **Tests**: unit tests in every module, integration tests against a scripted WebSocket server and a
  stand-in token endpoint (including `tests/trading.rs` and `tests/margin.rs`), property tests
  (proptest), robustness tests, and two ignored live tests for a demo account: one read only that
  also cross checks bars against ticks, and one, gated by `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1` on top
  of `#[ignore]`, that places and closes a minimal market order.

### Removed

- **The `examples/` directory** (`sign_in`, `account_info`, `list_symbols`, `stream_prices`,
  `download_ticks`, `download_bars`, `resilient_stream`, and their shared `examples/common`). Seven
  small programs were more upkeep than value to keep in sync with every new call; the README and
  the crate's doc comments cover the same ground with runnable doctests and inline snippets, and
  `tests/live.rs` is the live validation path now.

### Fixed (found by the live runs and the property tests)

- Tick prices are differences from the previous tick, like the times: `decode_ticks` keeps a running
  sum of both.
- Ticks that share a millisecond keep the order they happened in, so the last price is the last one.
- `retryAfter` and `maintenanceEndTimestamp` are seconds, not milliseconds.
- `BLOCKED_PAYLOAD_TYPE` is a rate limit: the defaults stay under the documented limits and the client
  resends the request after the wait the server asks for.
- The order book adds entries at the same price into one level, so the best level is unambiguous.
- A session stopped in the middle of a connection attempt returns at once instead of waiting for the
  attempt's timeout.
- A failure to reach the token endpoint is a transport error (retried), not a refusal (fatal).
- `ALREADY_SUBSCRIBED` counts as success, so a reconnect racing a subscription does not drop it.

### Known limits

- Not yet run against a live server: trading and margin (the code paths exist in `tests/live.rs`,
  gated behind `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`, but have not been exercised against a real demo
  account), the account calls, the `Session`, whether the consent page echoes `state`, the range
  limit of bar requests per period, and how long a broker keeps ticks.
