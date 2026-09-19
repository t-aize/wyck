# wyck — TODO

Working plan for the project as of 2026-09-19, ordered by what actually blocks the next
thing. Each item names the files involved and the "why," not just the "what," so this
document is usable cold in a fresh conversation.

## How to read this

- **P0** — blocks real trading. Do these before anything else touches the order path.
- **P1** — the feature the user is about to build next (economic calendar).
- **P2** — the actual point of the app (hotkey trading), currently unbuilt.
- **P3 / P4** — important but not blocking, can happen in parallel with P2.
- **Backlog** — real, tracked, not urgent.
- **v1 vision** — mirrors `README.md`'s own roadmap; included here for completeness, not
  because it's next.

Checkboxes are for tracking progress in this file directly — tick them as work lands.

---

## P0 — Before any hotkey touches a real order

- [ ] **Live-test order placement on a demo account**, both server families, before
      wiring any UI to them:
      - Remote: `RemoteClient::create_order` (`crates/ctrader-mcp/src/remote/client.rs`)
      - Local: `LocalClient::place_market_order` (`crates/ctrader-mcp/src/local/client.rs`)
      Everything verified so far (both live probes, 111 automated tests) only exercised
      read-only calls. The mutating path — the one thing this whole app exists to do —
      has zero live verification. Use `examples/probe_remote.rs` /
      `examples/probe_local.rs` as a starting point, or a short throwaway example that
      places a tiny market order on the demo account from the original token
      (`"environment":"demo"` in the JWT payload the user pasted earlier in this
      project's history) and immediately closes it.
- [ ] **Get real CI running on this branch.** `.github/workflows/ci.yml` only triggers
      on `push: branches: [main]` and `pull_request` — every commit on
      `rust-ratatui-rewrite` so far has been verified locally on Windows only (confirmed
      via `gh run list`: last actual CI run predates this branch's work). Either open a
      PR from `rust-ratatui-rewrite` into `main` (triggers CI without merging) or merge
      to `main` directly. This also matters because CI's matrix includes
      `ubuntu-latest` — nothing here has been checked on Linux yet.

---

## P1 — Economic calendar / news (next feature, per user)

Source: `https://nfs.faireconomy.media/ff_calendar_thisweek.json` (ForexFactory's public
feed, no auth). Matches the `README.md` v1 roadmap item "Economic calendar with real
filters (impact, currency, custom watchlist)" — this is that feature, pulled forward.

Sample record shape (from the feed, confirmed live 2026-09-19):

```json
{
  "title": "CPI m/m",
  "country": "CAD",
  "date": "2026-09-14T08:30:00-04:00",
  "impact": "High",
  "forecast": "-0.1%",
  "previous": "0.5%"
}
```

### Design

- [ ] **Decide where this lives.** Recommendation: start as a `wyck::news` module
      (`crates/wyck/src/news.rs`), not a new crate — it's UI-facing, has no reuse target
      yet (unlike `ctrader-mcp`/`wyck-config`, which are shared infrastructure by
      design), and splitting it out later is a mechanical move if it ever needs reuse
      (e.g. a future headless engine, per the v1 architecture split). Don't
      over-engineer a crate boundary before there's a second consumer.
- [ ] **HTTP client.** `wyck` doesn't depend on `reqwest` directly today (only
      `ctrader-mcp` does, transitively through `rmcp`). Add it as a direct dependency in
      `crates/wyck/Cargo.toml` rather than reaching into `ctrader-mcp`'s internals —
      this is a plain unauthenticated JSON fetch with nothing cTrader-specific about it.
- [ ] **Model the DTO.** `country` is sometimes `"All"` (non-currency events like "BRICS
      Summit"); `forecast`/`previous` are free-form strings, frequently empty (`""`),
      and NOT always a clean number — e.g. `"3.65|1.3"` (bond auction yield|bid-to-cover)
      appears in the sample data. Model them as `Option<String>` (empty string ->
      `None`) rather than trying to parse every shape into `f64` up front; parse
      opportunistically at display time where a plain percentage/number is expected.
- [ ] **Timestamps carry an explicit UTC offset** (`-04:00` in the sample — US
      Eastern, DST-aware from the source). Parse with the `time` crate (already a
      workspace dependency, see `crates/ctrader-mcp/src/time.rs` for the existing
      RFC 3339 parsing pattern) into an `OffsetDateTime`, then convert to whatever this
      feature displays against (local system time, or the broker's server time via
      `get_server_time` — cTrader's own docs recommend the latter for any time-window
      computation; same reasoning applies here for "is this event happening soon").
- [ ] **Caching / refresh interval.** Don't fetch on every render tick. Mirror
      `crates/wyck/src/engine.rs`'s existing `REFRESH_INTERVAL` pattern (currently 10s
      for account refresh) with its own, much longer interval — this feed changes at
      most a few times a day; every 15–30 minutes is already generous. Cache the last
      good response; a failed refresh should log and keep showing stale data, not clear
      the view (same non-fatal-refresh philosophy as `EngineEvent::RefreshFailed`).
- [ ] **Filtering**, per the README roadmap wording ("real filtering — by impact,
      currency, custom watchlist"):
      - by `impact` (`Low` / `Medium` / `High` — confirm there's no `Holiday`/other
        value by checking a few days of live feed output before hardcoding an enum)
      - by `country` (currency code, matching the symbols the user actually trades —
        natural hook: cross-reference against the active session's cached symbol list,
        e.g. `RemoteSessionContext.symbols` from `crates/ctrader-mcp/src/workflows/
        bootstrap.rs`, to default the filter to "currencies I actually trade")
      - a user-defined watchlist on top of the above two
- [ ] **Trading-safety hook (optional but high-value):** a non-blocking warning when a
      high-impact event for a relevant currency is imminent — same "warn, don't block"
      philosophy the README already commits to for prop-firm guardrails. This is the
      natural place this calendar earns its keep beyond just being a read-only panel.
- [ ] **Error handling:** a fetch failure (network down, feed schema drift, non-200)
      must never crash the TUI — same treatment as every other engine-side failure in
      this codebase (log via `tracing`, surface a UI hint, keep going).
- [ ] **Tests:**
      - JSON parsing unit tests covering: empty `forecast`/`previous`, `"All"` country,
        the `"3.65|1.3"`-style compound value, every known `impact` value.
      - An HTTP-level integration test using the same in-process mock-server pattern
        `ctrader-mcp` just adopted (`crates/ctrader-mcp/tests/support/mod.rs` — an
        `axum` router on `127.0.0.1:0`, see that file for the exact recipe) instead of
        hitting the real feed from tests.
- [ ] **UI:** a panel/screen in `crates/wyck/src/ui/` (new file, e.g. `news.rs`) —
      defer exact layout to when this is actually built; keep it consistent with
      `dashboard.rs`'s existing `ratatui` table style (see `Row::new([...])` usage
      there for the account snapshot table).

---

## P2 — The actual point of the app (currently unbuilt)

`crates/wyck/src/engine.rs` only implements `EngineCommand::Connect` and
`RefreshAccount` today. `crates/wyck/src/app.rs` only wires `q`/Esc (quit) and `r`
(refresh) as hotkeys (`is_quit_key`/`is_refresh_key`). None of the following exists yet,
despite being the entire premise in `README.md`'s "Why wyck":

- [ ] Position list rendering in `DashboardScreen` (`crates/wyck/src/ui/dashboard.rs`
      currently only renders Balance/Equity, no open-positions table).
- [ ] Risk-based lot size calculation wired into the UI — the math already exists and
      is fully tested (`crates/ctrader-mcp/src/math/sizing.rs`,
      `crates/ctrader-mcp/src/workflows/sizing.rs`), it just isn't called from anywhere
      in `wyck` yet.
- [ ] Hotkey market order execution (`b` buy / `s` sell per the README's planned
      keybindings table) — needs a new `EngineCommand::PlaceOrder` variant, a new
      `EngineEvent` for the result, and UI feedback (pending/filled/rejected).
- [ ] Stop loss / take profit adjustment hotkeys (`[` / `]` per the same table).
- [ ] Position size adjustment hotkeys (`↑` / `↓`).
- [ ] Close-position hotkey (`c`).
- [ ] Instrument cycling (`Tab`).

Do this only after P0's live order-placement test — no point building UI around a call
path that's never actually been fired against a real account.

---

## P3 — Wire Local support into `wyck` itself

`ctrader-mcp`'s Local path is now solid (fixed, tested, live-verified — see
`crates/ctrader-mcp/examples/probe_local.rs`'s last run: 63/64 known tools confirmed).
But `wyck`'s own engine never uses it:

- [ ] `crates/wyck/src/engine.rs::connect_and_bootstrap` is hardcoded to `RemoteClient`
      + `bootstrap_remote`, and completely ignores `ProfileConfig.service`
      (`crates/wyck-config/src/profile.rs` already anticipates `"ctrader-local"` as a
      valid value in its own doc comment — the config layer was built for this, the
      engine layer never followed through).
- [ ] Needs a `bootstrap_local` equivalent (Local's `get_balance` matches Remote's
      shape closely enough, but `get_assets`/`get_symbols` are Remote-specific
      `symbolId`-keyed concepts with no Local equivalent — a Local bootstrap needs its
      own, smaller `SessionContext`-like shape, not a reuse of `RemoteSessionContext`).
- [ ] Dispatch in `Engine::handle_command`'s `Connect` branch on `service` (or, more
      robustly, try Remote-shaped bootstrap first and fall back to Local-shaped on a
      schema mismatch — worth a short design pass rather than guessing here).

---

## P4 — `ctrader-mcp` polish (backlog, not blocking)

The crate is in good shape now (auth bug fixed, `get_balance`/`ping` bugs fixed, rate
limiting + retry with correct mutating-call safety semantics, 111 tests, both live
probes green). What's left is genuinely optional:

- [ ] The 15 Local capabilities `probe_local.rs` found but this crate doesn't call yet
      (`close_chart`, `open_chart`, `listAvailableIndicators`, `get_news`,
      `calculate_margin`, `get_asp_tab`/`set_asp_tab`, `get_current_app`,
      `get_layout_mode`/`set_layout_mode`, `get_market_watch_panel`/
      `set_market_watch_panel`, `get_trade_watch_tab`/`set_trade_watch_tab`,
      `switch_app`). Explicitly deferred per the user: nothing in the
      `ctrader-mcp-servers` skill documents them, so there's no behavioral spec to
      implement against yet — revisit if/when that changes, or if a concrete feature
      needs one of them (`calculate_margin` in particular looks relevant to the P2 lot
      sizing work above; worth a second look if the client-side math in
      `crates/ctrader-mcp/src/math/margin.rs` ever needs cross-checking against the
      server's own number).
- [ ] Rate limiter (`crates/ctrader-mcp/src/rate_limit.rs`) is tuned to the
      documented 5 req/s from the skill, unverified against real sustained load —
      revisit if a live 429/throttle response is ever actually observed.

---

## Backlog (real, tracked, not urgent)

- [ ] `wyck-config`'s `schema_version` field (`crates/wyck-config/src/app_config.rs`)
      exists for future migrations but no migration path is implemented yet — fine
      while the schema hasn't actually changed since `v1`, needs real logic the day it
      does.
- [ ] `README.md`'s "Configuration" section still says "Exact config file format and
      CLI flags are still in flux" — stale now that the first-run flow
      (`crates/wyck/src/ui/first_run.rs`) and `wyck-config` are both implemented and
      tested. Worth a documentation pass once P2 lands and the on-disk shape is
      actually stable.

---

## v1 vision (mirrors `README.md`'s own roadmap — not next, included for completeness)

- [ ] Engine / UI split (headless engine + RPC, TUI as first client)
- [ ] Multi-account (token) management
- [ ] Multi-order management across accounts
- [ ] Backtesting engine
- [ ] Trade journal (entries, exits, notes, running stats)
- [ ] Floating instant-trade panel for a future GUI (Axiom.trade/GMGN-style)
- [ ] Prop-firm guardrails (non-blocking warnings, not enforcement)
