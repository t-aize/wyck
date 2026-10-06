# cTrader connection audit

Crate: `crates/wyck-openapi`. Wire format: JSON over WebSocket on port 5036 (`config.rs:7-8`, endpoint `wss://{host}:5036`, `config.rs:36-40`), connected with `tokio_tungstenite::connect_async` (`transport/connection.rs:240-243`). No `prost`, no `.proto`, no `build.rs` anywhere in the workspace. The protobuf assumptions of the original brief do not apply.

## Message model

- `Envelope { client_msg_id: Option<String>, payload_type: u32, payload: serde_json::Value }` (`transport/wire.rs:279-291`).
- About 80 hand written `pub const` payload types in `wire.rs::payload` (`:23-219`), pinned against the official enum by a test (`wire.rs:406-461`).
- Each message is a hand written serde struct (`transport/messages.rs`, `market/*`, `account/requests.rs`, `trading/requests.rs`, `margin/*`). Integers may arrive as numbers or strings (`flex::int/opt/list`, `wire.rs:222-277`).
- Events: `#[non_exhaustive] enum Event` (`event.rs:41-82`), decoded by a `match` on `payload_type` (`event.rs:88-163`).
- Weak point: `Client::call<Req, Res>(request_type, response_type, ...)` takes two `u32` (`connection.rs:325-332`). Nothing stops a wrong pair; only `payload_type` is checked at run time. `Envelope::decode` clones the payload (`wire.rs:331`).

## Requirement table

| Requirement | Status | Evidence |
|---|---|---|
| Explicit state machine | Partial | `ConnectionState { Connected, Closed(reason) }` (`connection.rs:79-84`) plus `AtomicBool closed` (`:98`). `SessionState` has 5 states (`session/mod.rs:139-156`) but `supervise` is an imperative loop with flags `force_refresh`, `refreshed_for_invalid`, `attempt` (`:669-673`) |
| App auth then account auth | Present | `authenticate_application` (`connection.rs:483`), `authorize_account` (`:526`), chained in `session::connect` (`session/mod.rs:892-909`); order is a caller convention, not enforced by types |
| Heartbeat | Present | Sent every 5 s by default (`config.rs:88`); `validate()` rejects 10 s or more (`:117`); loop at `connection.rs:717-734` |
| Liveness detection | Absent | Inbound heartbeats are traced and ignored (`connection.rs:133-136`). No inbound silence timeout; only write failure, the 30 s per request timeout and TCP close. A half open link with no request in flight is not noticed. No `watchdog`, `last_seen` or `idle` in the code |
| Correlation, timeout, cleanup | Present | UUID `clientMsgId` (`:435`), `pending: Mutex<HashMap<String, oneshot::Sender>>` (`:94`), `PendingRequest` drop guard (`:110-120`), timeouts (`:450-472`), `finish()` empties waiters with `Error::Closed` (`:158-169`); tests `cancelled_requests_leave_no_pending_waiters` (`:773`) and `tests/robustness.rs` |
| Separate event channel | Present | `broadcast::Sender<Event>` capacity 8192 (`:95`, `config.rs:89`) |
| Rate limits | Present, not a token bucket | `RateLimiter` even spacing plus a burst of about 0.2 s (`rate_limit.rs:23-72`), defaults 40/s and 4/s (`config.rs:90-91`) chosen after a live `BLOCKED_PAYLOAD_TYPE` (`config.rs:63-66`); `RateClass::Historical` on trendbars, ticks, deals; bounded retry after `retryAfter` (`connection.rs:337-366`) |
| Reconnect, backoff, jitter | Present, weak jitter | `Backoff` 1 s to 60 s, factor 2 (`session/backoff.rs:16-24`); additive jitter 0 to 20 % from clock nanoseconds (`:54-58`) |
| Resubscribe after reconnect | Present | `restore_subscriptions` (`session/mod.rs:892-939`), `BTreeSet` registry for spots, live bars, depth (`:193-197`) |
| Reconciliation | Absent in the crate, present in the app | `Lagged` only logs (`session/mod.rs:848-850`); app reconciles on `SessionEvent::Ready` (`wyck/src/dashboard/mod.rs:478`, `trading/account.rs:652`); the app ignores `RecvError::Lagged` (`dashboard/mod.rs:468`) |
| `ProtoOAAccountDisconnectEvent` | Present | Re-auth on the same connection, then restore subscriptions, fallback to full reconnect (`session/mod.rs:828-838,865-889`) |
| `ProtoOAClientDisconnectEvent` | Present | `connection.rs:754-762` closes with `ServerAnnounced`; `serve` forces a full reconnect (`session/mod.rs:841-846`) |
| Token invalidated and refresh | Present | `AccountsTokenInvalidated` forces a refresh and reconnect (`:816-823`); one forced refresh only (`:719-727`); refresh over HTTP GET (`auth/oauth.rs:254-300`), deliberately not cancellable (`session/mod.rs:684-690`); new pair persisted before use with a 30 s timeout (`:951-955`) |
| Typed errors | Partial | `#[non_exhaustive] Error` and `ErrorKind` with 10 classes (`error.rs:17-105`); server `code` is a `String` classified by literal match (`:145-176`), unknown codes become `Rejected` (`:175`) |
| Watchdog for a dead but chatty connection | Absent | see liveness |
| Transport abstraction | Partial | `run<S: AsyncRead + AsyncWrite + Unpin>` is generic (`connection.rs:659-665`) but `Client::connect` hardwires `connect_async` (`:240-243`); `Session` holds a concrete `Client` (`session/mod.rs:203`); `TokenStore` is a trait (`token_store.rs:17`); tests use a real local WebSocket server (`tests/support/mod.rs`) |
| Demo and live separation, 2 connections max | Partial | `Environment { Demo, Live }` with one host each (`config.rs:19-41`); one `Session` is one connection; no cap on connections, only the `CONNECTIONS_LIMIT_EXCEEDED` code is classed `RateLimited` (`error.rs:152`); an app side counter: not verified |
| Secrets | Good | `ClientCredentials.client_secret: SecretString` (`config.rs:132-137`), test `the_secret_never_shows_in_debug` (`:191`); `TokenSet` holds `SecretString` (`oauth.rs:90-102`); sensitive requests do not implement `Debug` (`messages.rs:9`); `without_url()` keeps the secret out of logged URLs (`oauth.rs:22-24,272-296`); no token log found. Reserve: request structs hold plain `String` tokens (`messages.rs:32,50,90`); the app copies secrets with `expose_secret().to_string()` (`browser_handoff.rs:131`, `authorizing.rs:44`) |

## Other findings

- `Client::close()` is `async` but only signals (`connection.rs:314-319`).
- The connection task `JoinHandle` is dropped (`connection.rs:274`); shutdown relies on the last clone's `Drop` (`:197-218`), which is tested.
- `Session` exposes three subscription kinds; anything else goes through `session.client()` (`session/mod.rs:63-69,338-344`).
- Double validation: `Client::connect` and `Session` both call `config.validate()` (`session/mod.rs:300`).
- `unwrap_or_else(PoisonError::into_inner)` is used consistently on mutexes; locks are short and not held across `.await` in what was read.
- Only `Arc<Mutex<Option<JoinHandle>>>` (`session/mod.rs:251`) plus std `Mutex` and `RwLock<Option<Client>>` (`:203-205`).
- Only unbounded channel is in a test (`connection.rs:777`) and in `tests/support`; production channel is `mpsc::channel(256)` (`connection.rs:65,256`).

## Prices, volumes, money (wire level)

- Tick, bar, spot and depth prices: `i64` scaled by 100000 (`market/price.rs:9`).
- Order and position prices: `Option<f64>` (`account/types.rs:357-425`), order request prices `f64`; relative stops `i64` at `PRICE_SCALE`.
- Volumes `i64` in hundredths of a unit (`types.rs:11,218`). Money `i64` scaled by `10^moneyDigits`, with `money_digits: Option<i64>` repeated in 6 structs.
- Enum fields arrive as raw `i64` (`trade_side: i64`, `types.rs:310`) with `kind()`, `side()`, `status()` accessors.
- Conversion lives in several places: `market/price.rs:21,31,48,76`, `account/types.rs:212,218`, `trading/contract.rs:102-110,131-138` (second `PRICE_DIGITS = 5`), `margin/types.rs`, `account/requests.rs:247`, and `Deal::realized_pnl` re-reads an untyped `Value` (`types.rs:497,524-530`). No `Price`, `Volume` or `Money` newtypes.

## What to keep

Transport and correlation (`PendingRequest`, `finish`, `RateLimiter` with paused-time tests), `Backoff` (proptest), `flex` and `Envelope`, `error.rs` classification with its pinned tests, secret handling, tick and bar decoding and `tick_windows` (proptest and live runs), and the whole test harness (`tests/support` plus `tests/session.rs`, `robustness.rs`, `properties.rs`) as the safety net for any refactor.

## Tests

About 5000 test lines: unit tests in most modules; `client.rs` 21 tests, `session.rs` 31, `robustness.rs` 13, `market.rs` 13, `trading.rs` 9, plus `account`, `margin`, `auth`, `handle`; `properties.rs` 16 properties; `live.rs` 13 tests, all `#[ignore]`, order tests need `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1` (`live.rs:775-781`). Not covered: inbound silence, session reaction to `Lagged`, a demo plus live pair. No in-memory fake transport; tests need a real local socket. Whether integration tests use paused time: not verified.

Gaps and actions are in `PLAN.md` section 4.1 and tasks T-070 to T-078.
