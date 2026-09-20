# Architecture

How the workspace crates fit together, and how a front end is meant to use them. Crates marked (planned)
do not exist yet. For what is left to build, see
[TODO.md](../TODO.md); for the rules of the repository, see [AGENTS.md](../AGENTS.md).

## The picture

```
   +-------------------------------------------------------------+
   |  your views (GPUI): position table, panels, styling         |
   |  the visual design is the owner's, not in the repository    |
   +-----------------------------+-------------------------------+
                                 |  plugged in at shell::run
   +-----------------------------v-------------------------------+
   |  wyck-app (exists): application layer + GPUI shell          |
   |  settings, startup, use cases, formatting, messages,        |
   |  hotkeys, logging, crash marker; window and plumbing        |
   +-----------------------------+-------------------------------+
                                 |  typed commands down, events up
                                 |  (in-process, on the engine's own runtime)
   +-----------------------------v-------------------------------+
   |  wyck-engine: headless core, no UI code                     |
   |  owns connections, refresh loops, order flow, guardrails    |
   +------+------------------+-------------------+---------------+
          |                  |                   |
          v                  v                   v
   +--------------+   +--------------+   +----------------+
   | wyck-config  |   | ctrader-mcp  |   | wyck-calendar  |
   | (exists)     |   | (exists)     |   | (exists)       |
   +------+-------+   +------+-------+   +--------+-------+
          |                  |                    |
          v                  v                    v
    OS keyring or       cTrader MCP          nfs.faireconomy.media
    encrypted file,     servers, Remote      (ForexFactory weekly
    config.toml         (cloud) or Local     JSON feed)
                        (desktop app)
```

Rules that keep this shape:

- The crates at the bottom (`wyck-config`, `ctrader-mcp`, `ctrader-openapi`, `wyck-calendar`) never depend on each other, and never on anything above
  them. Each can be tested, documented and reused alone.
- No UI toolkit appears below the GUI. That is what makes the GUI replaceable (the
  ratatui TUI was removed for exactly this reason) and what makes a headless mode possible.
- All trading logic lives at or below the engine. The GUI only turns key presses into
  commands and events into pixels.
- The engine is the only place that knows about more than one of those crates.

## The crates

### `ctrader-mcp` (exists)

Typed Rust client for cTrader's two MCP servers, built on `rmcp`.

| Module | Role |
|---|---|
| `remote::RemoteClient` | Cloud server (`mcp.ctrader.com`), bearer token, account-scoped |
| `local::LocalClient` | Server inside cTrader Desktop, no token, also drives charts, drawings, indicators, watchlists |
| `transport`, `config`, `retry` | Streamable HTTP + SSE session, connection settings, retry with backoff (reads only, never a mutating call) |
| `error` | `CTraderError` with self-healing classification of server rejections |
| `math` | Pure pip, unit and risk-based position sizing math |
| `quirks` | Named recovery patterns for known server bugs (safe amend, relative SL/TP, history chunking) |
| `workflows` | Composed flows: session bootstrap, sizing, pre-trade briefing, cost comparison, safe flatten, backfill |

Used by: the engine, for everything that touches the broker. The DTOs follow what the live
servers really send, which differs from the documentation in places (see `TODO.md` 3.8).
The `raw_call` example prints raw server answers, which is how those differences were found.

### `ctrader-openapi` (exists, not wired in yet)

Typed Rust client for the cTrader Open API over its JSON WebSocket, built for what the MCP
servers cannot give: real ticks, tick history, bars of fourteen periods, live bars and the order
book. Read only. It shares nothing with `ctrader-mcp` and, like the other crates at the bottom of
the picture, depends on none of the others. Details in
[../crates/ctrader-openapi/README.md](../crates/ctrader-openapi/README.md).

| Module | Role |
|---|---|
| `client` | `Client`: one WebSocket, requests matched by `clientMsgId`, heartbeat, events, state, retries of rate limit refusals |
| `session` | `Session`: reconnects with backoff, signs in again, restores subscriptions, renews tokens through a `TokenStore` |
| `history` | `fetch_ticks` and `fetch_bars`: whole ranges, page by page, windows under the server's limits |
| `account`, `handle` | Read only account data (balance, positions, orders, deals, catalogs) and `AccountClient` |
| `market` | Symbol lookup, latest quote per symbol, an order book, price formatting |
| `auth`, `callback` | OAuth 2: consent URL, loopback redirect listener, code exchange, token refresh |
| `types`, `model`, `wire` | Periods, bars, ticks, quotes; the messages; the envelope and payload numbers |
| `rate_limit`, `error`, `config`, `event` | Request limits, typed errors, settings, unsolicited messages |

- **Limits**: 50 requests per second per connection, 5 for history; the limiter spaces requests
  so a burst waits instead of failing. A heartbeat keeps the connection under the server's 10
  second silence limit.
- **Reconnection lives in `Session`**, not in `Client`: a client is one connection, and a `Session`
  (used by the engine, or by anything that stays up) owns backoff, signing in again, restoring
  subscriptions and renewing the tokens.
- **Tested hard**: over 200 tests (mock server, stand-in token endpoint, property tests, robustness
  tests) and an ignored live test for a demo account.
- **Secrets**: client secret and tokens are `SecretString`s, `Debug` redacts them, and errors from
  the HTTP layer are stripped of their URL (which carries the secret and the code).
- **Not yet verified live**: it was written from the documentation without an approved
  application. `tests/live.rs` (ignored) settles the open points; they are listed in the README and
  in `TODO.md` section 2A. The engine `MarketData` port, the tick store and the connection screen
  are the next steps there.

### `wyck-config` (exists)

Application settings and credentials, shared by every front end.

- `AppConfig`: human-editable TOML with connection profiles (name, service tag, endpoint).
  Never contains a token.
- `SecretStore`: tokens, held in memory as `SecretString`, persisted in the OS keyring
  (`KeyringSecretStore`) or an Argon2id plus ChaCha20-Poly1305 file
  (`EncryptedFileSecretStore`) where no keyring exists.
- `WyckConfig`: the facade. Load once at startup, list or add profiles, fetch a token.

Used by: the GUI, for its first-run and account-management screens, and by the engine only
through `ConnectRequest::from_profile`, which turns a profile and its token into a connection
request. The engine never edits or stores profiles.

### `wyck-calendar` (exists)

Economic calendar from the ForexFactory weekly feed.

- `parse_feed`, `CalendarEvent`, `Impact`, `Scope`, `Currency`, `Reading`: pure model and
  decoding.
- `EventFilter`: impact, currency and watchlist filtering; `currencies_from_symbols`
  derives "the currencies I trade" from the session's symbol list.
- `imminent_events`: non-blocking warnings before high-impact releases.
- `CalendarService`: background refresh tuned to the feed's rate limit, stale data kept
  when a refresh fails.

Used by: the engine, which starts the service once, feeds the filter from the symbols being
traded, and exposes the result as part of its state. The engine hosts it, rather than the GUI,
because news warnings are guardrails: they depend on the traded symbols, the clock and the
armed state, and the feed's rate limit calls for a single owner. The calendar is optional
(`calendar_enabled`, or an injected `CalendarSource`); the engine works without it.

### `wyck-engine` (exists)

The headless core. Full details in the crate docs (`cargo doc -p wyck-engine --open`).

| Module | Role |
|---|---|
| `engine` | `Engine` (owns a Tokio runtime) and `EngineHandle` (the cloneable API a front end holds) |
| `core` | Session supervision: connect, refresh, ping, reconnect with backoff, reconciliation |
| `state`, `event` | The immutable `EngineState` snapshot and the `Event` stream |
| `risk` | `build_plan`, a pure function from an intent and a quote to an exact, validated order |
| `trading` | The order pipeline: arming, plans, in-flight gate, confirmation, flatten |
| `guardrails`, `news` | Warnings only; news hosts `wyck-calendar` |
| `broker` | The `Broker` port, its Remote and Local adapters, and `MockBroker` |
| `domain`, `config`, `error`, `ids` | Normalized types, configuration, errors, identifiers |

- **State and events**: one immutable `EngineState` on a `watch` channel, plus a bounded
  broadcast of `Event`s. Both are plain `tokio::sync` primitives, so any executor can await
  them. A slow subscriber that lags re-reads the state.
- **Threads**: the engine runs on its own Tokio runtime, so it can sit in the GUI's process
  today and behind an RPC boundary later without changing the GUI.

Not done yet: pending orders, several simultaneous accounts, margin checks, order history,
journal and backtesting (see `TODO.md`, section 5).

### `wyck-app` (exists)

The application layer and a GPUI shell. `cargo run -p wyck-app` opens an empty dark window wired
to the engine: it follows the engine's state, opens the connection, and registers global
shortcuts that plan **dry-run** orders. It has no visuals; the owner's views plug in at one place,
`shell::run` in `src/main.rs`. Details in [../crates/wyck-app/README.md](../crates/wyck-app/README.md).

Everything below the shell is plain Rust, tested without a window: `settings`, `startup`,
`controller`, `presentation` (state to strings and tones, never colors), `model`, `messages`
(errors to notices, in one place), `hotkeys`, `logging` and `session_marker`. The GPUI part is
behind the `gui` feature, so `--no-default-features` builds the layer with no windowing toolkit.

It depends on `wyck-engine`, `wyck-config` and `gpui-kit`, and never on `ctrader-mcp` or
`wyck-calendar` directly. It talks to the engine only through `EngineHandle`.

#### The price chart

The chart is `wyck-app`'s biggest feature. Its logic is in `src/chart/`, plain Rust tested without a
window (about 110 tests); `src/ui/chart.rs` only draws it with GPUI's low level painting.

| Module | Role |
|---|---|
| `series` | Bars of one symbol and period: merge, the live bar, the memory cap |
| `viewport` | Zoom (bar spacing) and pan (right offset), measured from the right edge |
| `scale` | The price range: auto or manual, linear or logarithmic, round grid steps |
| `timeaxis` | Time labels with day, month and year boundaries, stable while dragging |
| `lod` | Candles, lines or one column per pixel by zoom, snapped to whole pixels |
| `interaction` | `ChartModel`: what each wheel turn, drag, double click and key does |
| `coverage`, `history` | Which time ranges were fetched; what to ask the server for next |
| `store` | The bars on disk, in one redb file (`<data dir>/cache/candles.redb`) |

- **Data**: `Broker::bars` and `EngineHandle::candles` return bars for a span. Remote pages through
  `backfill_trendbars` (the server caps a call at 720 hours for every period, wants ISO 8601
  times, and answers at most 100 bars per call, the newest ones of the window, without saying so:
  the rest is fetched by moving the upper bound back), Local in windows of 1000 bars. Each time frame is fetched natively, never rebuilt from
  smaller ones, so the broker's own session and time zone cuts are kept.
- **Cache**: memory for what is drawn, redb for what was downloaded. Only closed bars are stored
  (the forming one is fetched again), and a coverage list records the ranges already asked for, so
  a weekend is never requested twice. The second launch draws at once and fetches only the tail.
- **Live**: the engine polls quotes once a second, so the bid is folded into the newest bar, and every
  15 seconds the tail is fetched again so the server's own bar replaces the one built from quotes.
- **Pages**: a request spans at most about six 720-hour windows, so weekly and monthly pages are
  short, and a scroll loads as many pages as the screen needs.

## The broker port

`Broker` is one trait behind `dyn`, implemented by the Remote adapter, the Local adapter and
`MockBroker`. Everything the engine knows about a broker goes through it, and every dialect
difference is absorbed in the adapters, so the engine sees one model: display-value prices and
money, exact integer volumes, ticker names.

| | Remote | Local |
|---|---|---|
| Quote prices | integers, always in units of 1e-5 | display floats |
| Position and order prices | display floats | display floats |
| Volume on the wire | hundredths of a unit | units, with `volumeType` |
| Volume rules, digits | not published (configured or inferred) | from `get_symbol_details` |
| Server clock | none | `get_server_time` |
| Order label on positions | not echoed | echoed |

The full table, with the reasons, is in the `wyck-engine` crate docs. Two consequences worth
knowing: volumes are kept in hundredths of a base-asset unit (BTCUSD trades in steps of 0.01),
and an order whose outcome is unknown is recognized later by its label where the server echoes
it, else by symbol, side and volume.

## Safety model

The engine can move real money, so the guarantees are structural, not a matter of care:

- **Dry-run by default.** A plan is fully prepared and not sent until trading is armed.
- **Explicit arming.** Arming names the account and acknowledges its kind (demo, live, or
  unknown). Any session problem disarms.
- **Single-use plans.** A plan is priced from a live quote, expires, and is consumed by its
  first submission. One order in flight per symbol, with a minimum interval.
- **Never replay a mutating call.** A lost reply is settled by reading positions back, never
  by sending the order again. If that fails too, the outcome is `Unknown`, a warning stays up,
  and later refreshes reconcile it.
- **Two-step flatten.** A preview returns a single-use token that the flatten must present.
- **Warn, never block.** Guardrails add sentences to a plan. Only structural problems refuse.

## Testing

| Layer | What it covers |
|---|---|
| Unit and property tests | Sizing arithmetic, volume rounding, decoding of captured server answers |
| Broker contract suite | One behavior contract run against `MockBroker` and the real Remote adapter talking to a scripted in-process MCP server that mimics the live answers |
| Scenario tests | The engine on paused time with `MockBroker`: reconnects, lost replies, reconciliation, arming |
| Live tests | `#[ignore]`d, gated by environment variables, demo accounts only; they place and close real orders (`TODO.md` 3.1) |

CI runs everything except the live tests.

## Data flows

**Hotkey order (the point of the app)**

```
arm (once): account + acknowledged kind -> mode Armed
key press -> GUI command
          -> engine.plan_entry(intent): quote, account, instrument -> risk::build_plan
                                        -> OrderPlan (volume, stop, risk, warnings)
          -> engine.submit(plan): checks (armed, ready, not stale, not in flight)
          -> Broker::place_market (no retry, no replay)
          -> engine re-reads positions to confirm
          -> event OrderResult -> GUI feedback (filled, rejected, unknown, dry-run)
```

**Startup and credentials**

```
wyck-app: wyck-config loads AppConfig -> active profile -> SecretStore::token
  -> ConnectRequest -> engine.connect
  -> Broker adapter: MCP session, bootstrap (build id, account, symbols)
  -> session Ready
```

**News**

```
wyck-calendar service --(watch channel)--> engine
engine: filter by currencies_from_symbols(watched symbols)
engine: imminent_events(now) --> Warning in EngineState --> GUI banner
```

`now` is the broker's server time where there is one (Local). Remote has no clock, so the
local clock is used there.

## Open decisions

- Whether GPUI holds up on the rest of the Windows requirements: a focus-free floating panel and
  key repeat. Global hotkeys and the async bridge are already proven. egui is the fallback
  (`TODO.md` 4.1).
- Whether the engine stays in the GUI's process or moves behind an RPC boundary. In-process
  first; `EngineHandle` is the API either way (`TODO.md` 4.3, 5.14).
- Whether news stay hosted by the engine once a journal or a backtest exists. The likely
  answer is a small "event source" trait injected into the engine, with the live calendar as
  the default implementation (`TODO.md` 10, 11).

The decisions already taken, and why, are in `TODO.md` section 12.2.

## Build order

1. Validate order placement live on demo accounts (done, `TODO.md` section 3).
2. `wyck-engine` around the three crates, testable without any UI (done).
3. `wyck-app`: application layer and GPUI shell (done). Then the remaining GPUI checks (`TODO.md` 4.1) and the views.

Steps 2 and 3 can overlap once the engine's command and event types are fixed.
