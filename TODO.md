# wyck: TODO

The full working plan, as of 2026-09-19. It is written to be usable cold, in a fresh
conversation: every item says what to do, why it matters, where the code lives, and how to
know it is done. Layout and data flows are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md);
the rules for writing in this repository are in [AGENTS.md](AGENTS.md).

Contents

1. [Where things stand](#1-where-things-stand)
2. [Working rules](#2-working-rules)
3. [P0: live validation before any real order](#3-p0-live-validation-before-any-real-order)
4. [P1: decisions and foundations](#4-p1-decisions-and-foundations)
5. [P2: wyck-engine, what is left](#5-p2-wyck-engine-what-is-left)
6. [P3: the GUI](#6-p3-the-gui)
7. [P4: other crates](#7-p4-other-crates)
8. [Repository and infrastructure](#8-repository-and-infrastructure)
9. [Assumptions to confirm](#9-assumptions-to-confirm)
10. [Risks and open questions](#10-risks-and-open-questions)
11. [v1 vision](#11-v1-vision)
12. [Reference](#12-reference)

---

## 1. Where things stand

| Crate | State | Notes |
|---|---|---|
| `ctrader-mcp` | Complete for the documented tools | Read paths verified live on Remote and Local. Mutating calls (orders) never verified live. Has a `test-support` feature exposing the mock MCP server. |
| `wyck-config` | Complete, tested | Profiles in TOML, tokens in the OS keyring or an encrypted file. |
| `wyck-calendar` | Complete, tested | ForexFactory weekly feed: client, tolerant parser, filters, refresh service, alerts. Tuned to the feed's real rate limit. |
| `wyck-engine` | First version, tested with mocks | Session, state and events, risk planning, dry-run-first order pipeline, guardrails, news hosting, Remote and Local adapters, `MockBroker`. Never run against a live server. |
| GUI (`wyck-app`) | Does not exist | Framework not chosen. The ratatui TUI was removed (`crates/wyck`, in git history before commit `9f5a8ba`). |

Other facts:

- Everything lives on `main`. CI (fmt, clippy with warnings denied, tests, docs with
  warnings denied, release build with `--locked`) runs on every push, on ubuntu and windows.
  The last push (`a1a0623`, the engine core) was **not confirmed green yet**: check
  `gh run list --branch main` first.
- Test suites: `cargo test --all-features` runs everything. The engine has unit tests, a
  broker contract suite (mock and real Remote adapter against a scripted MCP server), 27
  paused-time scenario tests, a news integration test and doctests.
- The engine has an example that needs no network:
  `cargo run -p wyck-engine --example dry_run_order --features testing`.

---

## 2. Working rules

From `AGENTS.md`, restated because they are easy to forget:

- Commit and push straight to `main`. No feature branches, no pull requests unless asked.
- No AI attribution in commit messages or PR descriptions (no `Co-Authored-By` line).
- Repository text stays plain ASCII (section sign excepted): no em or en dashes, arrows,
  ellipsis characters, curly quotes, emoji. Reword instead of swapping characters. The check
  command is at the bottom of `AGENTS.md`.

Before every commit:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps            # default features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo build --release --locked
```

Docs must build with and without `--all-features` (the `testing` feature hides
`MockBroker`; never link to feature-gated items with intra-doc links).

---

## 3. P0: live validation before any real order

Nothing that can send an order has ever touched a real server. This section is the gate.
**Do not arm the engine on a funded account until every item here is ticked.**

### 3.1 Preparation

- [ ] A **demo** cTrader account with AI Agent Connect enabled, and its Remote token.
      Confirm it is a demo token (the JWT payload has `"environment":"demo"`; the engine
      exposes this as `AccountKind`).
- [ ] cTrader Desktop running with the Local MCP server enabled (Advanced > MCP Server, default
      `http://127.0.0.1:9876/mcp/`), logged into the same demo account.
- [ ] A scratch place for the token that is not the repository: an environment variable, or the
      keyring through `wyck-config`. Never commit or paste a token into a file.
- [ ] Decide how live tests are gated: `#[ignore]` tests reading `WYCK_LIVE_REMOTE_TOKEN`,
      `WYCK_LIVE_LOCAL_ENDPOINT`, and a `WYCK_LIVE_CONFIRM_DEMO=1` switch that the test refuses
      to run without. Put them in `crates/wyck-engine/tests/live_*.rs`. CI never runs them.

### 3.2 Read-only checks, Remote (safe, do first)

Run `cargo run -p wyck-engine --example headless` with `WYCK_SERVICE=remote`.

- [ ] Connects and reaches `Ready`; account id, currency, balance, equity, free margin look right.
- [ ] `AccountKind` is `Demo`.
- [ ] `get_server_time` answer shape: what field holds the time, in what unit. Update
      `parse_server_time` in `crates/wyck-engine/src/broker/mod.rs` to match, and pin it with a
      test using the real response. (Currently a guess over several shapes.)
- [ ] Symbols list and `pipDigits` for EURUSD, USDJPY, XAUUSD, one index, one crypto: check
      `pip_size_for_digits` (`broker/remote.rs`) gives the right pip for each.
- [ ] **Volume rules.** The server publishes no lot size, minimum or step. Find the real ones
      for the symbols you trade (platform symbol info) and compare with `AssumedSpecs`
      (100,000 / 1,000 / 1,000). Decide the fix: per-symbol overrides in config (see 5.4).
- [ ] Quotes: bid/ask decode correctly (compare with the platform), `timestamp` unit is
      milliseconds.
- [ ] Does `get_positions` echo the order `label` or `comment` on positions? The engine's
      reconciliation of uncertain orders relies on it (`decode_position` reads `label` then
      `comment` from the extra fields). If not echoed, reconciliation falls back to symbol,
      side and volume matching; check that is good enough.
- [ ] Pending orders decode: `stopLoss` and `takeProfit` on orders are read from the raw extra
      fields (`decode_order`); confirm the field names.
- [ ] `unrealized_pnl`, `swap`, `commission` scaling (money digits) on a position with a floating
      result.

### 3.3 Read-only checks, Local

Run the headless example with `WYCK_SERVICE=local`.

- [ ] Connects; `get_balance` fields (`currency`, `accountType`, `traderId`, `margin`,
      `marginLevel`) match what `LocalBroker::account` reads. Confirm `AccountKind` detection from
      `accountType` (currently a guess: contains "demo", "live" or "real").
- [ ] `get_symbol_details`: real `lotSize`, `minVolume`, `volumeStep`, `digits`, `pipSize` for
      EURUSD, XAUUSD, an index. Check `instrument_from_details` (`broker/local.rs`), especially the
      **volume unit** (section 9, item A1).
- [ ] `get_spot_prices` per symbol works, and what a symbol without a quote returns (the adapter
      treats a `Rejected` error as "no quote").
- [ ] `get_positions` field names (`id`, `symbolName`, `tradeSide`, `volume`, and the P&L key: the
      adapter tries `unrealizedPnl` then `netProfit`).
- [ ] `get_server_time` shape (same as Remote).

### 3.4 Order lifecycle on the demo account

Only after 3.2 and 3.3. Use the smallest volume the symbol allows.

- [ ] Engine armed with the correct `acknowledged_kind`. Wrong kind and wrong account are refused.
- [ ] **Remote market order with stop loss and take profit** through `plan_entry` and `submit`:
      the position appears, volume is right in the platform, SL and TP are at the planned prices
      (the engine sends relative distances in points).
- [ ] The order's `label` shows up in the platform (comment or label field).
- [ ] `set_protection` changes only one leg; the other is kept (Remote quirk Q-R10).
- [ ] Partial close, then full close. Volume step respected.
- [ ] `flatten` with two open positions: preview, token, execute, report.
- [ ] Same list on **Local**: market order with pip-distance SL/TP, protection change, partial
      and full close, flatten (Local has no volume-less close on Remote's model but does have
      `close_all_positions`; the engine currently closes item by item).
- [ ] Rejected order (for example a volume below the minimum forced past the planner): outcome is
      `Rejected` with a readable reason, and no position appears.
- [ ] Order while the market is closed: what error the servers return, and whether
      `certainly_not_sent` (`crates/wyck-engine/src/trading.rs`) classifies it correctly.

### 3.5 Failure behavior, for real

- [ ] Kill the network during a session: state goes `Reconnecting`, trading disarms, the session
      recovers when the network returns, and positions are reconciled.
- [ ] Interrupt cTrader Desktop while connected to Local: same expectations.
- [ ] Lost reply for a real order is hard to reproduce; at minimum verify that a timeout shorter
      than the server latency (set `trading.order_timeout` very low on the demo) yields `Unknown`
      or `Filled` through confirmation, never a second order.
- [ ] Rate limits: run the headless example for a long time with several watched symbols and
      confirm no throttling errors (Remote reads are limited; the engine calls `get_positions`
      twice per refresh, see 5.9).

### 3.6 Exit criteria

- [ ] All boxes above ticked, or each unticked one has a written reason and a workaround.
- [ ] The live tests from 3.1 exist and pass against the demo account.
- [ ] Section 9 assumptions all confirmed or corrected in code, with tests pinning the real
      response shapes (use captured responses as fixtures, with tokens and account ids removed).
- [ ] `crates/ctrader-mcp/examples/probe_remote.rs` and `probe_local.rs` extended with an
      order round trip guarded by the same demo confirmation switch.

---

## 4. P1: decisions and foundations

### 4.1 Choose the GUI framework

GPUI is the leading candidate; the choice is open. Do a time-boxed evaluation (one day per
candidate at most) and record the result in `docs/`.

Criteria, with what to check:

- [ ] **Maturity and API stability.** GPUI is the framework of the Zed editor. Check how it is
      consumed today (Zed repository, published crate, community fork), release cadence, breaking
      changes over the last months, and the state of its documentation and examples.
- [ ] **Windows support** (this is the development machine): rendering backend, IME, high DPI,
      multi-monitor, always-on-top windows, borderless windows.
- [ ] **Async model.** GPUI's executor is not Tokio (source: the Zed "Async Rust" post and the
      `gpui-tokio-bridge` crate). The engine already works from any executor; confirm the pattern:
      `cx.spawn` awaiting `watch_state().changed()` and `EngineHandle` calls.
- [ ] **Tables and text.** A position table, a news table, a log panel: virtualized lists, cell
      formatting, selection, keyboard navigation.
- [ ] **Keyboard model.** Global hotkeys while another window (the chart platform) has focus? A
      hotkey-driven app that sits on top of cTrader Desktop needs this. Check what each framework
      can do and what needs OS-specific code.
- [ ] **Floating, always-on-top, click-through-free panel** for the instant-trade panel (section 6).
- [ ] **Custom drawing**, for price charts, order-flow displays and small sparklines. The biggest
      risk with GPUI: charts must probably be drawn by hand. Estimate the cost of a candlestick
      chart with pan and zoom.
- [ ] **Theming** (dark first) and font handling.
- [ ] **Testing story**: headless rendering, screenshot tests, interaction tests.
- [ ] **Packaging**: installers, code signing, auto-update, binary size.

Candidates to compare with GPUI: Slint, Iced, egui (with eframe), Tauri with a web UI (best
chart ecosystem, heaviest runtime), Dioxus desktop.

Deliverable: a decision record with the pick, the rejected options and why, and the known
risks. Then tick 4.2.

- [ ] Decision made and recorded.

### 4.2 Create `wyck-app`

- [ ] New crate `crates/wyck-app`, depends only on `wyck-engine`, `wyck-config` and the chosen UI
      toolkit. It must not depend on `ctrader-mcp` or `wyck-calendar` directly (pure formatting
      helpers excepted).
- [ ] Thin binary, all logic testable without a window.
- [ ] Update `docs/ARCHITECTURE.md`: crate marked as existing.

### 4.3 Engine boundary

- [ ] Decide: engine in the app's process (works today, simplest) or a separate process behind an
      RPC. Recommendation: in-process first. `EngineHandle` is the API either way.
- [ ] If a separate process is wanted later: define the wire protocol from the existing serde
      types (`EngineState`, `Event`, `OrderPlan`, `EntryIntent`, ...), pick a transport (local
      socket, gRPC, or JSON over a pipe), and add a client crate implementing the same handle
      surface. See 5.14.

### 4.4 Application shell concerns (need an owner before the GUI grows)

- [ ] **Logging.** The engine and libraries emit `tracing` events and install no subscriber. The
      app installs one: file logging with rotation (the TUI used `tracing-appender`; the
      dependency was removed with it and can come back), an env filter, and a rule that tokens and
      account secrets never reach a log line. `ConnectRequest` and `SecretString` already redact
      themselves in `Debug`; keep it that way in any new type.
- [ ] **Engine configuration persistence.** `EngineConfig` is `serde` with defaults but nothing
      stores it. Add a section to `wyck-config`'s `AppConfig` (or a sibling file) and load it at
      startup; validate with `EngineConfig::validate` and show errors instead of failing silently.
- [ ] **Error presentation.** Map `EngineError::kind()` and `is_retryable()` to user messages once,
      in one place, instead of per screen.
- [ ] **Startup flow.** Load config, pick the active profile, build `ConnectRequest::from_profile`,
      connect, and show progress from `SessionState`. First run without a profile goes to the
      profile creation screen.
- [ ] **Crash safety.** A panic hook that logs, and on next start a notice if the last session
      ended with an armed engine or an `Unknown` order warning (persist those two facts).

---

## 5. P2: wyck-engine, what is left

Crate: `crates/wyck-engine`. Start with its crate docs (`cargo doc -p wyck-engine --open`).

### 5.1 Pending orders (limit, stop, stop-limit)

Only market orders are placed today. The engine can list and cancel working orders.

- [ ] Extend `Broker` (`src/broker/mod.rs`) with `place_pending(&PendingOrderRequest)` and
      `amend_pending(...)`. Remote: `CreateOrderParams::limit/stop` (`ctrader-mcp`), with absolute
      SL/TP allowed on non-market orders (Q-R4 only forbids them on MARKET). Local:
      `place_limit_order`, `place_stop_order`, `place_stop_limit_order`, `amend_order`.
- [ ] Planning (`src/risk.rs`): `EntryIntent` gains an entry type and an entry price. Risk is
      computed from the entry price, not the market. Validate the side of the market (a buy limit
      below the ask, a buy stop above it) and reject inverted orders.
- [ ] Pipeline (`src/trading.rs`): same guarantees as market orders (armed, single-use plan, in
      flight lock, no replay). Confirmation reads **pending orders**, matched by label.
- [ ] Expiration (`GOOD_TILL_DATE`): Remote wants integer epoch milliseconds only (Q-R2).
- [ ] Local quirk to verify live: SL and TP semantics on pending orders are asymmetric (Q-L2,
      `normalize_pending_order_take_profit` in `ctrader-mcp`).
- [ ] Scenario tests with `MockBroker`, contract tests for both adapters, docs.
- [ ] Events: `OrderPlanned` and `OrderResult` already exist; add a pending-order variant if the
      outcome type needs one (working, filled later).

### 5.2 Order and position management extras

- [ ] Move stop loss to break-even (with an optional offset), as one command.
- [ ] Trailing stop helper (server-side trailing is exposed by Remote through `trailingStopLoss`;
      Remote's `AmendPositionParams` supports it). Decide server-side versus engine-side.
- [ ] Close by percentage (25, 50 percent) with correct rounding to the step.
- [ ] Reverse position (close and open opposite) as one guarded command.
- [ ] Scale in: add to an existing position with re-sized stop.
- [ ] Bracket orders across several targets (partial take profits): needs order groups.
- [ ] Amend pending orders (price, SL, TP, expiry).

### 5.3 Sizing improvements

- [ ] **Cross-currency conversion.** Today only a direct or inverse spot pair to the account
      currency is used (`conversion_rate` in `src/trading.rs`). `ctrader-mcp` has
      `math::conversion::compute_chain` (multi-hop) that could be reused; needs a quote fetch for
      the chain's symbols.
- [ ] Sizing modes beyond risk percent and fixed volume: fixed lots by symbol preset, fixed
      dollar risk per symbol, volatility-based (ATR) stops.
- [ ] Commission and swap aware sizing (subtract expected commission from the risk budget).
- [ ] Round-trip cost estimate shown in the plan (spread plus commission).
- [ ] Use `ctrader-mcp` `workflows::pre_trade_briefing` and `compare_trading_costs` where useful
      (spread and swap context in the plan).
- [ ] Margin check: needs leverage tiers, which neither server provides. Investigate whether the
      Local `calculate_margin` capability (found by the probe, undocumented) can supply a number.
- [ ] Instrument categories (forex, metals, indices, crypto) to pick defaults and sanity limits.

### 5.4 Per-symbol volume rules (blocks trusting Remote sizing)

Remote publishes no lot size, minimum or step, so `AssumedSpecs` is one global guess.

- [ ] Per-symbol overrides in `EngineConfig` (map symbol to lot size, min, step, max) and a way to
      edit them in the GUI.
- [ ] Seed table of known values per broker family, if a reliable source exists.
- [ ] Learn from rejections: when the server rejects a volume, capture the message and suggest an
      override.
- [ ] Keep the planner warning for assumed specs until values are confirmed.

### 5.5 Multi-account

The API already carries `AccountId` on every command and event.

- [ ] Several `Broker` sessions at once, each with its own supervisor, state slice and gate.
- [ ] `EngineState` becomes a map of account states plus a global part; keep single-account
      accessors for the simple case.
- [ ] Order routing: an order names its account; a broadcast-to-several-accounts command with
      per-account results.
- [ ] Aggregated P&L and exposure across accounts.
- [ ] Per-account arming (arming one must not arm another).
- [ ] Profile switching without losing the other sessions.

### 5.6 Guardrails

`src/guardrails.rs` has per-trade risk, total open risk, stale data and news. Warn, never block.

- [ ] Prop-firm rules: daily loss limit, max drawdown, max open lots, allowed trading hours,
      consistency and news restrictions. Configurable rule sets; warnings only.
- [ ] Multi-account conflict warnings (the README roadmap item): same symbol and side across
      several funded accounts.
- [ ] Spread guardrail thresholds in config (currently a fixed quarter of the stop distance).
- [ ] Max positions per symbol, max trades per hour (overtrading), loss-streak warning.
- [ ] Weekend and rollover warnings; market-closed detection from symbol sessions.
- [ ] Correlation warning (several trades on correlated pairs).
- [ ] Better open-risk estimate: currently counts only positions quoted in the account currency
      and reports the rest as unknown. Reuse the conversion rates once 5.3 lands.

### 5.7 News (engine side of `wyck-calendar`)

- [ ] Persist the last good calendar to disk so a restart does not spend a request (feed limit
      about 2 per 5 minutes per IP) and does not start empty offline.
- [ ] Start-up storm protection: several app restarts within minutes trip the limiter; the
      calendar service waits out `Retry-After`, but the UI should say why news is missing.
- [ ] Week rollover: the feed serves only the current week. On a Friday evening "nothing
      upcoming" must not read as "no news". Expose `week_ends_at` in `NewsView`.
- [ ] Watchlist and minimum impact configurable and persisted (the filter is built from the
      traded symbols today, with a fixed high-impact minimum).
- [ ] Optional: hold-off window after a release for extra caution (already a grace period).
- [ ] Optional: a second data source as fallback (the feed also exists as `.xml` and `.csv`, same
      host and same limit, so it is not a real fallback; a different provider would be).

### 5.8 Data the engine does not expose yet

- [ ] Order history and deals (Remote `get_order_history`, `get_deals`; Local equivalents), for a
      journal and for realized P&L today and this week.
- [ ] Symbol sessions (`get_symbol_sessions`, Local) to know when a market is open.
- [ ] Trendbars for a small chart or ATR (Remote and Local `get_trendbars`; the crate has window
      helpers that respect the servers' limits: `remote_history_windows`, `local_trendbar_windows`,
      `backfill_trendbars`).
- [ ] Quote stream instead of polling every second: check whether either server can push. Today
      `refresh_quotes` polls the watched symbols and open-position symbols.
- [ ] Realized P&L, daily statistics, equity curve snapshots.

### 5.9 Efficiency and robustness details found while writing the engine

- [ ] Remote `get_positions` returns positions and orders together, but the adapter calls it
      twice per refresh (`positions()` and `pending_orders()`). Add a combined `snapshot()` to the
      `Broker` trait or cache within a refresh tick.
- [ ] Local `quotes()` makes one call per symbol, sequentially. Fine for a few symbols; measure
      and parallelize if the watch list grows.
- [ ] `RemoteBroker::close` only marks intent; the MCP session closes on drop. Verify there is no
      lingering task after `Engine::shutdown` when the broker `Arc` is still referenced elsewhere.
- [ ] `RemoteBroker::account` leaves `used_margin` and `margin_level_pct` empty (Remote's balance
      response does not include them). Compute from positions if margin data becomes available.
- [ ] Instrument cache never expires. Symbol lists are stable per session, but a reconnect should
      clear the cache (currently only the session state is reset; check `instruments` in `Inner`).
- [ ] `Inner::plan_entry` reads the account every time (correct for sizing) but not in parallel
      with the quote; measure planning latency and overlap the two reads if it matters.
- [ ] Hotkey latency budget: measure `plan_entry` plus `submit` end to end on the mock (should be
      microseconds of engine time) and with real latency; write the number down. Target for the
      engine's own overhead: well under 5 ms.
- [ ] `EngineState` is cloned copy-on-write per update; with large quote maps this may allocate
      often. Profile before optimizing.
- [ ] Event broadcast capacity default 256; confirm the GUI can keep up, and handle `Lagged`
      by re-reading state (documented, needs a UI-side test).
- [ ] Activity log (`recent_events`) is in memory only; a persisted audit log of every order
      decision (plan, submit, outcome) would help post-trade review and dispute resolution.
- [ ] Config validation for cross-field sense (for example `confirm_delay * confirm_attempts`
      should fit inside `order_timeout` scale).
- [ ] Time handling: the engine uses the local clock for event stamps and a measured offset for
      news timing. If the offset check fails it falls back to the local clock silently (a debug
      log); consider a visible warning.

### 5.10 Observability

- [ ] `tracing` spans per command carrying the `CommandId`, so a log line can be tied to a
      hotkey press. Events already carry it.
- [ ] Counters worth having: refresh failures, reconnects, orders by outcome, plan-to-submit
      latency. A small metrics struct exposed on the handle is enough.
- [ ] A diagnostics bundle (recent events, state, config with secrets stripped) the user can copy
      when reporting a problem.

### 5.11 Testing to add

- [ ] Live tests (section 3.1), `#[ignore]`d and env-gated.
- [ ] Property tests for `diff_positions` and for guardrail thresholds.
- [ ] Contract tests for `LocalBroker` against a scripted Local MCP server (only the Remote
      adapter has one today; the Local adapter has unit tests for instrument building only).
- [ ] Long soak test with the mock: hours of simulated time, random failures, invariant checks
      (never two orders for one plan, never armed while not ready, state revision monotonic).
- [ ] Fuzz the feed parser and the `parse_server_time` shapes.
- [ ] Concurrency stress: many simultaneous handle calls from several threads.
- [ ] Coverage report in CI (`cargo llvm-cov`) with a floor for the engine.

### 5.12 Documentation to add

- [ ] A "trading day" guide: connect, watch, plan, arm, execute, flatten, with the safety model
      explained in user terms.
- [ ] A failure-modes table (what the engine does when X happens) kept next to the tests that
      prove it.
- [ ] An integration guide for the GUI (GPUI pattern: `cx.spawn` plus `watch_state`, handling
      `Lagged`, error mapping). Write it after the GUI choice.
- [ ] Update the workspace `README.md` "Configuration" section (still says the format is in flux).
- [ ] `CHANGELOG.md` kept per crate as behavior changes.

### 5.13 API stability

- [ ] Review every `pub` item in `wyck-engine` for whether it must be public; mark enums that will
      grow `#[non_exhaustive]` (done for most).
- [ ] Decide the versioning policy once there is a second consumer (the GUI); until then `0.x`
      and breaking changes are allowed but recorded in the changelog.
- [ ] Add `cargo semver-checks` to CI once the API settles.

### 5.14 RPC boundary (optional, later)

- [ ] Client and server crates over the existing serde types; authentication between processes;
      versioned protocol; same conformance tests as `EngineHandle`.
- [ ] Only worth doing for a headless always-on engine or several front ends at once.

---

## 6. P3: the GUI

Talks only to the engine (`EngineHandle`). Framework: see 4.1.

### 6.1 Screens and panels

- [ ] **Connection bar**: profile picker, session state, account id, account kind badge (Demo,
      Live, **Unknown shown as unknown**), latency, clock skew warning.
- [ ] **Arming control**: two steps, shows account and kind, requires typing or a deliberate
      gesture for Live. A permanent, unmistakable "ARMED" indicator when armed, and a red frame
      for a live account. Disarm always one keystroke away.
- [ ] **Account panel**: balance, equity, free margin, margin level, day P&L.
- [ ] **Positions table**: symbol, side, volume (lots and units), entry, SL, TP, P&L, R multiple,
      age; row actions: close, partial close, move stop, break-even.
- [ ] **Working orders table** with cancel and amend.
- [ ] **Order ticket**: symbol, side, risk (percent or amount) or fixed size, stop, target
      (risk/reward), shows the plan (volume, risk, cost, warnings) before anything is sent.
- [ ] **Warnings area**: news warnings, risk warnings, `UnknownOrder` notices with a dismiss
      action that calls `dismiss_warning`.
- [ ] **News panel**: filtered table with freshness indicator (`NewsFreshness`), countdown to the
      next high-impact event, forecast and previous, currency filter and watchlist editing.
- [ ] **Activity log**: recent events with command correlation (`recent_events`).
- [ ] **Settings**: engine config (limits, guardrails), per-symbol volume overrides, hotkeys,
      theme, news filter, logging level.
- [ ] **First run and account management**: add, edit, remove profiles (`wyck-config`), token
      entry with secure storage, connection test, service choice (Remote or Local).
- [ ] **Watchlist and instrument cycling**.
- [ ] **Charts**, if they are in scope: see 4.1 and section 11.

### 6.2 Hotkeys (from the README's planned table, adjust after usage)

| Key | Action |
|---|---|
| `b` | Plan and send a buy |
| `s` | Plan and send a sell |
| `c` | Close current position |
| `Up` / `Down` | Adjust position size or risk |
| `[` / `]` | Adjust stop loss / take profit |
| `Tab` | Cycle instrument |
| `q` | Quit |

- [ ] Decide the flow for `b` and `s`: one-key execution with the configured risk and stop
      (fast, needs a very visible armed state) versus a confirm step. Make it a setting.
- [ ] Key repeat: the engine's in-flight lock and minimum interval already stop double orders;
      the UI should still ignore auto-repeat for order keys.
- [ ] Global hotkeys while another window has focus (framework dependent, see 4.1).
- [ ] Rebindable keys, conflicts detection.

### 6.3 UX safety requirements (non-negotiable)

- [ ] Never show demo styling when the account kind is `Unknown`.
- [ ] Every order shows its plan first; a dry-run banner while disarmed.
- [ ] `Unknown` order outcomes are loud and persistent until reconciled or dismissed.
- [ ] Disable order controls while the session is not `Ready`.
- [ ] No action hides behind a mouse-only path; all critical actions have a key.

### 6.4 Floating instant-trade panel

From the README roadmap (Axiom.trade or GMGN style):

- [ ] Compact always-on-top panel over whatever chart is in use, hotkey driven, never stealing
      focus.
- [ ] Configurable preset buttons for order size (count and values), not hardcoded.
- [ ] Unit-aware sizing: lots, shares, contracts or base units depending on the instrument.

### 6.5 Testing and packaging

- [ ] Logic tests without a window (view models against `MockBroker` and paused time).
- [ ] Screenshot or interaction tests if the framework supports them.
- [ ] Installers per platform, code signing, auto-update, crash reporting policy.
- [ ] Performance: input to order latency measured and budgeted.

---

## 7. P4: other crates

### 7.1 `ctrader-mcp`

- [ ] Order round trip in the live probes (section 3.6).
- [ ] The 15 Local capabilities found by `probe_local.rs` and not called yet: `close_chart`,
      `open_chart`, `listAvailableIndicators`, `get_news`, `calculate_margin`, `get_asp_tab` and
      `set_asp_tab`, `get_current_app`, `get_layout_mode` and `set_layout_mode`,
      `get_market_watch_panel` and `set_market_watch_panel`, `get_trade_watch_tab` and
      `set_trade_watch_tab`, `switch_app`. No behavioral spec exists, so this waits for a concrete
      need. `get_news` and `calculate_margin` are the ones the engine could use (see 5.3, 5.7).
- [ ] Rate limiter (`src/rate_limit.rs`) is tuned to the documented 5 requests per second and is
      unverified under sustained load; revisit if a real throttle response is observed.
- [ ] Typed responses for the `serde_json::Value` returning methods (chart templates, workspaces,
      watchlists, price alerts, plugins), once their shapes are known.
- [ ] A Local `bootstrap` workflow inside `ctrader-mcp` (the engine has its own light version in
      the Local adapter); consider moving it down so other consumers benefit.
- [ ] Document the `test-support` feature in the crate README and keep it out of the stable API.

### 7.2 `wyck-calendar`

- [ ] Confirm whether the feed ever emits `Holiday` as an impact value (handled defensively, only
      Low, Medium and High were observed).
- [ ] Optional gzip support (the host compresses on request; the payload is about 14 KB).
- [ ] Decide whether a `304` counts against the request budget (unknown; treated as counting).
- [ ] Optional: display times in a chosen zone helper; the feed uses US Eastern with DST.
- [ ] No `actual` value exists in the feed, so surprise-versus-forecast cannot be computed. A
      different source would be needed for that (see the journal and news ideas in section 11).

### 7.3 `wyck-config`

- [ ] `schema_version` (`crates/wyck-config/src/app_config.rs`) exists but no migration path;
      write real logic the day the schema changes.
- [ ] Store engine preferences, news filter and watchlist, per-symbol volume overrides.
- [ ] Multiple tokens per profile or per account for the multi-account work (5.5).
- [ ] Keyring failure UX: what the app shows when the OS keyring is unavailable and the encrypted
      file fallback needs a passphrase (the crate deliberately does not prompt).
- [ ] Config file backup and restore.

---

## 8. Repository and infrastructure

- [ ] CI: add `cargo deny check` (licenses, advisories, duplicate versions) and `cargo audit`.
- [ ] CI: scheduled run (weekly) to catch dependency drift and new lints on latest stable.
- [ ] CI: coverage job and a floor for `wyck-engine`.
- [ ] CI: MSRV check (the workspace declares `rust-version = 1.98`; confirm it is really the
      minimum and test it).
- [ ] Dependabot or Renovate for dependency updates, with a policy for the pinned `rmcp` and
      the `native-tls` note in the root `Cargo.toml`.
- [ ] Benchmarks (criterion) for planning and state publication.
- [ ] Publish docs (GitHub Pages) from `cargo doc`.
- [ ] `CONTRIBUTING.md` and a security policy (`SECURITY.md`): how tokens are handled, how to
      report an issue.
- [ ] Release process: versioning, changelog, tags, build artifacts.
- [ ] Clean the workspace: the root `Cargo.toml` still lists `directories`, `toml`, `argon2` and
      friends for `wyck-config`; check for unused entries after each crate change.
- [ ] README: rewrite the Configuration and Usage sections once there is something to run; the
      roadmap checkboxes there should mirror this file.

---

## 9. Assumptions to confirm

Each is a place where the code guesses. Confirm live (section 3), then fix the code and pin the
real answer with a test. Ordered by risk.

| # | Assumption | Where | If wrong |
|---|---|---|---|
| A1 | Local raw volume `v` means `v` lots of `lotSize` base units | `broker/local.rs` (module docs, `instrument_from_details`) | Wrong sizes on Local. The adapter already refuses a symbol whose minimum volume rounds to zero units. |
| A2 | Remote volume rules default to lot 100,000, min 1,000, step 1,000 units | `config.rs` (`AssumedSpecs`), `broker/remote.rs` | Orders rejected or wrongly sized. Fix with per-symbol overrides (5.4). |
| A3 | Remote `pipDigits` gives the pip size: 5 and 3 digits mean pip is 10 points, other digit counts mean pip is one point | `pip_size_for_digits` in `broker/remote.rs` | Wrong pip distances for indices, metals, crypto. Affects stop planning and risk. |
| A4 | Remote relative stop distances are integer points of one pipette each | `RemoteBroker::place_market` | Wrong protective levels on the position. Verify on every symbol class. |
| A5 | `get_server_time` returns a time under one of `timestamp`, `serverTime`, `server_time`, `time`, `utc`, `now` | `parse_server_time` | Falls back to the local clock; news timing uses this computer's clock. |
| A6 | Remote positions echo `label` or `comment` | `decode_position` | Reconciliation falls back to symbol, side and volume. |
| A7 | Remote quote `timestamp` is in milliseconds | `RemoteBroker::quotes` | Only affects display and staleness checks. |
| A8 | Local P&L key is `unrealizedPnl` or `netProfit`; account type text contains demo, live or real | `broker/local.rs` | P&L blank, account kind Unknown. Harmless but misleading. |
| A9 | Local `place_market_order` takes pip distances as integers, at least 1 | `LocalBroker::place_market` | Wrong protective levels. |
| A10 | A `Rejected` broker error means the order certainly was not sent; timeouts and connection errors are ambiguous | `certainly_not_sent` in `trading.rs` | Wrongly assuming "not sent" could hide a live order; wrongly assuming "maybe sent" only adds a confirmation read. Errors on the safe side today. |
| A11 | The mock broker's behavior matches the real servers well enough for the scenario tests | `broker/mock.rs` | Tests pass while reality differs. The live tests (3.1) close this gap. |
| A12 | The calendar feed rate limit is about 2 requests per 5 minutes per IP | `wyck-calendar` docs | Service defaults are conservative (5 minute minimum gap). |

---

## 10. Risks and open questions

- **Unverified order path.** The whole point of the app has never run against a server.
  Mitigations in place: dry-run default, arming with acknowledgement, single-use plans, no
  replay, label reconciliation. Mitigation still needed: section 3.
- **Volume model differences between servers** (A1, A2). The riskiest technical unknown.
- **Chart requirements versus framework choice.** If price charts are in scope for v1, the GUI
  framework decision (4.1) is really a charting decision.
- **Unofficial calendar feed.** No SLA, single host, throttled. The app must work without it.
- **cTrader MCP servers are young.** Behavior and quirks can change between builds
  (`rest-proxy 1.0.18` is the reference in the skill notes). Check the build id at connect and
  warn on a version the workarounds were not written for (the Remote bootstrap already reads
  it; the engine does not compare it yet).
- **f64 for prices and money.** Matches `ctrader-mcp` and is acceptable while volumes are exact
  integers and comparisons avoid float equality. Revisit if a rounding discrepancy is observed.
- **Single-account assumption in state shape** (5.5). API is ready; internals are not.
- **Open question: one-key execution.** Speed versus safety for the hotkey flow (6.2).
- **Open question: journal and backtesting scope** for v1 (section 11).
- **Open question: license and distribution.** The repository is Apache-2.0 and `publish = false`;
  decide if any crate is meant to be published.

---

## 11. v1 vision

Mirrors the README roadmap. Not next, listed with the crates each touches.

- [ ] Engine and UI split with an RPC boundary (5.14).
- [ ] Multi-account (token) management and multi-order management across accounts (5.5).
- [ ] Backtesting engine: strategy or ruleset against historical data (needs trendbars, 5.8, and a
      new crate).
- [ ] Trade journal: entries, exits, notes, running statistics (needs order history and deals,
      5.8, plus storage, likely SQLite, and a UI).
- [ ] Economic calendar with real filters: engine and `wyck-calendar` side done; the panel and
      watchlist editing are 6.1.
- [ ] Floating instant-trade panel (6.4).
- [ ] Prop-firm guardrails as non-blocking warnings (5.6).
- [ ] Multiple customizable charts, custom indicators, saved templates and layouts.
- [ ] Futures and order-flow support; other brokers and platforms (a new `Broker` adapter plus a
      new service tag in `wyck-config`).
- [ ] TradingView alert webhook ingestion for semi-systematic setups (README roadmap).

---

## 12. Reference

### 12.1 Where things are

| Need | Look at |
|---|---|
| Crate roles and data flows | `docs/ARCHITECTURE.md` |
| Repository writing and git rules | `AGENTS.md` |
| Engine overview and safety model | `crates/wyck-engine/src/lib.rs` (rendered: `cargo doc -p wyck-engine --open`) |
| Session, refresh, reconnect | `crates/wyck-engine/src/core.rs` |
| Order pipeline, arming, flatten | `crates/wyck-engine/src/trading.rs` |
| Sizing arithmetic | `crates/wyck-engine/src/risk.rs` |
| Warnings and thresholds | `crates/wyck-engine/src/guardrails.rs`, `config.rs` |
| Calendar hosting | `crates/wyck-engine/src/news.rs` |
| Broker port and adapters | `crates/wyck-engine/src/broker/` |
| Scenario tests | `crates/wyck-engine/tests/engine_scenarios.rs` |
| Scripted Remote server for tests | `crates/wyck-engine/tests/support/mod.rs` |
| Mock MCP server (library) | `crates/ctrader-mcp/src/test_support.rs` (feature `test-support`) |
| Old TUI engine, for reference | `git show 883f7ab:crates/wyck/src/engine.rs` |
| cTrader server behavior notes | the `ctrader-mcp-servers` skill references (local and remote server docs, known quirks, self-healing playbook, trader workflows) |

### 12.2 Decisions already made (and why)

- **Dry-run by default, explicit arming.** A front end bug or a dropped connection must not be
  able to send an order.
- **Warn, never block** for guardrails. The person decides; only structural problems refuse.
- **Never replay a mutating call.** A lost reply is settled by reading state back.
- **The engine owns a Tokio runtime**, and its handle can be awaited from any executor. This is
  what keeps the GUI choice open (GPUI is not Tokio based).
- **Engine and profile management are separate.** The GUI edits profiles with `wyck-config`
  directly and hands the engine a `ConnectRequest`.
- **One `Broker` trait behind `dyn`.** Remote, Local and the mock share one behavior contract.
- **Volumes are exact integers; prices and money are `f64`** display values.
- **The ratatui TUI was dropped** in favor of a native GUI.
- **Calendar defaults follow the feed's real limit** (about 2 requests per 5 minutes), verified
  live; the service polls every 30 minutes and never twice within 5.
- **Push straight to `main`**, no branches, no attribution lines, plain ASCII text.

### 12.3 Calendar feed facts (`nfs.faireconomy.media`)

- URL: `https://nfs.faireconomy.media/ff_calendar_thisweek.json`, no authentication.
- Behind Cloudflare: `Cache-Control: public, max-age=60`, weak `ETag`, `Last-Modified`;
  conditional requests return `304`.
- Rate limit about 2 requests per 5 minutes per IP: then `429` with `Retry-After: 300` and an
  HTML body.
- Only the current week exists; `nextweek` and `lastweek` are `404`. `.xml` and `.csv` variants
  exist with the same limit.
- Records have six fields (`title`, `country`, `date`, `impact`, `forecast`, `previous`); no
  `actual`. `country` is a currency code or `All`. `date` carries a UTC offset (US Eastern).

### 12.4 Useful commands

```sh
cargo test --all-features                                   # everything
cargo test -p wyck-engine --test engine_scenarios           # engine scenarios
cargo run -p wyck-engine --example dry_run_order --features testing
WYCK_SERVICE=remote WYCK_TOKEN=... cargo run -p wyck-engine --example headless   # read only
cargo doc -p wyck-engine --open --all-features
gh run list --branch main --limit 3                         # CI status
```
