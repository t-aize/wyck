# wyck-ctrader

A typed client of the cTrader Open API over its JSON WebSocket: the connection, the OAuth sign-in,
a session that reconnects and renews its tokens, and the calls for market data, accounts and
trading. No screen, no file, no keyring: tokens come in as arguments and go out as values.

Depends on: `wyck-core`, `wyck-market`, `wyck-trading`, `tokio`, `tokio-tungstenite`, `reqwest`,
`futures-util`, `serde`, `serde_json`, `secrecy`, `uuid`, `async-trait`, `thiserror`, `tracing`
(`dotenvy` in the examples).

## Module map

`Client -> AccountClient -> { MarketClient, AccountDataClient, TradingClient, MarginClient }`, and
`session::Session` built on the same `Client` for a program that stays up.

| Module | What it holds |
|---|---|
| (root) | `Client`, `ClientBuilder` (connect and identify the application), `AccountClient` (a client bound to one account), `ConnectionState`, and the re-exports of `config`, `error` and `event`. |
| `market` | `MarketClient`: symbols, live prices and bars, the order book, tick and bar history (`fetch_bars`, `fetch_ticks`). Requests in `requests.rs`. |
| `account` | `AccountDataClient`: balance, positions, orders, deals. |
| `trading` | `TradingClient`: place, amend and cancel orders, amend a position's stop loss and take profit, close positions. |
| `margin` | `MarginClient`: the margin a volume would need (`expected_margin`). |
| `session` | `Session`: reconnects with a `Backoff`, renews tokens, restores subscriptions; `TokenStore` and `MemoryTokenStore` say where tokens are kept. |
| `auth` | OAuth 2: the consent URL, the exchange of the code, the refresh, the local redirect listener (`CallbackListener`). |
| `event`, `config`, `error` | What the server sends unasked (`Event`); demo or live, rates, timeouts (`ConnectionConfig`); `Error` and `ErrorKind`. |
| `transport` (private) | The wire format (`wire`, `messages`), the connection task and the rate limiter. |

## Invariants and traps

- **A trading call moves money**: simulated on a demo account, real on a live one. A call is not
  idempotent: after a timeout, look at the account before sending the order again (the module docs
  of `trading` say how).
- **Demo and live are separate**: `demo.ctraderapi.com` and `live.ctraderapi.com` on port 5036, one
  connection each; the accounts of one cannot be used on the other.
- **Rate limits**: the server allows 50 requests per second and 5 for history, per connection. The
  client stays under (`ConnectionConfig` defaults: 40 and 4) and sends a request again, after the
  wait the server asks for (`retryAfter` is in seconds), when it is refused for its rate.
- **Heartbeat**: the connection sends one every 5 seconds by default, and `ConnectionConfig` refuses
  10 seconds or more, because the server drops a connection silent for that long.
- **Prices are integers** scaled by `wyck_market::PRICE_SCALE`. Ticks come newest first, with their
  times and prices as differences from the tick before (`wyck_market::decode_ticks`), at most a
  week per request, bid and ask requested separately. There is no volume per tick.
- **A secret never appears in an error or a log**: tokens are `secrecy` values, and an HTTP error
  is stripped of its URL (`without_url`) before it is written down.
- The client never reconnects on its own: `Event::Disconnected` says it ended, `Session` decides.

## Global state

None. The runtime that tokio-based code needs is `wyck-platform`'s; this crate assumes a live
tokio runtime under it (`reqwest` and `tokio-tungstenite` panic without one).

## What the crate offers

Only what the app calls. The rest of the Open API (margin calls, dynamic leverage, order details,
cash flow, deals by position, depth subscriptions, the version and profile requests, the
logout...) was deleted with its tests; `docs/PLAN.md` (phase 4d) has the list. The server offers it
still: add a call back when the app needs it, following "Where to add X".

## Tests

Unit tests next to the code and a local mock WebSocket server for everything above the wire. The
examples (`cargo run -p wyck-ctrader --example ctrader_prices -- EURUSD`, also `ctrader_accounts`,
`ctrader_history`, `ctrader_demo_order`) run against a real demo account and need the variables of
`.env.example` (copy it to `.env` at the root of the repository; Git ignores `.env`). Nothing
automated does. Run: `cargo test -p wyck-ctrader`.

## Where to add X

- A call the server offers: the request and response types in the `requests.rs` of the module, the
  payload number in `transport/wire.rs`, a method on the client of the module, a test on the mock
  server.
- An event: `event.rs` (`Event` and `event_from`).
- A new example: `examples/`, with its variables in `.env.example`.

## Do not

- Do not use `gpui`, `keyring`, `directories` or any screen or file crate: `cargo xtask deps` fails
  on them. Storing tokens is `wyck-storage`'s job, the app wires the two.
- Do not depend on `wyck-chart`, `wyck-storage` or anything above `wyck-trading`.
- Do not send an order from a test or an example against a live account.
