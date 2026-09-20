# ctrader-openapi

A Rust client for the **cTrader Open API**, over its JSON WebSocket.

The two MCP servers cTrader offers (see [`ctrader-mcp`](../ctrader-mcp)) stop at one minute: no
tick history, no tick stream, no bars below M1. The Open API has all of that. This crate is a
typed, tested client for it, with what it takes to run for days unattended: request matching,
heartbeats, the documented rate limits and their retries, the OAuth 2 sign in, and a session that
reconnects and renews its tokens by itself.

**Read only.** Nothing here places, changes or cancels an order.

Contents: [What it does](#what-it-does) | [Quick start](#quick-start) | [Guides](#guides) |
[Examples](#examples) | [Configuration](#configuration) | [Errors](#errors) |
[Things to know](#things-to-know) | [Troubleshooting](#troubleshooting) | [Tests](#tests) |
[What is verified](#what-is-verified) | [Design](#design) | [Sources](#sources)

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
| Account | Balance, positions, working orders, deals, assets, symbol categories, the cTrader ID profile |
| Helpers | Symbol lookup by name, latest quote per symbol, an order book, price formatting, an account bound client |
| Errors | One typed error, sorted into a few kinds, with `is_retryable` and the server's retry advice |

## Quick start

You need an application registered on the Open API portal (`https://openapi.ctrader.com/`), which
gives a client id and a client secret, and lets you register redirect URIs. Approval is by
Spotware and can take time. For a desktop program register `http://localhost:8765` (or another
port) as a redirect URI. Keep the secret out of files and logs: it belongs in the OS keyring or an
encrypted store (see `wyck-config`).

1. **Sign in once** and get a token, from the terminal:

   ```powershell
   $env:WYCK_OPENAPI_CLIENT_ID = "..."; $env:WYCK_OPENAPI_CLIENT_SECRET = "..."
   cargo run -p ctrader-openapi --example sign_in
   ```

   It opens the browser, catches the redirect, trades the code, lists the accounts and prints the
   tokens (secrets: do not share them).

2. **Use the token** in the other examples:

   ```powershell
   $env:WYCK_OPENAPI_ACCESS_TOKEN = "..."
   cargo run -p ctrader-openapi --example account_info
   cargo run -p ctrader-openapi --example stream_prices -- EURUSD
   cargo run -p ctrader-openapi --example download_ticks -- EURUSD --from 2h --out ticks.csv
   ```

3. **Or write your own**, in a few lines:

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

   A complete, compiling version is in the crate docs (`cargo doc -p ctrader-openapi --open`).

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
```

Money amounts are integers scaled by `10^moneyDigits`; volumes are in hundredths of a unit; prices
of positions and orders are ordinary decimals. `account::money`, `volume_units` and the `Trader`
helpers convert. Enumerations are numbers with a `from_number` that gives `None` for a value this
crate does not know, so a server that adds one does not crash a program.

## Examples

Every example needs `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` and (except `sign_in`)
`WYCK_OPENAPI_ACCESS_TOKEN`.

| Example | What it does |
|---|---|
| `sign_in` | The whole OAuth flow from the terminal; prints the tokens |
| `account_info` | Balance, open positions, working orders, recent deals |
| `list_symbols [PREFIX]` | The symbols of the account, with ids and decimals |
| `stream_prices SYMBOL... [--seconds N] [--depth]` | Live prices with the spread, and the order book |
| `download_ticks SYMBOL [--from 2h] [--to now] [--out f.csv]` | Ticks as CSV, bid and ask side by side |
| `download_bars SYMBOL PERIOD [--from 2d] [--out f.csv]` | Bars as CSV (`M1` to `MN1`) |
| `resilient_stream SYMBOL...` | Live prices through a `Session`: cut the network and it comes back |

Times on the command line are `now`, Unix milliseconds, or a time ago (`30m`, `6h`, `2d`).

## Configuration

| Variable | Used by | Meaning |
|---|---|---|
| `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET` | all | The registered application |
| `WYCK_OPENAPI_ACCESS_TOKEN` | all but `sign_in` | The access token `sign_in` printed |
| `WYCK_OPENAPI_REFRESH_TOKEN` | `resilient_stream` | Lets the session renew the access token |
| `WYCK_OPENAPI_ACCOUNT_ID` | most | The trading account; default: the first demo account the token covers |
| `WYCK_OPENAPI_ENV` | all | `demo` (default) or `live`; a live account must be named explicitly |
| `WYCK_OPENAPI_PORT`, `WYCK_OPENAPI_SCOPE` | `sign_in` | Redirect port (default 8765) and `accounts` or `trading` |
| `WYCK_OPENAPI_SYMBOL` | live test | The symbol the live test reads (default `EURUSD`) |

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

## Tests

```sh
cargo test -p ctrader-openapi                       # over 200 tests, no network beyond localhost
cargo test -p ctrader-openapi --test live -- --ignored --nocapture   # a real demo account
```

| Suite | What it covers |
|---|---|
| unit tests (`src/`) | Every module: decoding, limits, errors, the callback listener, the session helpers |
| `tests/client.rs` | The client against a scripted WebSocket server: requests, errors, events, end of connection, history |
| `tests/account.rs`, `tests/handle.rs` | The account calls and the account bound client |
| `tests/session.rs` | Reconnection, restoring subscriptions, token refresh, failures, stopping |
| `tests/oauth.rs` | The token exchange against a stand-in endpoint, and that no error repeats a secret |
| `tests/properties.rs` | Properties on random inputs (proptest): tick round trips, windows, merging, the book, backoff |
| `tests/robustness.rs` | Nonsense frames, event bursts, 300 concurrent requests, races with a close, a vanishing peer |
| `examples/` | The command line helpers shared by the examples |
| `tests/live.rs` | Read only checks against a real demo account (ignored by default) |

The **live** test reads from a demo account and needs an approved application and an access token
in environment variables (`WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET`,
`WYCK_OPENAPI_ACCESS_TOKEN`, optionally `WYCK_OPENAPI_SYMBOL`). It refuses a live account and never
writes anything. It also cross checks bars against ticks and the page seams of the tick history.

## What is verified

Everything above the wire runs in tests against a local server. A live demo account confirmed the
connection, both sign in steps, the symbol list and details, the price subscription, tick and bar
history with paging (no tick lost at page seams), the tick encoding, bars against ticks, and the
rate limit behavior. **Not yet run live**: the account calls (`trader`, positions, deals, catalogs),
the `Session` against the real server, whether the consent page echoes `state` (the callback
accepts a redirect without one and reports it, see `AuthorizationCode::state_echoed`), the range
limit of bar requests per period, and how far back a broker keeps ticks. `TODO.md` section 2A.7
tracks them.

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
