# Architecture

How the workspace crates fit together today, and how they are meant to be used once the
engine and the GUI exist. Crates marked (planned) do not exist yet.

## The picture

```
   +-------------------------------------------------------------+
   |  wyck-app (planned): native desktop GUI                     |
   |  hotkeys, position table, floating trade panel, news panel  |
   +-----------------------------+-------------------------------+
                                 |  typed commands down, events up
                                 |  (in-process channels first, RPC later)
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

- The three crates at the bottom never depend on each other, and never on anything above
  them. Each can be tested, documented and reused alone.
- No UI toolkit appears below the GUI. That is what makes the GUI replaceable (the
  ratatui TUI was removed for exactly this reason) and what makes a headless mode possible.
- All trading logic lives at or below the engine. The GUI only turns key presses into
  commands and events into pixels.
- The engine is the only place that knows about more than one of the three crates.

## The crates

### `ctrader-mcp` (exists)

Typed Rust client for cTrader's two MCP servers, built on `rmcp`.

| Module | Role |
|---|---|
| `remote::RemoteClient` | Cloud server (`mcp.ctrader.com`), bearer token, account-scoped, rate limited to 5 req/s on history endpoints |
| `local::LocalClient` | Server inside cTrader Desktop, no token, also drives charts, drawings, indicators, watchlists |
| `transport`, `config`, `retry` | Streamable HTTP + SSE session, connection settings, retry with backoff (reads only, never a mutating call) |
| `error` | `CTraderError` with self-healing classification of server rejections |
| `math` | Pure pip, unit and risk-based position sizing math |
| `quirks` | Named recovery patterns for known server bugs (safe amend, relative SL/TP, history chunking) |
| `workflows` | Composed flows: session bootstrap, sizing, pre-trade briefing, cost comparison, safe flatten, backfill |

Used by: the engine, for everything that touches the broker. Status: read paths verified
live on both servers. Order placement has not been verified live yet (see `TODO.md`, P0).

### `wyck-config` (exists)

Application settings and credentials, shared by every front end.

- `AppConfig`: human-editable TOML with connection profiles (name, service tag, endpoint).
  Never contains a token.
- `SecretStore`: tokens, held in memory as `SecretString`, persisted in the OS keyring
  (`KeyringSecretStore`) or an Argon2id plus ChaCha20-Poly1305 file
  (`EncryptedFileSecretStore`) where no keyring exists.
- `WyckConfig`: the facade. Load once at startup, list or add profiles, fetch a token.

Used by: the engine (to build a `ctrader_mcp::ConnectionConfig` for the active profile)
and by the GUI's first-run and account-management screens, through the engine.

### `wyck-calendar` (exists)

Economic calendar from the ForexFactory weekly feed.

- `parse_feed`, `CalendarEvent`, `Impact`, `Scope`, `Currency`, `Reading`: pure model and
  decoding.
- `EventFilter`: impact, currency and watchlist filtering; `currencies_from_symbols`
  derives "the currencies I trade" from the session's symbol list.
- `imminent_events`: non-blocking warnings before high-impact releases.
- `CalendarService`: background refresh tuned to the feed's rate limit, stale data kept
  when a refresh fails.

Used by: the engine, which starts the service once, feeds the filter from the
`ctrader-mcp` symbol list, and forwards `CalendarState` changes and warnings to the GUI.

### `wyck-engine` (exists, first version)

The headless core. It replaces what `crates/wyck/src/engine.rs` did in the TUI, with far
more scope. Full details in the crate docs (`cargo doc -p wyck-engine --open`).

- **Session**: connects (Remote or Local, from a `ConnectRequest`, which can be built from a
  `wyck-config` profile), refreshes account, positions and quotes, pings, reconnects with
  backoff, and disarms trading whenever the session is not `Ready`.
- **State and events**: one immutable `EngineState` snapshot on a `watch` channel, plus a
  bounded broadcast of `Event`s. Both use plain `tokio::sync` primitives, so any executor can
  await them.
- **Planning**: `plan_entry` turns "side, stop, risk %" into an exact, validated order
  (`risk::build_plan`, a pure function).
- **Pipeline**: dry-run by default, explicit arming, single-use plans, one order in flight per
  symbol, no replay of mutating calls, label-based reconciliation of uncertain outcomes,
  two-step flatten.
- **Guardrails and news**: warnings only. Hosts `wyck-calendar` and warns about high-impact
  releases for the currencies being traded.
- **Brokers**: a `Broker` trait with a Remote adapter, a Local adapter and a scriptable
  `MockBroker` (feature `testing`).

The engine does not manage profiles: a front end reads and edits them with `wyck-config`
directly and hands the engine a `ConnectRequest`. It runs on its own Tokio runtime, so it can
sit in the GUI's process today and behind an RPC boundary later without changing the GUI.

Not done yet: verification of order placement on a live server, pending order placement,
several simultaneous accounts, margin checks, journal and backtesting.

### `wyck-app` (planned)

The desktop GUI. Framework not decided yet (GPUI is the leading candidate). It talks only
to the engine. It renders state, captures hotkeys, and never calls `ctrader-mcp`,
`wyck-config` or `wyck-calendar` directly except for pure helpers such as formatting.

## Data flows

**Hotkey order (the point of the app)**

```
key press -> GUI command PlaceOrder{side, stop_loss, risk}
          -> engine: sizing (ctrader-mcp math + workflows::sizing)
          -> engine: guardrail check (calendar warnings, prop-firm rules) -> warn only
          -> ctrader-mcp: create_order / place_market_order (no retry on mutating calls)
          -> engine: re-read positions to confirm
          -> event OrderResult -> GUI feedback (pending, filled, rejected)
```

**Startup and credentials**

```
wyck-config: load AppConfig -> pick active profile -> SecretStore::token
          -> ctrader-mcp ConnectionConfig -> McpSession::connect
          -> workflows::bootstrap (build id, account, symbols)
```

**News**

```
wyck-calendar service --(watch channel)--> engine
engine: filter by currencies_from_symbols(session symbols)
engine: imminent_events(now = broker server time) --> event NewsWarning --> GUI banner
```

## Build order

1. Verify order placement live on a demo account (`ctrader-mcp`, no UI needed).
2. `wyck-engine` around the three crates, testable without any UI (done, first version).
3. Create `wyck-app` on top of it.

Steps 2 and 3 can overlap once the engine's command and event types are fixed.
