# Changelog

All notable changes to `ctrader-openapi`. The crate is not published, and versions follow the
workspace, so entries are grouped by what was added rather than by release.

## Unreleased

### Added

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
- **Examples**: `sign_in`, `account_info`, `list_symbols`, `stream_prices`, `download_ticks`,
  `download_bars`, `resilient_stream`.
- **Tests**: unit tests in every module, integration tests against a scripted WebSocket server and a
  stand-in token endpoint, property tests (proptest), robustness tests, and an ignored live test for a
  demo account that also cross checks bars against ticks.

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

- Read only: no order is sent. Trading would be a separate, deliberate addition.
- Not yet run against a live server: the account calls, the `Session`, whether the consent page echoes
  `state`, the range limit of bar requests per period, and how long a broker keeps ticks.
