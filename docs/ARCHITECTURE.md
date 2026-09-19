# Architecture

How the workspace crates fit together, and how they are meant to be used once the GUI
exists. Crates marked (planned) do not exist yet. For what is left to build, see
[TODO.md](../TODO.md); for the rules of the repository, see [AGENTS.md](../AGENTS.md).

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

### `wyck-app` (planned)

The desktop GUI, built on GPUI with `gpui-component` (decision in
[decisions/0001-gui-framework.md](decisions/0001-gui-framework.md), dependency policy in
[gpui-dependency.md](gpui-dependency.md)). It talks only
to the engine. It renders state, captures hotkeys, and never calls `ctrader-mcp`,
`wyck-config` or `wyck-calendar` directly except for profile management (`wyck-config`) and
pure helpers such as formatting.

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
GUI: wyck-config loads AppConfig -> active profile -> SecretStore::token
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

- Whether GPUI holds up on the Windows requirements (global hotkeys, focus-free floating panel,
  key repeat), which the validation spike decides, with egui as the fallback (`TODO.md` 4.1).
- Whether the engine stays in the GUI's process or moves behind an RPC boundary. In-process
  first; `EngineHandle` is the API either way (`TODO.md` 4.3, 5.14).
- Whether news stay hosted by the engine once a journal or a backtest exists. The likely
  answer is a small "event source" trait injected into the engine, with the live calendar as
  the default implementation (`TODO.md` 10, 11).

The decisions already taken, and why, are in `TODO.md` section 12.2.

## Build order

1. Validate order placement live on demo accounts (done, `TODO.md` section 3).
2. `wyck-engine` around the three crates, testable without any UI (done).
3. Validate GPUI with a spike in `wyck-app` (`TODO.md` 4.1), then build the app on top of it.

Steps 2 and 3 can overlap once the engine's command and event types are fixed.
