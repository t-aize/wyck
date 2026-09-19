# wyck: TODO

Working plan as of 2026-09-19, ordered by what blocks the next thing. Each item names the
files involved and the reason, so this document is usable cold in a fresh conversation.
The crate layout and data flows are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Where things stand

- `ctrader-mcp`: complete for the documented tools, read paths verified live on both
  servers, 111 tests. Mutating calls (orders) not verified live yet.
- `wyck-config`: complete and tested (profiles, keyring and encrypted-file secret stores).
- `wyck-calendar`: complete and tested (feed client, filters, refresh service, warnings).
  Hosted by the engine.
- `wyck-engine`: first version done and tested with mocks (session, state, planning, order
  pipeline, guardrails, news). Not yet validated against a live server.
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
- [x] **Create the `wyck-engine` crate** (first version done, see P2). The engine runs on its own
      runtime and works from any executor, so the GUI choice does not affect it.

---

## P2: `wyck-engine` (headless core, no UI code)

First version done (`crates/wyck-engine`, see its docs and `docs/ARCHITECTURE.md`): session
lifecycle, state and events, risk planning, dry-run-first order pipeline, guardrails, news
hosting, Remote and Local adapters, mock broker, scenario tests.

Remaining:

- [ ] **Live validation on a demo account** (the engine part of P0): connect Remote and Local,
      place and close a minimal order, run a flatten, and confirm the assumptions listed
      below. Until this is done, do not arm the engine on a funded account.
- [ ] Confirm the **Local volume unit** (`crates/wyck-engine/src/broker/local.rs` module
      docs): the adapter reads raw volume as lots of `lotSize` base units and fails closed
      when that rounds to zero.
- [ ] Confirm the **Remote volume rules**: the server publishes no lot size, minimum or step,
      so the engine assumes them (`EngineConfig::assumed_specs`). Add per-symbol overrides
      once real values are known.
- [ ] `get_server_time` response shape on both servers (`broker::parse_server_time` accepts
      several shapes; confirm which one is real).
- [ ] Place pending orders (limit, stop) through the engine. Only market orders exist today.
- [ ] Margin awareness: needs leverage tier data neither server provides.
- [ ] Several accounts at once (the API already carries an `AccountId` everywhere).
- [ ] Prop-firm guardrails (warnings for multi-account rule conflicts).
- [ ] Persist the last good calendar to disk, so a restart does not spend a request from the
      feed's budget (about 2 per 5 minutes per IP) or start empty offline.
- [ ] Optional: an RPC boundary in front of `EngineHandle` for a separate engine process.

Calendar integration (done in the engine): `CalendarService` hosting, filter from the symbols
being traded, warnings timed with the broker's clock. Still for the UI: handle the week
rollover so that "nothing upcoming" on a Friday evening does not read as "no news".

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
