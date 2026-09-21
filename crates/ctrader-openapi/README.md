# ctrader-openapi

A Rust client for the **cTrader Open API**, over its JSON WebSocket.

The two MCP servers cTrader offers (see [`ctrader-mcp`](../ctrader-mcp)) stop at one minute: no
tick history, no tick stream, no bars below M1. The Open API has all of that. This crate is a
typed, tested client for it, with what it takes to run for days unattended: request matching,
heartbeats, the documented rate limits and their retries, the OAuth 2 sign in, and a session that
reconnects and renews its tokens by itself.

**Reads and trades.** Symbols, prices, history, account data and margin are read only calls; the
`trading` module places, amends and cancels orders, and closes positions. A trading call can move
money, simulated on a demo account but real on a live one. See [Trading safety](#trading-safety)
before using it from anything you are not watching.

Contents: [What it does](#what-it-does) | [Quick start](#quick-start) | [Guides](#guides) |
[Configuration](#configuration) | [Errors](#errors) | [Things to know](#things-to-know) |
[Trading safety](#trading-safety) | [Troubleshooting](#troubleshooting) | [Tests](#tests) |
[What is verified](#what-is-verified) | [Protocol coverage](#protocol-coverage) |
[Design](#design) | [Sources](#sources)

## What it does

| | |
|---|---|
| Connection | `wss://demo.ctraderapi.com:5036` or `live`, JSON envelope, one connection per environment |
| Requests | Matched to answers by `clientMsgId`, many in flight at once, a timeout each |
| Limits | 50 requests per second, 5 for history (the client keeps a little under: 40 and 4), enforced by spacing; a request refused for its rate is sent again after the wait the server asks for (in seconds) |
| Keep alive | A heartbeat every few seconds (the server drops a connection silent for 10) |
| Sign in | The consent URL, a loopback web server for the redirect, code exchange, token refresh |
| Session | Reconnects with a growing delay, signs in again, restores subscriptions, renews tokens before they expire, stops promptly |
| Live data | Prices, live bars, the order book, account and token notices, as events |
| History | Ticks (bid and ask, in windows under a week) and bars (14 periods), paged into whole ranges |
| Account | Balance, positions, working orders, deals (also by position, with offsets), order details, assets, symbol categories, cash flow history, unrealized PnL, the cTrader ID profile |
| Trading | New orders (market, limit, stop, stop limit), amend, cancel, close a position (in full or in part), amend a position's stop loss and take profit |
| Margin | Expected margin for a volume, the three margin call thresholds (read and update one), dynamic leverage schedules |
| Helpers | Symbol lookup by name, latest quote per symbol, an order book, price formatting, an account bound client |
| Errors | One typed error, sorted into a few kinds, with `is_retryable` and the server's retry advice |

## Quick start

You need an application registered on the Open API portal (`https://openapi.ctrader.com/`), which
gives a client id and a client secret, and lets you register redirect URIs. Approval is by
Spotware and can take time. For a desktop program register `http://localhost:8765` (or another
port) as a redirect URI. Keep the secret out of files and logs: it belongs in the OS keyring or an
encrypted store (see `wyck-config`).

1. **Sign in once** and get a token. The crate has no `sign_in` binary of its own; wire
   `auth::authorization_url`, `callback::CallbackListener` and `auth::OAuthClient` together, as the
   complete example in the crate docs does (`cargo doc -p ctrader-openapi --open`, or the
   ["A complete run"](#a-complete-run) example below). Ask for `Scope::Trading` at this step if the
   program will place orders; `Scope::Accounts` cannot.

2. **Use the token**:

   ```rust
   let client = Client::connect(&ConnectionConfig::new(Environment::Demo)).await?;
   client.authenticate_application(&credentials).await?;
   let account = client.account(account_id);
   account.authorize(&access_token).await?;

   let mut events = client.events();
   account.subscribe_spots(&[symbol_id]).await?;             // live prices
   let ticks = account.ticks(symbol_id, QuoteType::Bid, from_ms, to_ms).await?;
   let bars = account.bars(symbol_id, Period::M1, from_ms, to_ms).await?;
   ```

A complete, compiling version of steps 1 and 2 together is in the crate's top level docs.

### A complete run

```rust
use ctrader_openapi::auth::{authorization_url, new_state, OAuthClient, Scope};
use ctrader_openapi::callback::CallbackListener;
use ctrader_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use ctrader_openapi::{Client, Event};
use secrecy::ExposeSecret;
use std::time::Duration;

// 1. Sign the user in (once; keep the tokens in a secret store afterwards).
let credentials = ClientCredentials::new("my-client-id", "my-client-secret");
let listener = CallbackListener::bind(8765).await?;
let redirect = listener.redirect_uri();
let state = new_state();
println!("open: {}", authorization_url(&credentials.client_id, &redirect, Scope::Accounts, &state));
let code = listener.wait(&state, Duration::from_secs(300)).await?;
let oauth = OAuthClient::new(credentials.clone())?;
let tokens = oauth.exchange_code(code.code(), &redirect).await?;

// 2. Connect, identify the application, authorize an account.
let client = Client::connect(&ConnectionConfig::new(Environment::Demo)).await?;
client.authenticate_application(&credentials).await?;
let access = tokens.access_token.expose_secret();
let accounts = client.accounts(access).await?;
let account = client.account(accounts.ctid_trader_account[0].ctid_trader_account_id);
account.authorize(access).await?;

// 3. Follow a symbol's prices.
let symbols = account.symbols().await?;
let eurusd = symbols.iter().find(|s| s.symbol_name.as_deref() == Some("EURUSD")).unwrap();
let mut events = client.events();
account.subscribe_spots(&[eurusd.symbol_id]).await?;
while let Ok(event) = events.recv().await {
    match event {
        Event::Spot(spot) => println!("{:?} {:?}", spot.bid, spot.ask),
        Event::Disconnected(reason) => { println!("gone: {reason:?}"); break; }
        _ => {}
    }
}
```

This same example, in a form that compiles and runs as a doctest, is in `src/lib.rs`.

## Guides

### Which layer to use

| You are writing | Use | Why |
|---|---|---|
| A script or a tool that runs for a minute | `Client` | Connect, sign in, ask, close. Simple. |
| A program that stays up (a recorder, a chart feed) | `Session` | It owns the connection; you read events and say what to follow. |
| Your own supervision | `Client` and `Event::Disconnected` | The client never reconnects by itself. |

### Signing in

The Open API takes no user name and password. The user grants an **application** access to their
cTrader ID in the browser, and the application then holds tokens.

```text
authorization_url(...)  ->  user opens it, picks accounts and a scope
CallbackListener        ->  catches the redirect on localhost, with a code (valid one minute)
OAuthClient::exchange_code  ->  access token (about 30 days) + refresh token
OAuthClient::refresh        ->  a new pair; the OLD refresh token stops working
Client::accounts / authorize_account  ->  the token opens an account on a connection
```

Scopes: `Scope::Accounts` is view only (enough for everything in this crate), `Scope::Trading`
is full access. Store the tokens with a `TokenStore` on top of the OS keyring; the session saves
every new pair **before** using it.

### Streaming prices

```rust
let mut events = client.events();
account.subscribe_spots(&ids).await?;
let mut tracker = SpotTracker::new();
while let Ok(event) = events.recv().await {
    if let Some(quote) = tracker.apply_event(&event) {   // both sides, the other carried forward
        println!("{:?} {:?}", quote.bid, quote.ask);
    }
}
```

An event carries only the side that changed, and the first one after a subscription carries the
last price even when the market is closed. `format_price(raw, digits)` writes a price the way the
symbol quotes it. `subscribe_live_bars` adds the bar in progress to the events (`SpotEvent::live_bars`),
and `subscribe_depth` feeds a `DepthBook`.

### History

`account.ticks(...)` and `account.bars(...)` return whole ranges. Underneath, `history::fetch_ticks`
cuts the range into windows under the server's one week limit, follows `hasMore` backwards (the
server returns the **newest** ticks of a window first), and puts the pages together. Every request
goes through the 5 per second historical limit, so a long range takes a while.

```rust
let bids = account.ticks(id, QuoteType::Bid, from, to).await?;
let asks = account.ticks(id, QuoteType::Ask, from, to).await?;
let quotes = merge_sides(&bids, &asks);        // one quote per change, both sides side by side
```

### Staying up

```rust
let session = Session::start(SessionConfig::new(conn, credentials, account_id), tokens, store)?;
let mut events = session.events();
session.subscribe_spots(&ids).await?;          // kept, and restored after every reconnect
while let Ok(event) = events.recv().await {
    match event {
        SessionEvent::Data(Event::Spot(spot)) => { /* ... */ }
        SessionEvent::Reconnecting { attempt, retry_in, .. } => { /* the link dropped */ }
        SessionEvent::Failed(error) => { /* only the user can fix this: sign in again */ break; }
        _ => {}
    }
}
session.stop().await;
```

What it does on your behalf: connects and signs the application and the account in; restores
subscriptions after every reconnect; waits a doubling, jittered delay between attempts; refreshes
the access token when it expires within a day (and saves the new pair first); reconnects at once
after a "tokens invalidated" notice; stops promptly even in the middle of an attempt. Failures that
will pass (a drop, a timeout, maintenance, a rate limit) are retried; a refused refresh token, an
unauthorized account or a bad setting end the session with `SessionEvent::Failed`. It does not
replay requests that were in flight when the link dropped: they fail with `Closed` and may be
repeated.

### Account data

```rust
let trader = account.trader().await?;                          // balance, leverage, rights
let (positions, orders) = account.open_positions_and_orders(false).await?;
let (deals, more) = account.deals(from, to, Some(100)).await?;
let pnl = account.client().position_unrealized_pnl(account.id()).await?;
let (deals_of_position, more) = account.client().deals_by_position(account.id(), position_id, None, None).await?;
```

Money amounts are integers scaled by `10^moneyDigits`; volumes are in hundredths of a unit; prices
of positions and orders are ordinary decimals. `account::money`, `volume_units` and the `Trader`
helpers convert. Enumerations are numbers with a `from_number` that gives `None` for a value this
crate does not know, so a server that adds one does not crash a program.

### Trading

Needs a token of the `trading` scope (`Scope::Trading` at sign in); a `trading` call on an
`accounts` token is refused. See [Trading safety](#trading-safety) first.

```rust
use ctrader_openapi::account::TradeSide;
use ctrader_openapi::trading::NewOrderReq;

// A market buy, with a stop loss, recognizable later by its label.
let request = NewOrderReq::market(account.id(), symbol_id, TradeSide::Buy, volume_in_centilots)
    .with_protection(Some(stop_loss_price), None)
    .with_label("my-strategy-1");
let execution = account.new_order(request).await?;
println!("{:?}", execution.kind());               // ExecutionType::OrderFilled, usually, for a market order

// A limit order, amended, then cancelled.
let request = NewOrderReq::limit(account.id(), symbol_id, TradeSide::Sell, volume_in_centilots, limit_price);
let execution = account.new_order(request).await?;
let order_id = execution.order.as_ref().unwrap().order_id;
let mut amend = ctrader_openapi::trading::AmendOrderReq::new(account.id(), order_id);
amend.limit_price = Some(new_limit_price);
account.amend_order(amend).await?;
account.cancel_order(order_id).await?;

// Closing a position, in full or in part.
account.close_position(position_id, volume_to_close).await?;
```

`NewOrderReq` is **not idempotent**: a call that times out may still have reached the server. See
the `trading` module docs, and [Trading safety](#trading-safety), before retrying one.

### Margin

```rust
let margins = account.expected_margin(symbol_id, &[volume_in_centilots]).await?;   // before sending an order
let thresholds = account.margin_calls().await?;                                    // the three margin call levels
let leverage = account.dynamic_leverage(leverage_id).await?;                       // Symbol::leverage_id
```

## Configuration

There is no built in configuration file or environment variable reading in the crate itself
(`ConnectionConfig`, `ClientCredentials` and `SessionConfig` are plain structs your program fills
in, from `wyck-config` or however else it keeps settings); the one place this crate reads the
environment is its own live test.

| Variable | Meaning |
|---|---|
| `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` | The registered application, for `tests/live.rs` |
| `WYCK_OPENAPI_ACCESS_TOKEN` | An access token covering a demo account, for `tests/live.rs` |
| `WYCK_OPENAPI_SYMBOL` | The symbol the live test reads and, when trading is allowed, trades (default `EURUSD`) |
| `WYCK_OPENAPI_ALLOW_LIVE_TRADING` | Must be exactly `1` for the live trading test to do anything at all; see [Trading safety](#trading-safety) |

In code, `ConnectionConfig` holds the timeouts, the heartbeat interval, the request rates
(`standard_rate`, `historical_rate`), the retry count for rate limit refusals (`rate_limit_retries`)
and the event buffer size; `SessionConfig` adds the token refresh margin, the reconnect backoff
and an optional attempt limit.

## Errors

Everything that can go wrong is an `OpenApiError`. Callers rarely need the variants: `kind()` sorts
them, `is_retryable()` says whether trying again later can work, and `retry_after()` gives the
server's own advice.

| `ErrorKind` | Meaning | What to do |
|---|---|---|
| `RateLimited` | The rate was exceeded (`REQUEST_FREQUENCY_EXCEEDED`, `BLOCKED_PAYLOAD_TYPE`) | Wait; the client already retries a few times |
| `Maintenance` | The server is under maintenance | Wait for `maintenance_end` |
| `TokenInvalid` | The access token is expired or invalidated | Refresh it, or sign in again |
| `NotAuthorized` | The application or account is not authorized on this connection | Authorize it; a demo account cannot be used on `live` |
| `Rejected` | The server refused the request (an unknown symbol, a bad range) | Fix the request |
| `Transport` | The connection could not be made or dropped | Retry with a delay |
| `Timeout` | No answer in time | Retry |
| `Protocol` | An unreadable or unexpected message | Report it; it is a bug or a change on the server |
| `Closed` | The connection had ended | Make a new client, or use a `Session` |
| `Config` | Unusable settings | Fix them |

## Things to know

- **Prices are integers** scaled by 100 000 (`1.08501` is `108501`). `types::to_price` converts;
  `market::format_price` writes them with the symbol's decimals.
- **Ticks** arrive newest first with their times **and prices** as differences from the tick before
  (confirmed on a live account). `decode_ticks` turns that into absolute, ascending times and
  prices, and keeps ticks that share a millisecond in the order they happened.
- **There is no volume per tick.** A bar's `volume` counts ticks, of both the bid and the ask. Only
  the order book has sizes.
- **Bars and ticks differ a little.** About 96 percent of minutes match the bid ticks exactly; the
  rest are wider by 1 to 3 units, because the bars come from a richer feed than the tick history.
- **Demo and live never mix.** An app that needs both opens two connections.
- **The market being closed** is not an error: history for the last hours is empty, and only one
  price per symbol arrives.
- **The client never reconnects by itself**; a `Session` does. After a disconnect, waiting requests
  fail with `Closed` and a last `Event::Disconnected` is sent.
- **Tokens are secrets.** `Debug` never shows them, errors never repeat them, and the HTTP layer's
  errors are stripped of their URL, which holds the client secret and the code.
- **`retryAfter` is in seconds**, and with `BLOCKED_PAYLOAD_TYPE` it is the time until that type of
  request is unblocked.
- **A trading call needs the `trading` scope.** A token signed in with `Scope::Accounts` is
  refused; there is no way to widen a token's scope after the fact, only to sign in again.
- **Every trading call answers with an `ExecutionEvent`,** whether it succeeded (`OrderAccepted`,
  `OrderFilled`, `OrderPartialFill`, ...) or the server rejected it as a typed error. A rejection
  raises `OpenApiError::Server`, like any other refused request; it is not silently folded into a
  "success" execution event.

## Trading safety

- **A trading call can move money.** Simulated on a demo account, real on a live one; a demo
  account's balance is not real money, but it is not reset between runs either, so a script that
  loops on `new_order` can still wreck a demo account's balance and confuse whatever else uses it.
- **`NewOrderReq` (and the rest of `trading`) is not guaranteed idempotent.** A timeout
  (`OpenApiError::Timeout`) or a dropped connection (`OpenApiError::Closed`) while a trading request
  is in flight means the *answer* was lost, not necessarily the request: the order may already be
  on the account. Do not retry a trading call on a bare timeout. Check
  `Client::open_positions_and_orders` (`ProtoOAReconcileReq`) or `Client::deals` first, using the
  `label` or `client_order_id` the original request carried to recognize it, then decide.
- **Demo and live use separate application credentials and endpoints.** `demo.ctraderapi.com` and
  `live.ctraderapi.com` are different connections with different accounts; an application approved
  for one is not automatically approved for the other, and a demo account cannot be authorized on
  the live connection (or the reverse).
- **This crate adds no trading safety net of its own.** It is a thin typed client, the same shape as
  Spotware's own `OpenApiPy` and `OpenAPI.Net`: no position sizing, no confirmation prompts, no
  "are you sure this is a demo account" check. Whatever guardrails a program needs (position limits,
  a dry run mode, a human in the loop) belong in the caller.
- **The live trading test is gated twice.** `tests/live.rs`'s
  `place_and_close_a_minimal_market_order_on_a_demo_account` needs `#[ignore]` overridden *and*
  `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`, checked before the test does anything else, so it cannot fire
  from a bare `cargo test` or from CI.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `BLOCKED_PAYLOAD_TYPE`, "You are being rate limited" | Too many requests of one kind. The client retries after the wait; if it persists, lower `historical_rate` |
| History is empty | The market was closed for that range. Try a range that ends at the last price (Friday) |
| `ACCOUNT_NOT_AUTHORIZED` | The account was not authorized on this connection, or it is a demo account on `live` (or the reverse) |
| `CH_ACCESS_TOKEN_INVALID` / `OA_AUTH_TOKEN_EXPIRED` | Refresh the token (`OAuthClient::refresh`) or sign in again; a `Session` does this |
| The consent page shows an error about the redirect | The redirect URI must be registered for the application exactly as used, port included |
| `cannot listen on port ... is it in use?` | Another program holds the redirect port; free it or register another |
| Prices look wrong by a factor | Divide raw prices by 100 000, then show them with the symbol's `digits` |
| The session keeps saying `Reconnecting` | See the `reason` in the event; a wrong system clock or a blocked port are common |
| A trading call fails with `ACCOUNT_NOT_AUTHORIZED` or a similar refusal even though reads work | The token's scope is `accounts`, not `trading`; sign in again with `Scope::Trading` |
| `TRADING_BAD_VOLUME` | The volume is not a multiple of the symbol's `step_volume`, or is outside `min_volume`/`max_volume` |
| `NOT_ENOUGH_MONEY` / `MAX_EXPOSURE_REACHED` | Check `Client::expected_margin` before sending the order, and the account's free margin |

## Tests

```sh
cargo test -p ctrader-openapi                       # about 250 tests, no network beyond localhost
cargo test -p ctrader-openapi --test live -- --ignored --nocapture   # a real demo account, read only
```

| Suite | What it covers |
|---|---|
| unit tests (`src/`) | Every module: decoding, limits, errors, the callback listener, the session helpers |
| `tests/client.rs` | The client against a scripted WebSocket server: requests, errors, events, end of connection, history, the symbol conversion chain |
| `tests/account.rs`, `tests/handle.rs` | The account calls (including cash flow, deals and orders by position, order details, deal offsets, unrealized PnL) and the account bound client |
| `tests/trading.rs` | Placing, amending and cancelling orders, closing a position, and their error paths |
| `tests/margin.rs` | Expected margin, margin call thresholds, dynamic leverage |
| `tests/session.rs` | Reconnection, restoring subscriptions, token refresh, failures, stopping |
| `tests/oauth.rs` | The token exchange against a stand-in endpoint, and that no error repeats a secret |
| `tests/properties.rs` | Properties on random inputs (proptest): tick round trips, windows, merging, the book, backoff, every known event type decoded from arbitrary JSON |
| `tests/robustness.rs` | Nonsense frames (including malformed trading and margin events), event bursts, 300 concurrent requests, races with a close, a vanishing peer |
| `tests/live.rs` | Read only checks against a real demo account (ignored by default), plus a doubly gated trading check |

The **read only** live test needs an approved application and an access token in environment
variables (`WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET`, `WYCK_OPENAPI_ACCESS_TOKEN`,
optionally `WYCK_OPENAPI_SYMBOL`). It refuses a live account and never writes anything. It also
cross checks bars against ticks and the page seams of the tick history.

The **trading** live test (`place_and_close_a_minimal_market_order_on_a_demo_account`) needs the
same variables, an access token with the `trading` scope, and
`WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`; without that last variable it does nothing at all, checked
before it even connects. See [Trading safety](#trading-safety).

## What is verified

Everything above the wire runs in tests against a local server. A live demo account confirmed the
connection, both sign in steps, the symbol list and details, the price subscription, tick and bar
history with paging (no tick lost at page seams), the tick encoding, bars against ticks, and the
rate limit behavior. **Not yet run live**: trading (`new_order` and the rest of the `trading`
module; the code path exists in `tests/live.rs` but has not been exercised against a real demo
account), margin, the account calls (`trader`, positions, deals, catalogs), the `Session` against
the real server, whether the consent page echoes `state` (the callback accepts a redirect without
one and reports it, see `AuthorizationCode::state_echoed`), the range limit of bar requests per
period, and how far back a broker keeps ticks. `TODO.md` section 2A.7 tracks them.

## Protocol coverage

Every value of the official `ProtoOAPayloadType` enum (`OpenApiModelMessages.proto`,
`github.com/spotware/openapi-proto-messages`, checked 2026-09-21), and what implements it here. The
two envelope messages below the `2100` block are the proxy's own, not `ProtoOA*`.

| Number | `ProtoOAPayloadType` | Implemented by |
|---|---|---|
| 50 | `ProtoErrorRes` (the proxy's own) | `model::ErrorRes`, `event::error_of` |
| 51 | `ProtoHeartbeatEvent` | `wire::Envelope::heartbeat`, `client` (sent and received) |

### Auth (`2100`-`2105`)

| Number | `ProtoOAPayloadType` | Implemented by |
|---|---|---|
| 2100/2101 | `..._APPLICATION_AUTH_REQ`/`RES` | `Client::authenticate_application` |
| 2102/2103 | `..._ACCOUNT_AUTH_REQ`/`RES` | `Client::authorize_account` |
| 2104/2105 | `..._VERSION_REQ`/`RES` | `Client::version` |

### Trading (`2106`-`2111`, `2126`, `2132`)

| Number | `ProtoOAPayloadType` | Implemented by |
|---|---|---|
| 2106 | `..._NEW_ORDER_REQ` | `Client::new_order`, `trading::NewOrderReq` |
| 2107 | `..._TRAILING_SL_CHANGED_EVENT` | `Event::TrailingSlChanged`, `trading::TrailingSlChangedEvent` |
| 2108 | `..._CANCEL_ORDER_REQ` | `Client::cancel_order` |
| 2109 | `..._AMEND_ORDER_REQ` | `Client::amend_order`, `trading::AmendOrderReq` |
| 2110 | `..._AMEND_POSITION_SLTP_REQ` | `Client::amend_position_sl_tp` |
| 2111 | `..._CLOSE_POSITION_REQ` | `Client::close_position` |
| 2126 | `..._EXECUTION_EVENT` | The answer to every call above, and `Event::Execution` when unsolicited |
| 2132 | `..._ORDER_ERROR_EVENT` | `Event::OrderError`, `trading::OrderErrorEvent` |

### Market (`2112`-`2120`, `2127`-`2131`, `2135`-`2138`, `2145`-`2146`, `2155`-`2161`)

| Number | `ProtoOAPayloadType` | Implemented by |
|---|---|---|
| 2112/2113 | `..._ASSET_LIST_REQ`/`RES` | `Client::assets` |
| 2114/2115 | `..._SYMBOLS_LIST_REQ`/`RES` | `Client::symbols` |
| 2116/2117 | `..._SYMBOL_BY_ID_REQ`/`RES` | `Client::symbol_details` |
| 2118/2119 | `..._SYMBOLS_FOR_CONVERSION_REQ`/`RES` | `Client::symbols_for_conversion` |
| 2120 | `..._SYMBOL_CHANGED_EVENT` | `Event::SymbolChanged` |
| 2127/2128 | `..._SUBSCRIBE_SPOTS_REQ`/`RES` | `Client::subscribe_spots` |
| 2129/2130 | `..._UNSUBSCRIBE_SPOTS_REQ`/`RES` | `Client::unsubscribe_spots` |
| 2131 | `..._SPOT_EVENT` | `Event::Spot`, `market::SpotTracker` |
| 2135/2165 | `..._SUBSCRIBE_LIVE_TRENDBAR_REQ`/`RES` | `Client::subscribe_live_bars` |
| 2136/2166 | `..._UNSUBSCRIBE_LIVE_TRENDBAR_REQ`/`RES` | `Client::unsubscribe_live_bars` |
| 2137/2138 | `..._GET_TRENDBARS_REQ`/`RES` | `Client::bars_page`, `history::fetch_bars` |
| 2145/2146 | `..._GET_TICKDATA_REQ`/`RES` | `Client::tick_page`, `history::fetch_ticks` |
| 2153/2154 | `..._ASSET_CLASS_LIST_REQ`/`RES` | `Client::asset_classes` |
| 2155 | `..._DEPTH_EVENT` | `Event::Depth`, `market::DepthBook` |
| 2156/2157 | `..._SUBSCRIBE_DEPTH_QUOTES_REQ`/`RES` | `Client::subscribe_depth` |
| 2158/2159 | `..._UNSUBSCRIBE_DEPTH_QUOTES_REQ`/`RES` | `Client::unsubscribe_depth` |
| 2160/2161 | `..._SYMBOL_CATEGORY_REQ`/`RES` | `Client::symbol_categories` |

### Account (`2121`-`2125`, `2133`-`2134`, `2142`-`2152`, `2162`-`2164`, `2173`-`2188`)

| Number | `ProtoOAPayloadType` | Implemented by |
|---|---|---|
| 2121/2122 | `..._TRADER_REQ`/`RES` | `Client::trader` |
| 2123 | `..._TRADER_UPDATE_EVENT` | `Event::TraderUpdated` |
| 2124/2125 | `..._RECONCILE_REQ`/`RES` | `Client::open_positions_and_orders` |
| 2133/2134 | `..._DEAL_LIST_REQ`/`RES` | `Client::deals` |
| 2142 | `..._ERROR_RES` | `error::OpenApiError::Server`, every call |
| 2143/2144 | `..._CASH_FLOW_HISTORY_LIST_REQ`/`RES` | `Client::cash_flow_history` |
| 2147 | `..._ACCOUNTS_TOKEN_INVALIDATED_EVENT` | `Event::TokensInvalidated`, `Session` |
| 2148 | `..._CLIENT_DISCONNECT_EVENT` | `Event::ServerDisconnecting` |
| 2149/2150 | `..._GET_ACCOUNTS_BY_ACCESS_TOKEN_REQ`/`RES` | `Client::accounts` |
| 2151/2152 | `..._GET_CTID_PROFILE_BY_TOKEN_REQ`/`RES` | `Client::ctid_profile` |
| 2162/2163 | `..._ACCOUNT_LOGOUT_REQ`/`RES` | `Client::logout_account` |
| 2164 | `..._ACCOUNT_DISCONNECT_EVENT` | `Event::AccountDisconnected` |
| 2173/2174 | `..._REFRESH_TOKEN_REQ`/`RES` | `Client::refresh_tokens`, `auth::OAuthClient::refresh` |
| 2175/2176 | `..._ORDER_LIST_REQ`/`RES` | `Client::orders` |
| 2179/2180 | `..._DEAL_LIST_BY_POSITION_ID_REQ`/`RES` | `Client::deals_by_position` |
| 2181/2182 | `..._ORDER_DETAILS_REQ`/`RES` | `Client::order_details` |
| 2183/2184 | `..._ORDER_LIST_BY_POSITION_ID_REQ`/`RES` | `Client::orders_by_position` |
| 2185/2186 | `..._DEAL_OFFSET_LIST_REQ`/`RES` | `Client::deal_offsets` |
| 2187/2188 | `..._GET_POSITION_UNREALIZED_PNL_REQ`/`RES` | `Client::position_unrealized_pnl` |

### Margin (`2139`-`2141`, `2167`-`2172`, `2177`-`2178`)

| Number | `ProtoOAPayloadType` | Implemented by |
|---|---|---|
| 2139/2140 | `..._EXPECTED_MARGIN_REQ`/`RES` | `Client::expected_margin` |
| 2141 | `..._MARGIN_CHANGED_EVENT` | `Event::MarginChanged` |
| 2167/2168 | `..._MARGIN_CALL_LIST_REQ`/`RES` | `Client::margin_calls` |
| 2169/2170 | `..._MARGIN_CALL_UPDATE_REQ`/`RES` | `Client::update_margin_call` |
| 2171 | `..._MARGIN_CALL_UPDATE_EVENT` | `Event::MarginCallUpdated` |
| 2172 | `..._MARGIN_CALL_TRIGGER_EVENT` | `Event::MarginCallTriggered` |
| 2177/2178 | `..._GET_DYNAMIC_LEVERAGE_REQ`/`RES` (message `ProtoOAGetDynamicLeverageByIDReq`/`Res`; the enum constant itself has no "ById") | `Client::dynamic_leverage` |

Every value of `ProtoOAPayloadType` (2100 to 2188, plus the proxy's own 50 and 51) is implemented;
there is nothing left unmapped or marked not applicable. The API itself exposes no message to
create a deposit or a withdrawal (only `ProtoOACashFlowHistoryListReq`, to read them, and
`ProtoOAExecutionEvent`'s `DEPOSIT_WITHDRAW` and `BONUS_DEPOSIT_WITHDRAW` execution types, to be
notified of one happening elsewhere), so there is no gap to fill there either. `wire.rs`'s test
`trading_account_and_margin_payload_numbers_match_the_official_enum` (and its older sibling
`payload_numbers_match_the_official_enum`) pin every number in this section against the value
above, so a typo cannot silently point a call at the wrong message.

## Design

```text
   your program
        |
        |  Session (reconnect, tokens, subscriptions)      optional
        v
     Client  ----  one WebSocket, one background task:
        |          reads frames, matches answers by clientMsgId, sends heartbeats,
        |          broadcasts events, fails waiting requests when the link ends
        v
   RateLimiter x2 (general, history)   ->   wss://{demo,live}.ctraderapi.com:5036
```

- **JSON, not Protobuf**: no code generation and no `.proto` files to vendor. The messages are the
  same; a Protobuf transport can replace it without touching callers.
- **One TLS stack**: `native-tls`, like the MCP client and `reqwest` in this workspace.
- **No `unsafe`**, and every public item is documented.
- **Secrets are `SecretString`s**: client secret and tokens are redacted by `Debug`, wiped on drop,
  and never put in an error message.
- **Failures are classified once**, in `error`, and the session and the retries read that
  classification instead of guessing.

## Sources

The official documentation at `help.ctrader.com/open-api` (getting started, messages, model
messages, symbol data, account authentication, register an application, proxies and endpoints,
protobuf and JSON), the `.proto` files at `github.com/spotware/openapi-proto-messages`, the forum
threads on tick timestamps and on missing bars, and the live runs recorded in `TODO.md`.
