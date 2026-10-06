# Tasks

Each task fits one fresh agent session. Rules for every task:

- Start from `PROGRESS.md`, read only the files the task lists, stop when the check passes.
- One structural change per commit. Move first, clean second, rewrite last. Never mix a move with a behavior change.
- Commit style: `type(scope): summary` and a `Refs: T-xxx` line. No `Co-Authored-By`, no session line, no "Generated with" footer (see `AGENTS.md`).
- Default check: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, then the ASCII one-liner from `AGENTS.md`. Once `xtask` exists (T-010), use `cargo xtask check`.
- Default fallback: `git revert <commit>`.
- Sizes: S under 1 hour, M up to half a day. No L task: split it.
- Known test caveats from the project memory: `tmp_fvg_script` (reads `%APPDATA%`) and `a_chart_computes_a_script_it_holds_through_the_config` (flaky under load, rerun alone). Do not run `rustfmt` on `crates/wyck/src/chart/mod.rs` (it follows `mod` lines into other files); format single files.
- Live tests stay `#[ignore]` and need `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`. No task may place an order on a real account.

Paths are relative to the repo root. Line numbers come from the audit at `c805e65` and drift; search by symbol.

## M0 Safety net

- [x] T-001 | M0 | Tag and refactor branch | S
  Goal: freeze a rollback point. Needs the maintainer's go-ahead (git action).
  Steps: 1) `git tag pre-refactor` on the current `main` commit. 2) `git switch -c refactor/workspace-v2`. 3) Do not push unless asked.
  Verify: `git tag --list pre-refactor` and `git branch --show-current`.
  Depends on: none. Risk: low. Fallback: delete the tag and branch.

- [x] T-002 | M0 | Commit the refactor documents | S
  Goal: this folder is in git.
  Files: `docs/refactor/**`.
  Steps: 1) Run the ASCII one-liner on `docs/refactor`. 2) Commit as `docs(refactor): add restructuring plan and audit`.
  Verify: one-liner prints nothing.
  Depends on: T-001. Risk: low.

- [x] T-003 | M0 | Install audit tools | S
  Goal: tools named in the plan exist and their versions are recorded.
  Steps: 1) `cargo install cargo-machete cargo-dupes tokei` (check crate names with `cargo search` first). 2) `rustup toolchain install nightly --profile minimal`, `cargo install cargo-udeps`. 3) `npm i -g jscpd`. 4) Record versions in `audit/00-baseline.md`.
  Verify: each tool prints its version.
  Depends on: none. Risk: low.

- [x] T-004 | M0 | Complete the baseline | M
  Goal: fill the "not measured" cells of `audit/00-baseline.md`.
  Steps: 1) `cargo test --workspace` and note failures. 2) Skip the release build (the maintainer asked not to run it; CI covers it). 3) `cargo machete --with-metadata`, `cargo +nightly udeps --workspace --all-features`. 4) `cargo dupes report`, `jscpd --min-lines 8 crates/`. 5) `tokei crates/`. 6) `rg -n "mod (utils|common|helpers)" crates`. 7) Set the duplication threshold.
  Verify: the baseline file has no "not measured" left.
  Depends on: T-003. Risk: low.

- [x] T-005 | M0 | Snapshot all study outputs | M
  Goal: freeze the numeric output of the 50 `StudyKind` before anything moves.
  Files: new `crates/wyck-chart/tests/study_snapshots.rs`, fixture bars in `crates/wyck-chart/tests/fixtures/`.
  Steps: 1) Build a deterministic bar set (about 500 bars, include a gap and a large bar). 2) For each `StudyKind::ALL`, run `study::compute` with default config and serialize `StudyOutput` rounded to 9 decimals. 3) Store as text snapshots, compare in the test. 4) Add an update switch through an env var.
  Verify: `cargo test -p wyck-chart study_snapshots`.
  Depends on: none. Risk: low.

- [x] T-006 | M0 | Characterize sizing and PnL math | S
  Goal: pin `Contract`, `lots_for_risk`, `live_net` at the edges.
  Files: `crates/wyck-openapi/src/trading/contract.rs` tests.
  Steps: add cases for lot step, min and max volume, `digits` 0, 3 and 5, JPY style pip, rounding never going up past requested risk.
  Verify: `cargo test -p wyck-openapi contract`.
  Depends on: none. Risk: low.

- [x] T-007 | M0 | Capture list and before screenshots | S
  Goal: a checklist of screens to capture before and after M6.
  Files: `docs/refactor/audit/20-ui.md` (list), screenshots stored outside git or under `docs/refactor/shots/` (maintainer decides).
  Steps: list every screen in `audit/20-ui.md`, capture each at default scale and at 125 percent, name `before-<screen>.png`.
  Verify: every listed screen has a file.
  Depends on: none. Risk: low.

- [x] T-008 | M0 | Check GPUI test support | S
  Goal: know whether `#[gpui::test]` works with `gpui-pre 0.3.5`.
  Steps: 1) Find the test feature in the vendored gpui sources under `~/.cargo/registry/src/*/gpui-pre-0.3.5`. 2) Try a throwaway test in a scratch crate or an ignored test. 3) Record the result in `audit/40-ai-friction.md`.
  Verify: written conclusion with the exact feature name or "unsupported".
  Depends on: none. Risk: low.

- [x] T-009 | M0 | Re-measure flaky tests | S
  Goal: confirm the baseline of known flaky tests.
  Steps: run `cargo test --workspace` 3 times; run the two known tests alone; record results.
  Verify: table in `audit/00-baseline.md`.
  Depends on: T-004. Risk: low.

## M1 Workspace hygiene

- [x] T-010 | M1 | Create `xtask` with `check` | M
  Goal: one command for fmt, clippy `-D warnings` and tests.
  Files: new `xtask/` crate, `Cargo.toml` (`members = ["crates/*", "xtask"]`), `.cargo/config.toml` (`[alias] xtask = "run --package xtask --"`), `.github/workflows/ci.yml`.
  Steps: 0) First fix the clippy error that makes `-D warnings` fail today: `crates/wyck-chart/src/drawing/extras.rs:594` `let golden: f32 = 1.618_034;` -> `std::f32::consts::GOLDEN_RATIO` if stable under `rust-version = 1.98`, otherwise a justified `#[allow(clippy::approx_constant)]` with the reason; one commit, `fix(chart): ...`. 1) Add a small binary without extra deps (use `std::process::Command`). 2) `check` runs the three commands in order and stops at the first failure. 3) CI jobs call `cargo xtask check` where they ran the raw commands, keeping the 3 OS matrix and the `--no-default-features` openapi clippy line.
  Verify: `cargo xtask check` passes locally; CI green on a draft PR.
  Depends on: T-004. Risk: low.

- [x] T-011 | M1 | Extend workspace lints | S
  Goal: lints enforced everywhere.
  Files: `Cargo.toml` `[workspace.lints]`, every crate `Cargo.toml` (`[lints] workspace = true`; `wyck` and `wyck-openapi` must be checked, `wyck-config`, `wyck-chart`, `wyck-ui` have it).
  Steps: add `clippy::unwrap_used`, `expect_used`, `dbg_macro`, `todo` as warn; allow in tests via `clippy.toml` (`allow-unwrap-in-tests = true`, `allow-expect-in-tests = true`). Fix or justify the 9 runtime cases.
  Verify: default check.
  Depends on: T-010. Risk: low.

- [x] T-012 | M1 | Choose the pedantic subset | M
  Goal: a justified list of pedantic lints.
  Steps: 1) From `audit/00-baseline.md`, accept cheap and useful ones (`needless_pass_by_value`, `redundant_closure_for_method_calls`, `semicolon_if_nothing_returned`, `manual_midpoint`, `match_same_arms`). 2) Reject noisy ones with a one-line reason in `docs/CONVENTIONS.md` later (`must_use_candidate`, cast lints, `unreadable_literal`). 3) Fix the accepted ones crate by crate in separate commits.
  Verify: default check; pedantic count for accepted lints is 0.
  Depends on: T-011. Risk: low.

- [x] T-013 | M1 | Remove unused dependencies | S
  Goal: 0 unused deps.
  Steps: act on `machete` and `udeps` output; keep false positives with a `[package.metadata.cargo-machete] ignored` entry and a comment saying why. List avoidable duplicate versions from `cargo tree -d` in `audit/00-baseline.md`.
  Verify: `cargo machete --with-metadata` prints nothing; default check.
  Depends on: T-004. Risk: low.

- [x] T-014 | M1 | Useful `AGENTS.md` v1 | M
  Goal: an agent knows the commands, the target layers and the hard rules.
  Files: `AGENTS.md`, `CLAUDE.md`.
  Steps: keep the two existing sections verbatim. Add: commands (`cargo run -p wyck`, `cargo xtask check`, `cargo test -p <crate>`), a 15 line architecture summary linking `docs/refactor/PLAN.md` for now, hard rules (domain crates have no gpui or tokio; orders only through `Account::trade`; UI uses `wyck-ui`; no unwrap outside tests; never log secrets; default account is demo), traps (gpui pinned, tokio task cancelled when its GPUI task is dropped, cTrader limits). Under 200 lines. `CLAUDE.md` stays `@AGENTS.md` plus a short Claude Code section.
  Verify: line count, ASCII one-liner.
  Depends on: T-010. Risk: low.

## M2 Cleanup

- [ ] T-020 | M2 | Remove `#[path]` modules | S
  Files: `crates/wyck/src/main.rs:3,18,22`, `crates/wyck/src/services/**`.
  Steps: move `alerts`, `token_store`, `updates` under `services/` as normal `mod` declarations; fix `crate::` paths.
  Verify: default check. Risk: low.

- [ ] T-021 | M2 | Comment noise in `wyck-openapi` | S
  Files: `src/transport/wire.rs:31-218`, `src/market/bars.rs`, `src/handle.rs`, `src/transport/messages.rs`, `src/transport/rate_limit.rs`.
  Steps: delete docs that repeat the name; keep "why" comments (`connection.rs:444`, `session/mod.rs:684`, `config.rs:63`); remove the "MCP servers" comment in `bars.rs:15`.
  Verify: default check; doc-comment count drops.
  Risk: low.

- [ ] T-022 | M2 | Comment noise in `wyck-config` | S
  Files: `crates/wyck-config/src/**`.
  Steps: same policy; keep security and crash-safety rationale.
  Verify: default check. Risk: low.

- [ ] T-023 | M2 | Comment noise in `wyck-chart` and `wyck-ui`; document central types | S
  Files: `crates/wyck-chart/src/{data,study/mod,timeframe}.rs`, `crates/wyck-ui/src/{tokens,theme}.rs`.
  Steps: remove paraphrasing docs (`data.rs:51,56`, `study/mod.rs:1079,1217`, `tokens.rs:28`); add one sentence docs on `Timeframe`, `Series`, `InputSpec`, `PlotSpec`.
  Verify: default check. Risk: low.

- [ ] T-024 | M2 | Comment noise in the app | S
  Files: `crates/wyck/src/trading/account.rs` and other files with `// ----` separators.
  Verify: default check. Risk: low.

- [ ] T-025 | M2 | Shrink the `wyck-openapi` public surface | S
  Files: `crates/wyck-openapi/src/lib.rs:166-169`, `Cargo.toml`, `src/market/price.rs`, `src/trading/contract.rs`, `tests/**`.
  Steps: hide `transport` behind a `test-support` feature enabled for tests; delete `UNITS_PER_PRICE` and the second `Contract::format_price`; make unused-elsewhere items `pub(crate)` where tests do not need them.
  Verify: default check plus `cargo clippy -p wyck-openapi --no-default-features --lib -- -D warnings`.
  Risk: low.

- [ ] T-026 | M2 | One settings row module in the app | M
  Files: `crates/wyck/src/chart/{settings_rows,chart_settings_ui,export_ui,drawing_props}.rs`.
  Steps: keep `settings_rows.rs` versions of `choice`, `switch`, `number`; make the other three call them (note `unwrap_or(0)` vs `unwrap_or(usize::MAX)` in `choice`: pick one and test it). Temporary: M6 lifts these into `wyck-ui`.
  Verify: default check; manual open of the three dialogs. Risk: low.

- [ ] T-027 | M2 | Unify tiny duplicates in chart | S
  Files: `crates/wyck-chart/src/scene/{series,price,studies,mod,footprint,cmd}.rs`, `export.rs`, `format.rs`.
  Steps: one `compact()` (decide `k`/`K`, `M`, `B` once and update tests), one `dash_pattern` (decide which behavior wins and say so in the commit), one `rgb_alpha`/`with_alpha`, `price_text` calls `wyck_openapi::market::format_price` while `Contract` and `export` share it.
  Verify: default check and T-005 snapshots. Risk: low (visual change possible, report it).

- [ ] T-028 | M2 | Deduplicate study spec helpers | S
  Files: `crates/wyck-chart/src/study/{mod,extended}.rs`.
  Steps: share `choice` and the color constants; merge `StudyConfig::new` and `for_script` bodies; extract the default `PlotStyle` and the opacity clamp.
  Verify: default check and T-005 snapshots. Risk: low.

- [ ] T-029 | M2 | Remove the `panic!` in `extended.rs` | S
  Files: `crates/wyck-chart/src/study/{extended,mod}.rs`.
  Steps: `spec_for` returns `Option`; `StudyKind::ALL` derived with a small macro or a test that fails when a variant is missing.
  Verify: default check and T-005 snapshots. Risk: low.

- [ ] T-030 | M2 | Remove dead `allow`s | S
  Files: `crates/wyck/src/multichart/drawing_ui.rs` (6 x `unused_variables`), `crates/wyck/src/appearance/presets.rs:24`, `services/alerts/eval.rs:111`, `crates/wyck-chart/src/drawing/book.rs:542,699`.
  Steps: remove unused parameters, or add a one-line reason where an `allow` stays.
  Verify: default check. Risk: low.

- [ ] T-031 | M2 | Typed errors instead of `String` | M
  Files: `crates/wyck/src/chart/{raster,paint}.rs`, `services/alerts/sound.rs`, `services/updates.rs`, `settings_hub/alerts.rs`, `trading/ticket/mod.rs` (about 10 places).
  Steps: add `thiserror` to `wyck` if needed (check it is already in `[workspace.dependencies]`); one small enum per module; keep user-visible text identical.
  Verify: default check. Risk: low.

- [ ] T-032 | M2 | Split `backup.rs` | M
  Files: `crates/wyck-config/src/backup.rs` into `backup/{format,store,pending}.rs`.
  Steps: move only; no behavior change; keep the public API.
  Verify: default check. Risk: low.

- [ ] T-033 | M2 | Retry transient `PermissionDenied` on Windows file replace and read | S
  Goal: no failed save or load when two threads touch the same document at once.
  Evidence: `audit/00-baseline.md` (T-009). `many_threads_saving_one_document_never_tear_it` fails about 4 % of runs under CPU load with OS error 5 in `atomic_write` (`crates/wyck-config/src/fs_util.rs:44-67`, the `fs::rename`) or in `DocumentStore::load`.
  Files: `crates/wyck-config/src/fs_util.rs`, `crates/wyck-config/src/documents.rs`.
  Steps: 1) On Windows only, retry `fs::rename` and the document read a few times (for example 5 tries, 2 ms then doubling) when the error kind is `PermissionDenied`; other errors stay immediate. 2) Keep the "old file or new file, never torn" guarantee and the cleanup of the temp file on failure. 3) Add a comment saying why. 4) Rerun the stress loop of T-009 (8 processes x 15 runs of the lifecycle test).
  Verify: the stress loop gives 0 failures out of 120; default check.
  Depends on: none. Risk: low. Fallback: revert.

## M3 Domain extraction

- [ ] T-040 | M3 | Create `wyck-core` and an arch-check skeleton | S
  Files: new `crates/wyck-core`, `Cargo.toml` `[workspace.dependencies]`, `docs/architecture/layers.toml`, `xtask/src/arch.rs`.
  Steps: crate with a one-line README; `layers.toml` maps crate to layer for all 5 current crates plus new ones; `cargo xtask arch-check` reads `cargo metadata` and fails on a dependency pointing up or sideways; start in warn mode.
  Verify: `cargo xtask arch-check` runs; default check.
  Depends on: T-010. Risk: low.

- [ ] T-041 | M3 | Move market basics to `wyck-core` | M
  Files: `crates/wyck-openapi/src/market/{price,bars,ticks,symbols,hours}.rs` to `crates/wyck-core/src/`.
  Steps: move with `git mv`; keep `pub use wyck_core::...` in `wyck-openapi` temporarily so callers compile; point `wyck-chart` at `wyck-core` directly; `chrono-tz` leaves `wyck-openapi` if no longer needed.
  Verify: default check and T-005 snapshots.
  Depends on: T-040. Risk: medium (many imports). Fallback: revert.

- [ ] T-042 | M3 | One `Symbol` type | M
  Files: `crates/wyck/src/chart/mod.rs:290`, `crates/wyck/src/multichart/mod.rs:51`, `wyck-core` symbol module.
  Steps: replace `chart::Symbol` and `multichart::SymbolRef` by the core type; adapt serde of saved layouts (keep old field names with `#[serde(alias)]`); add a round trip test of an old layout file.
  Verify: default check. Depends on: T-041. Risk: medium.

- [ ] T-043 | M3 | Price, Volume, Money newtypes | M
  Files: `wyck-core`, call sites of `as f64 / PRICE_SCALE as f64` (about 20, find with `rg "PRICE_SCALE"`).
  Steps: add `Price(i64)`, `Volume(i64)`, `Money` with the single conversion functions; migrate the 20 sites; keep wire structs unchanged in this task.
  Verify: default check and T-006 tests. Depends on: T-041. Risk: medium.

- [ ] T-044 | M3 | Create `wyck-trading` and move `Contract` | S
  Files: `crates/wyck-openapi/src/trading/contract.rs` to `crates/wyck-trading/src/contract.rs`; `crates/wyck/src/trading/math.rs`.
  Steps: move; `math.rs` re-export goes away and its callers import `wyck_trading`; decide whether `SpotTracker`, `LiveBarTracker`, `DepthBook` (`market/{quotes,live,depth}.rs`) move now or stay (record the decision in `docs/adr/`).
  Verify: default check and T-006 tests. Depends on: T-041. Risk: low.

- [ ] T-045 | M3 | Spike: move account types and `AccountBook` | M
  Files: `crates/wyck-openapi/src/account/{types,book}.rs`.
  Steps: 1) Try moving the enums and structs with their serde derives to `wyck-trading`, keeping the wire shape; `wyck-openapi` depends on `wyck-trading`. 2) Move `Notice`, `Tone`, `NoticeAction`, `explain`, `refusal`, `describe` to `crates/wyck/src/trading/notices.rs`. 3) If the wire coupling is too invasive, stop, keep the types in openapi, write the decision in an ADR, and give `arch-check` a documented exception.
  Verify: default check. Depends on: T-044. Risk: high. Fallback: revert and keep the exception.

- [ ] T-046 | M3 | Move guard, plan and sizing math | M
  Files: `crates/wyck/src/trading/{guard,plan,math}.rs` to `wyck-trading`.
  Steps: move; replace `Instant`/system time in `DuplicateGuard` and `TimeStop` with a `Clock` trait from `wyck-core`; tests use a fake clock.
  Verify: default check (24 existing tests pass). Depends on: T-044. Risk: medium.

- [ ] T-047 | M3 | Extract pure logic from `account.rs` | M
  Files: `crates/wyck/src/trading/account.rs` (`standing`, `assess`, `assess_batch`, `fingerprint`, `ReverseTracker`, `missed_exits`, `catch_up`).
  Steps: move to `wyck-trading` as free functions or small structs over plain data; the entity calls them; add tests for each (none exist today).
  Verify: default check. Depends on: T-046. Risk: medium.

- [ ] T-048 | M3 | One sizing source of truth | S
  Files: `crates/wyck-chart/src/study/atr_stop.rs`, `crates/wyck-chart/src/drawing/position.rs:210`.
  Steps: `AtrStop` to `wyck-trading`; `PositionSettings::stats` calls `lots_for_risk` and the shared formatter; keep rounding "down to the lot step" (`position.rs:219`).
  Verify: default check and T-006 tests. Depends on: T-044. Risk: medium.

- [ ] T-049 | M3 | Create `wyck-indicators` | M
  Files: `crates/wyck-chart/src/study/**` to `crates/wyck-indicators/src/`; `crates/wyck/src/indicators/**`.
  Steps: move study modules; leave `custom/library.rs` I/O and the global `REGISTRY` to a service in `crates/wyck/src/indicators/`, with the crate exposing a pure compile and a registry trait; update `wyck-chart` to depend on `wyck-indicators`; `rhai` leaves `wyck-chart`.
  Verify: default check and T-005 snapshots. Depends on: T-048. Risk: medium.

- [ ] T-050 | M3 | Pure chart code out of the app | S
  Files: `crates/wyck/src/chart/{raster,load,live}.rs`, `crates/wyck-chart/src/export/store.rs`.
  Steps: `raster`, `load`, `live` to `wyck-chart` (inject the font bytes into `raster`); `export/store.rs` (uses `wyck-config`) to the app, so `wyck-chart` no longer depends on `wyck-config`.
  Verify: default check. Depends on: T-049. Risk: medium.

- [ ] T-051 | M3 | Alerts, appearance, preferences placement | M
  Files: `crates/wyck/src/services/alerts/{model,eval}.rs`, `crates/wyck/src/appearance/{contrast,presets}.rs`, `crates/wyck/src/workspace/preferences.rs`, `trading/{panel,ticket}/prefs.rs`.
  Steps: decide with a short ADR whether alerts live in `wyck-chart` or `wyck-indicators`; move; `contrast` and `presets` to `wyck-ui::theme`; keep persistence models in the app unless a clean move to `wyck-config` documents exists.
  Verify: default check. Depends on: T-050. Risk: medium.

- [ ] T-052 | M3 | Isolate cTrader parts of `wyck-config` | S
  Files: `crates/wyck-config/src/{profile,tokens,app_config,lib}.rs`, `crates/wyck/src/services/token_store.rs`.
  Steps: move `client_id`, `callback_port`, `account_id`, `CLIENT_SECRET`, `OAUTH_TOKENS`, `OpenApiTokens` into a `ctrader` module; `last_symbol` becomes a `DocumentStore` document (migrate the old field on load); store `obtained_at` so the `TokenSet` round trip is exact.
  Verify: default check; test that an old `app.toml` still loads. Risk: medium (user data). Fallback: revert.

- [ ] T-053 | M3 | Make arch-check strict | S
  Files: `xtask/src/arch.rs`, `deny.toml`, remove temporary re-exports.
  Steps: arch-check fails instead of warns; `[bans]` with `wrappers` limiting `gpui-pre` and `gpui-kit` to `wyck-ui` and `wyck`; delete the temporary `pub use` re-exports.
  Verify: `cargo xtask arch-check`, `cargo deny check`, default check. Depends on: T-045, T-050. Risk: low.

## M4 Order path

- [ ] T-060 | M4 | Map and test the single entry point | M
  Files: `crates/wyck/src/trading/account.rs`, `crates/wyck-trading/src/gateway.rs` (new).
  Steps: add `TradingGateway` trait in `wyck-trading` (new order, amend, cancel, close, amend SL/TP); the app implements it over `TradingClient`; `Account::trade` calls only the trait; add a grep-based test or `xtask` check that only the adapter file names `TradingClient`.
  Verify: default check. Depends on: T-047. Risk: medium.

- [ ] T-061 | M4 | One `OrderIntent` and shared policy | M
  Files: `crates/wyck-trading/src/{guard,intent}.rs`, `account.rs`.
  Steps: place, close, cancel and amend all build an `OrderIntent` that passes the same policy function; `reduces` computed for real (close and reduce orders skip the new-risk limits, new orders do not); the policy returns allow, confirm or refuse; one setting decides confirmation for close, cancel and drag edits.
  Verify: default check; table tests of the policy. Depends on: T-060. Risk: medium (behavior change: say so in the commit).

- [ ] T-062 | M4 | Audit log | S
  Files: `crates/wyck/src/trading/account.rs`.
  Steps: `tracing` with `target: "audit"` on send, result and timeout; fields: environment, account id, symbol, side, volume, order type, client message id, outcome; never tokens or secrets; a test captures log output and asserts no secret strings.
  Verify: default check. Depends on: T-060. Risk: low.

- [ ] T-063 | M4 | Live is a real mode | S
  Files: `crates/wyck/src/connection/mod.rs:170`, `dashboard/mod.rs:259`, `trading/ticket/mod.rs`, `trading/panel/view.rs`.
  Steps: `Environment` is the only source; remove the `is_live` copy; in live, confirmation cannot be turned off for new orders and one-click shows a persistent warning; demo behavior unchanged.
  Verify: default check; manual check on a demo account. Depends on: T-061. Risk: medium.

- [ ] T-064 | M4 | Visible uncertain state | S
  Files: `crates/wyck/src/trading/account.rs:895-930`, panel views.
  Steps: after the 15 s timeout, show an "outcome unknown, checking account" marker until `on_ready` or the next book update clears it.
  Verify: default check. Depends on: T-060. Risk: low.

- [ ] T-065 | M4 | One quote cache | M
  Files: `crates/wyck/src/trading/account.rs:161`, `services/alerts/mod.rs:247`, `dashboard/mod.rs:143,479-487`, `chart/history.rs:352`.
  Steps: one entity holds quotes; consumers read it; `Event::Spot` is dispatched once; notify only when a tracked value changed.
  Verify: default check; manual check of chart lines, alerts, ticket. Depends on: T-047. Risk: medium.

## M5 Connection hardening

- [ ] T-070 | M5 | `Transport` trait and fake transport | M
  Files: `crates/wyck-openapi/src/transport/connection.rs:240-274`, `tests/support/mod.rs`.
  Steps: trait that yields a framed stream; default impl over `tokio_tungstenite`; fake over `tokio::io::duplex`; migrate 3 tests of `tests/session.rs` to it.
  Verify: `cargo test -p wyck-openapi`. Risk: medium.

- [ ] T-071 | M5 | Explicit `Phase` state machine | M
  Files: `crates/wyck-openapi/src/session/mod.rs:136-189,661-792`.
  Steps: add `Phase` and a pure `next(phase, input) -> phase`; table test for every transition; rewrite `supervise` to use it; delete `force_refresh` and `refreshed_for_invalid` flags without changing the observable event order (keep the existing session tests green).
  Verify: `cargo test -p wyck-openapi`. Depends on: T-070. Risk: medium.

- [ ] T-072 | M5 | Inbound silence watchdog | M
  Files: `crates/wyck-openapi/src/transport/connection.rs`, `config.rs`.
  Steps: track the last inbound frame time; no frame within 3 heartbeat periods closes the connection with a new `Error` kind that the session treats as retryable; configurable; test with paused time.
  Verify: `cargo test -p wyck-openapi`. Depends on: T-070. Risk: medium.

- [ ] T-073 | M5 | Typed requests | S
  Files: `crates/wyck-openapi/src/transport/{connection,messages,wire}.rs`, request structs.
  Steps: `trait Request { const TYPE: u32; const RESPONSE: u32; const CLASS: RateClass; type Response; }`; `Client::call` takes `R: Request`; migrate callers.
  Verify: `cargo test -p wyck-openapi`. Depends on: T-070. Risk: low.

- [ ] T-074 | M5 | Server error code enum | S
  Files: `crates/wyck-openapi/src/error.rs:145-176`.
  Steps: `ServerErrorCode` with the known codes and `Unknown(String)`; keep `ErrorKind` mapping and its pinned tests.
  Verify: `cargo test -p wyck-openapi`. Risk: low.

- [ ] T-075 | M5 | Resync after `Lagged` | S
  Files: `crates/wyck-openapi/src/session/mod.rs:848`, `crates/wyck/src/dashboard/mod.rs:468`.
  Steps: emit `SessionEvent::Resynced` on lag; the app re-reads the account; test with a tiny event capacity.
  Verify: default check. Depends on: T-071. Risk: low.

- [ ] T-076 | M5 | Demo and live connection cap | S
  Files: `crates/wyck/src/connection/mod.rs`, `dashboard/mod.rs`.
  Steps: at most one `Session` per environment; switching environment closes the other first.
  Verify: default check. Risk: low.

- [ ] T-077 | M5 | Degraded UI | S
  Files: `crates/wyck/src/trading/ticket/view.rs`, `trading/panel/view.rs`, `dashboard/header.rs`.
  Steps: while `Conn::Reconnecting` or `Failed`, disable send, close, cancel and amend buttons with a short reason; add a stale-quote marker on quotes older than a threshold.
  Verify: default check; manual disconnect test on demo. Depends on: T-064. Risk: low.

- [ ] T-078 | M5 | Zeroize secret copies | S
  Files: `crates/wyck/src/connection/{browser_handoff,authorizing,credentials}.rs`.
  Steps: keep secrets in `SecretString` until the last moment; avoid `expose_secret().to_string()` copies.
  Verify: default check. Risk: low.

## M6 UI

- [ ] T-080 | M6 | Size enum and tokens | M
  Files: `crates/wyck-ui/src/{tokens,button,controls,form,layout,menu}.rs`.
  Steps: `Size` (Xs, Sm, Md, Lg) is the single source of control heights and of the gpui-kit size passed to buttons; fix `control()` 28 vs 32; add spacing and radius tokens; scale `tokens::menu::*`; replace the 39 literal `px(..)` in `wyck-ui`.
  Verify: default check; before and after captures of 3 screens. Risk: medium (visible).

- [ ] T-081 | M6 | Missing components | M
  Files: `crates/wyck-ui/src/{select,table,tabs}.rs` (new), `field.rs`.
  Steps: build from the app copies (`trading/panel/view.rs:189,1118`, `dashboard/header.rs:365`, `panel/dialogs.rs:401`, `ticket/view/order.rs:9`, `indicators/editor/parts.rs:71`); add `switch_row`, `choice_row`, `number_row`, `card`, `tint`, `mono`, swatch row.
  Verify: default check. Depends on: T-080. Risk: low.

- [ ] T-082 | M6 | `xtask ui-check` | M
  Files: `xtask/src/ui.rs`, `docs/architecture/ui-exceptions.toml`.
  Steps: scan `crates/wyck/src/**` for `gpui_kit::` (except `IconName`), literal `px(<number>)` for control sizes, `rgb(`, `rgba(`, `hsla(` outside the exception list; first run records the current violations as the exception list, later tasks shrink it; remove `no_inline_colors` once covered.
  Verify: `cargo xtask ui-check`. Depends on: T-010. Risk: low.

- [ ] T-083 | M6 | Migrate `multichart/drawing_ui.rs` | M
  Steps: use `wyck-ui` buttons and the swatch row; drop local `swatch_color`; file under 800 lines; shrink the exception list.
  Verify: default check, `ui-check`, before and after capture. Depends on: T-081, T-082. Risk: medium.

- [ ] T-084 | M6 | Migrate `dashboard/header.rs` and `picker.rs` | M
  Steps: same as T-083; remove `.h(px(32.))` and the 7 literals in `picker.rs`.
  Verify: same. Depends on: T-081, T-082.

- [ ] T-085 | M6 | Migrate `settings_hub/**` | M
  Steps: same; replace the 10 buttons in `data.rs` and the literals in `look.rs`.
  Verify: same. Depends on: T-081, T-082.

- [ ] T-086 | M6 | Migrate chart settings dialogs | M
  Files: `chart/{export_ui,chart_settings_ui,settings_rows}.rs`.
  Steps: use `wyck-ui` rows; delete the local `choice`, `switch`, `number`, `mono`; split `export_ui.rs` by page.
  Verify: same. Depends on: T-081, T-082.

- [ ] T-087 | M6 | Migrate `study_settings.rs` and `drawing_props.rs` | M
  Steps: same; keep generated rows from the spec; split by page.
  Verify: same, plus T-005 snapshots. Depends on: T-086.

- [ ] T-088 | M6 | Migrate `trading/panel` | M
  Steps: use `wyck-ui` table and tabs; remove local `tab_button`, `tint`; split `view.rs` (rows, menus, filters, actions).
  Verify: same. Depends on: T-081, T-082.

- [ ] T-089 | M6 | Migrate `trading/ticket` | M
  Steps: same; split `ticket/mod.rs` into state, plan, send, lines modules; each under 600 lines.
  Verify: same. Depends on: T-088, T-061.

- [ ] T-090 | M6 | Migrate `indicators/editor` and `chart/overlay.rs` | M
  Steps: same; remove local `rgb` helper in `overlay.rs`; unify the chart palette with the theme where possible (record leftovers as exceptions).
  Verify: same. Depends on: T-081, T-082.

- [ ] T-091 | M6 | Move work out of `render` | S
  Files: `trading/panel/view.rs:1534-1551`, `trading/ticket/view.rs:574-582`, `multichart/mod.rs:1117-1137`, `chart/mod.rs:536`.
  Steps: compute rows and plans on data change, not on render; stop the per-second task unless something is pending.
  Verify: default check; manual check of panel and ticket updates. Risk: medium.

- [ ] T-092 | M6 | Split the biggest remaining files | M
  Files: `trading/account.rs`, `dashboard/mod.rs`, `workspace/preferences.rs`, `multichart/mod.rs`.
  Steps: split by responsibility without behavior change.
  Verify: default check. Depends on: T-065, T-089. Risk: medium.

## M7 Indicators

- [ ] T-100 | M7 | `ParamValue` and migration | M
  Files: `crates/wyck-indicators/src/{spec,config}.rs`.
  Steps: typed values; `StudyConfig` serializes a version field; reading the old `BTreeMap<String, f64>` form migrates; test with a captured old file.
  Verify: default check, snapshots. Depends on: T-049. Risk: medium (user data).

- [ ] T-101 | M7 | New input kinds and layout fields | M
  Steps: add `Str`, `Symbol`, `Timeframe`, `Session` kinds and `group`, `inline`, `tooltip` on `InputSpec`; stable option identifiers instead of indices; scripts map `section` onto `group`.
  Verify: default check, snapshots. Depends on: T-100.

- [ ] T-102 | M7 | Generated form | M
  Files: `crates/wyck/src/chart/study_settings.rs`, `wyck-ui` form pieces.
  Steps: render groups and inline rows from the spec; searchable symbol select over `SymbolTable`; timeframe picker; express the `Atr` and `VolumeProfile` special cases in the spec where possible.
  Verify: default check; manual check of 6 indicators. Depends on: T-101, T-081.

- [ ] T-103 | M7 | Common offset and wider source | M
  Steps: `offset` input on all overlay studies; `Source` accepts volume and the plot of another study.
  Verify: snapshots (defaults unchanged). Depends on: T-101.

- [ ] T-104 | M7 | External series for symbol and timeframe inputs | M
  Steps: provider trait in `wyck-indicators`; app implementation reusing `chart/history.rs` loading and subscriptions; studies with a different symbol or timeframe show a loading state.
  Verify: default check; manual check on demo. Depends on: T-102. Risk: medium.

- [ ] T-105 | M7 | Reference values | S
  Steps: pick 6 indicators (RSI, MACD, EMA, Bollinger, ATR, Supertrend); compute reference values with a third-party source; add tests with a tolerance.
  Verify: new tests pass. Depends on: T-005.

- [ ] T-106 | M7 | Skill `add-indicator` | S
  Files: `.claude/skills/add-indicator/SKILL.md`.
  Verify: follow the skill once for a dummy study, then revert it. Depends on: T-103.

## M8 Docs and agent environment

- [ ] T-110 | M8 | `docs/README.md`, `ARCHITECTURE.md`, `layers.toml` final | S
- [ ] T-111 | M8 | `docs/CONVENTIONS.md` (naming, errors, logs, modules, async, comment policy, pedantic subset) | S
- [ ] T-112 | M8 | `docs/UI_GUIDELINES.md` (tokens, sizes per context, allowed components, captures) | S
- [ ] T-113 | M8 | `docs/CTRADER.md` (auth sequence, phases, limits, errors, gotchas) | S
- [ ] T-114 | M8 | `docs/INDICATORS.md` (spec, add an indicator, reference values) | S
- [ ] T-115 | M8 | `docs/TESTING.md` (pyramid, fake transport, snapshots, live test rules) | S
- [ ] T-116 | M8 | `docs/GLOSSARY.md` (Symbol, Period vs Timeframe, Bar, ctid, Study vs Indicator) | S
- [ ] T-117 | M8 | README of at most 15 lines per crate | S
- [ ] T-118 | M8 | ADRs 0001 to 0007 in `docs/adr/` | S
  Each of T-110 to T-118: write in English, ASCII, link do not copy, run the one-liner. Verify: `cargo xtask docs-check` once T-120 exists, otherwise manual link check.

- [ ] T-119 | M8 | Rules, nested files, skills, hooks | M
  Note: `.gitignore` ignores `.claude/` today. Ask the maintainer first; the proposed change is to ignore only `.claude/settings.local.json` and local state, and to version `.claude/rules/`, `.claude/skills/` and `.claude/settings.json` (see `PLAN.md` section 9).
  Files: `.claude/rules/{domain,ui,ctrader,tests,docs}.md` (each under 60 lines, with `paths:`), `crates/wyck-openapi/CLAUDE.md`, `crates/wyck-ui/CLAUDE.md`, `.claude/skills/{add-ui-component,add-panel,bump-gpui,new-crate}/SKILL.md`, `.claude/settings.json` hooks (fmt after `.rs` edits, reminder to run `cargo xtask check`).
  Verify: `/context` shows the files load; `/doctor` reports sizes fine. Depends on: T-110 to T-118.

- [ ] T-120 | M8 | `xtask docs-check` | S
  Also: replace the perl one-liner of `AGENTS.md` for forbidden characters (it aborts on tracked binaries such as `crates/wyck/assets/app-icon.png`, `sounds/*.wav`); a text-only check, for example `git grep -nIP '(*UTF8)[\x{2013}\x{2014}\x{2018}\x{2019}\x{201C}\x{201D}\x{2026}\x{2190}-\x{2193}\x{2212}\x{D7}\x{2248}\x{200B}\x{FEFF}]'`, works today and finds nothing.
  Steps: fail when `AGENTS.md` or `CLAUDE.md` exceed 200 lines, an internal Markdown link is broken, or a crate has no README of at most 15 lines.
  Verify: `cargo xtask docs-check`. Depends on: T-117.

- [ ] T-121 | M8 | Cold agent test | M
  Steps: in a fresh session with only `AGENTS.md` and docs, ask for (a) a new indicator with inputs, (b) a new button in a panel, (c) a new cTrader message; record where the agent went wrong; fix the docs, not the prompt.
  Verify: all three succeed without human correction. Depends on: T-119, T-106.

## M9 Hardening

- [ ] T-130 | M9 | Full CI | S
  Steps: `cargo xtask check` with machete, deny, arch-check, ui-check, docs-check, dupes on the 3 OS matrix; cache kept.
  Verify: green run on main.
- [ ] T-131 | M9 | Final metrics | S
  Steps: re-run the baseline commands; fill the before and after table in `PLAN.md` section 6 and `PROGRESS.md`.
- [ ] T-132 | M9 | Remove `docs/refactor/audit/` | S
  Steps: delete the audit folder (git remembers); keep `PLAN.md`, `TASKS.md`, `PROGRESS.md`, `RISKS.md` or archive them in a tag, as the maintainer prefers.
