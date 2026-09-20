# ctrader-openapi

A Rust client for the **cTrader Open API**, over its JSON WebSocket.

The two MCP servers cTrader offers (see [`ctrader-mcp`](../ctrader-mcp)) stop at one minute: no
tick history, no tick stream, no bars below M1. The Open API has all of that. This crate is a
small, typed, tested client for it: the connection, the OAuth 2 sign in, live prices, tick and
bar history, with the request matching, heartbeats and rate limits it takes to use it safely.

**Read only.** Nothing here places or changes an order.

## What it does

| | |
|---|---|
| Connection | `wss://demo.ctraderapi.com:5036` or `live`, JSON envelope, one connection per environment |
| Requests | Matched to answers by `clientMsgId`, many in flight at once, a timeout each |
| Limits | 50 requests per second, 5 per second for history, enforced by spacing (a request waits, it does not fail) |
| Keep alive | A heartbeat every few seconds (the server drops a connection silent for 10) |
| Sign in | The consent URL, a loopback web server for the redirect, code exchange, token refresh |
| Live data | Prices (`Event::Spot`), live bars, the order book (`Event::Depth`), account and token notices |
| History | Ticks (bid and ask, in windows under a week) and bars (14 periods), paged into whole ranges |
| Errors | One typed error, sorted into a few kinds, with `is_retryable` and the server's retry advice |

## Modules

| Module | Role |
|---|---|
| `client` | `Client`: the connection, one method per call, events, state |
| `history` | `fetch_ticks`, `fetch_bars`: whole ranges, page by page |
| `auth` | `authorization_url`, `OAuthClient` (`exchange_code`, `refresh`), `TokenSet` |
| `callback` | `CallbackListener`: catches the redirect of the consent page on `localhost` |
| `event` | `Event`, `DisconnectReason` |
| `types` | `Period`, `Bar`, `Tick`, `Quote`, `Spot`, price scale, `decode_ticks`, `merge_sides` |
| `model` | The messages as plain data (camelCase, integers read from numbers or text) |
| `wire` | The envelope and the payload type numbers |
| `config` | `Environment`, `ConnectionConfig`, `ClientCredentials` |
| `rate_limit` | `RateLimiter` |
| `error` | `OpenApiError`, `ErrorKind` |

## Using it

You need an application registered on the Open API portal (`https://openapi.ctrader.com/`), which
gives a client id and a client secret, and lets you register redirect URIs. Approval is by
Spotware and can take time. For a desktop program register `http://localhost:8765` (or another
port) as a redirect URI. Keep the secret out of files and logs: it belongs in the OS keyring or an
encrypted store (see `wyck-config`).

```rust
// 1. Sign the user in, once. Keep the tokens in a secret store afterwards.
let credentials = ClientCredentials::new(client_id, client_secret);
let listener = CallbackListener::bind(8765).await?;
let redirect = listener.redirect_uri();
let state = new_state();
open_in_browser(&authorization_url(&credentials.client_id, &redirect, Scope::Accounts, &state));
let code = listener.wait(&state, Duration::from_secs(300)).await?;
let tokens = OAuthClient::new(credentials.clone())?
    .exchange_code(code.code(), &redirect)   // the code lives one minute
    .await?;

// 2. Connect, identify the application, authorize an account.
let client = Client::connect(&ConnectionConfig::new(Environment::Demo)).await?;
client.authenticate_application(&credentials).await?;
let access = tokens.access_token.expose_secret();
let account = client.accounts(access).await?.ctid_trader_account[0].ctid_trader_account_id;
client.authorize_account(account, access).await?;

// 3. Live prices and history.
let mut events = client.events();
client.subscribe_spots(account, &[symbol_id], true).await?;
let ticks = fetch_ticks(&client, account, symbol_id, QuoteType::Bid, from_ms, to_ms).await?;
let bars = fetch_bars(&client, account, symbol_id, Period::M1, from_ms, to_ms).await?;
```

A complete, compiling version is in the crate docs (`cargo doc -p ctrader-openapi --open`).

## Things to know

- **Prices are integers** scaled by 100 000 (`1.08501` is `108501`). `types::to_price` converts.
- **Ticks** arrive newest first with their times as differences from the tick before. `decode_ticks`
  turns that into absolute, ascending times; the two sides (bid, ask) are separate requests and
  `merge_sides` joins them.
- **There is no volume per tick.** A bar's `volume` counts ticks. Only the order book has sizes.
- **Demo and live never mix.** An app that needs both opens two connections.
- **The client never reconnects by itself.** When the connection ends, waiting requests fail with
  `Closed` and a last `Event::Disconnected` is sent. Build a new `Client`, authenticate the
  application and the accounts again, and renew the subscriptions.
- **Tokens are secrets.** `Debug` never shows them, errors never repeat them, and the HTTP layer's
  errors are stripped of their URL, which holds the client secret and the code.

## Tests

```sh
cargo test -p ctrader-openapi          # about 100 tests, no network beyond localhost
cargo test -p ctrader-openapi --test live -- --ignored --nocapture   # a real demo account, see below
```

The regular tests run against a scripted local WebSocket server (`tests/support`) and a local
stand-in for the token endpoint. The **live** test reads from a demo account and needs an approved
application and an access token in environment variables (`WYCK_OPENAPI_CLIENT_ID`,
`WYCK_OPENAPI_CLIENT_SECRET`, `WYCK_OPENAPI_ACCESS_TOKEN`, optionally `WYCK_OPENAPI_SYMBOL`). It
refuses a live account and never writes anything.

## What has not been verified against a live server

This client was written from the official documentation and `.proto` files, without an approved
application to try it on. The live test exists to settle these, and the answers belong in
`TODO.md` section 2A:

- That the JSON endpoint accepts enumerations (periods, quote type) as numbers.
- Whether the consent page echoes the `state` parameter (the callback accepts a redirect without
  one and reports it, see `AuthorizationCode::state_echoed`).
- Which end of the range a truncated bar answer holds, and the range limit of a bar request per
  period (`history::fetch_bars` copes with either end).
- That the tick price is an absolute value and the tick time a difference (as the `.proto` comments
  say), and how far back a broker keeps ticks.
- The tick count per response, and the exact behavior at the boundary between two pages.

## Sources

The official documentation at `help.ctrader.com/open-api` (getting started, messages, model
messages, symbol data, account authentication, register an application, proxies and endpoints,
protobuf and JSON), the `.proto` files at `github.com/spotware/openapi-proto-messages`, and the
forum threads on tick timestamps and on missing bars.
