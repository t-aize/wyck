# wyck: TODO

Working plan as of 2026-09-19, ordered by what blocks the next thing. Each item names the
files involved and the reason, so this document is usable cold in a fresh conversation.
The crate layout and data flows are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Where things stand

- `ctrader-mcp`: complete for the documented tools, read paths verified live on both
  servers, 111 tests. Mutating calls (orders) not verified live yet.
- `wyck-config`: complete and tested (profiles, keyring and encrypted-file secret stores).
- `wyck-calendar`: complete and tested (feed client, filters, refresh service, warnings).
  Not wired into anything yet.
- The ratatui TUI (`crates/wyck`) was removed. It is in git history (commit before
  `9f5a8ba`) if its engine loop is useful as a reference.
- CI runs on every push to `main` (fmt, clippy `-D warnings`, tests, release build) on
  ubuntu and windows, and is green. It also builds the docs with `-D warnings`.

## How to read this

- **P0**: blocks real trading. Do first.
- **P1**: decisions and foundations everything else depends on.
- **P2 / P3**: the point of the app (engine, then GUI).
- **P4**: polish, not blocking.
- **Backlog** and **v1 vision**: tracked, not next.

Tick checkboxes as work lands.

---

## P0: Before any hotkey touches a real order

- [ ] **Live-test order placement on a demo account**, both server families, before any
      UI is wired to them:
      - Remote: `RemoteClient::create_order` (`crates/ctrader-mcp/src/remote/client.rs`)
      - Local: `LocalClient::place_market_order` (`crates/ctrader-mcp/src/local/client.rs`)
      Everything verified so far only exercised read-only calls. The mutating path, the
      one thing this app exists to do, has no live verification. Extend
      `crates/ctrader-mcp/examples/probe_remote.rs` and `probe_local.rs`, or add a short
      example that places the smallest market order on a demo account and closes it.

---

## P1: Foundations

- [ ] **Choose the GUI framework.** GPUI is the leading candidate. Evaluate before
      committing: maturity and API stability, how it is consumed (Zed repository,
      published crate, or fork), Windows support, text and table rendering, and the need
      for custom price charts (the biggest risk if drawing must be done by hand).
      Alternatives to compare: Slint, Iced, egui, Tauri with a web UI.
- [ ] **Decide the engine boundary.** Does the engine run in the GUI's process (channels)
      first with an RPC boundary later, or is a headless process required from the start?
      The command and event types should be the same either way.
- [ ] **Create the `wyck-engine` crate** (see P2) once the two decisions above are made.

---

## P2: `wyck-engine` (headless core, no UI code)

Everything the removed TUI engine did, plus the pieces it never had.

- [ ] Connection lifecycle for the active profile, using `wyck-config` for the profile
      and token and `ctrader-mcp` for the session. Dispatch on `ProfileConfig.service`
      (`crates/wyck-config/src/profile.rs`): `"ctrader-remote"` or `"ctrader-local"`.
- [ ] Remote bootstrap: `workflows::bootstrap::bootstrap_remote`.
- [ ] Local bootstrap: a `bootstrap_local` with its own smaller session context. Local has
      no `symbolId` keyed `get_assets` / `get_symbols`, so `RemoteSessionContext` cannot
      be reused. Worth a short design pass.
- [ ] Periodic account, position and P&L refresh, non-fatal on failure (keep last state,
      surface the error).
- [ ] Risk-based order flow: size with `ctrader-mcp` `math::sizing` and
      `workflows::sizing`, place, re-read positions to confirm, report the result.
      Requires P0.
- [ ] Modify stop loss and take profit, change size, close position, cycle instrument.
- [ ] Host `wyck-calendar` (see below).
- [ ] Guardrails that warn and never block: imminent high-impact news first, prop-firm
      rules later.

### Calendar integration (uses the finished `wyck-calendar` crate)

- [ ] Start `CalendarService::spawn_default()` once in the engine and forward
      `CalendarState` changes as events.
- [ ] Build the filter from the session: `EventFilter::currencies` from
      `currencies_from_symbols(...)` over the symbol list, plus the user's watchlist and
      minimum impact from config.
- [ ] Call `imminent_events` on a timer with `now` corrected by the broker's
      `get_server_time`, and emit warnings.
- [ ] Persist the last good calendar to disk, so an app restart does not spend a request
      from the feed's budget (about 2 per 5 minutes per IP) or start empty offline.
- [ ] Handle the week rollover in the UI: the feed only serves the current week, so on
      Friday evening "nothing upcoming" must not read as "no news".

---

## P3: GUI (`wyck-app`)

Talks only to the engine.

- [ ] Account and position table.
- [ ] Hotkeys: `b` buy, `s` sell, `c` close, `Up` / `Down` size, `[` / `]` stop loss and
      take profit, `Tab` instrument (as in the README's planned keybindings).
- [ ] Order feedback: pending, filled, rejected.
- [ ] First-run and account management screens over `wyck-config`.
- [ ] News panel: filtered table, freshness indicator (`Freshness`), warning banner.
- [ ] Floating instant-trade panel with configurable size presets and units.

---

## P4: `ctrader-mcp` polish

- [ ] The 15 Local capabilities found by `probe_local.rs` that the crate does not call yet
      (`close_chart`, `open_chart`, `listAvailableIndicators`, `get_news`,
      `calculate_margin`, `get_asp_tab` / `set_asp_tab`, `get_current_app`,
      `get_layout_mode` / `set_layout_mode`, `get_market_watch_panel` /
      `set_market_watch_panel`, `get_trade_watch_tab` / `set_trade_watch_tab`,
      `switch_app`). No behavioral spec exists for them, so this waits for a concrete need.
      `calculate_margin` could cross-check `crates/ctrader-mcp/src/math/margin.rs`.
- [ ] The rate limiter (`crates/ctrader-mcp/src/rate_limit.rs`) is tuned to the documented
      5 req/s and unverified under sustained load. Revisit if a live 429 is observed.

---

## Backlog

- [ ] `wyck-config`: `schema_version` (`crates/wyck-config/src/app_config.rs`) exists for
      migrations, but no migration path is implemented yet. Needs real logic the day the
      schema changes.
- [ ] `README.md`: the Configuration section still says the file format is in flux. Update
      it once the engine and first-run flow exist.

---

## v1 vision (mirrors the README roadmap)

- [ ] Engine / UI split with an RPC boundary
- [ ] Multi-account (token) management
- [ ] Multi-order management across accounts
- [ ] Backtesting engine
- [ ] Trade journal (entries, exits, notes, running stats)
- [ ] Floating instant-trade panel (Axiom.trade / GMGN style)
- [ ] Prop-firm guardrails (non-blocking warnings, not enforcement)
