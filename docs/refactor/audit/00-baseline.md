# Baseline

Measured on 2026-10-06 at commit `c805e65` (branch `main`, clean working tree), Windows 11, cargo 1.99.0, toolchain `stable`. Tools run read-only. Cells marked "not measured" are filled by T-004.

## Git

- Branch `main`, up to date with `origin/main`. Tags `v0.1.0` to `v0.1.8` exist. No `pre-refactor` tag yet (T-001).
- Remote branches: `claude/sharp-cori-dlknwx`, `dependabot/cargo/gpui-kit-0.7.0`.

## Audit tools (installed in T-003)

`cargo-machete` 0.9.2, `cargo-dupes` 0.2.1, `tokei` 15.0.0, `cargo-udeps` 0.1.61 (needs `+nightly`, rustc 1.101.0-nightly ea137335b 2026-10-05), `jscpd` 5.4.0, `cargo-deny` (already installed).

## Build and lint

| Command | Result |
|---|---|
| `cargo check --workspace --all-targets` | 1 min 37 s, 0 warnings |
| `cargo clippy --workspace --all-targets -- -D warnings` (toolchain 1.99.0) | **fails** on a clean checkout of `c805e65`: `error: approximate value of f64::consts::GOLDEN_RATIO found` at `crates/wyck-chart/src/drawing/extras.rs:594` (`let golden: f32 = 1.618_034;`), a deny-by-default lint. CI uses a floating `stable`, so it breaks as soon as the runner has 1.99. Fix in T-010 |
| `cargo clippy --workspace --all-targets -- -W clippy::pedantic` | 1949 warnings, **partial**: the `wyck-chart` lib failed to compile under clippy because of the error above, so `wyck` and `wyck-ui` were never linted. Redo after the fix (T-012) |
| `cargo tree --workspace -d` | 56 crates present in more than one version, mostly pulled by gpui (see `deny.toml` comment on `multiple-versions`) |
| `cargo test --workspace` | 1,201 passed, 0 failed, 16 ignored (12 live tests in `wyck-openapi/tests/live.rs`, 1 `chart::raster::tests::indicator_families_preview`, 3 doctests), 30 s once built. One run only; flakiness check is T-009. `tmp_fvg_script` from the old project notes no longer exists in the code |
| `cargo build --release` | not measured on purpose (maintainer request; fat LTO build is slow, CI builds it on main) |
| `cargo machete`, `cargo udeps` | see "Unused dependencies" below |
| `cargo dupes`, `jscpd`, `tokei` | see "Unused dependencies, duplication" below |
| `cargo deny` | installed, not run in this audit |

Top pedantic lints: 326 `long literal lacking separators`, 318 plus 237 missing `#[must_use]`, 184 `i64` to `f64` casts, 116 `f64` to `f32`, 83 `usize` to `f64`, 55 `assert!(x.is_empty())` style, 55 manual `midpoint`, 46 `usize` to `i64`, 35 strict float comparison.

## Size

| Crate | .rs files | Lines |
|---|---|---|
| `wyck` | 118 | 56,217 |
| `wyck-chart` | 59 | 37,252 |
| `wyck-openapi` | 59 | 17,540 (src 12,213, tests 5,062, examples) |
| `wyck-config` | 20 | 6,756 (src 6,069, tests 472) |
| `wyck-ui` | 19 | 5,956 |
| Total | 275 | 123,721 |

Largest files: `wyck-chart/src/drawing/book.rs` 2740, `wyck/src/trading/ticket/mod.rs` 2150, `wyck-chart/src/study/mod.rs` 2053, `wyck-chart/src/drawing/extras.rs` 2039, `drawing/model.rs` 1936, `drawing/geometry.rs` 1936, `wyck-chart/src/export.rs` 1876, `wyck/src/trading/panel/view.rs` 1723, `wyck/src/trading/account.rs` 1661, `wyck-config/src/backup.rs` 1565.

## Quality counters

| Counter | Value |
|---|---|
| `unwrap/expect/panic!` outside tests | about 9: app 5 `expect` (`main.rs` 2, `runtime.rs` 1, `connection/mod.rs` 2), chart 1 `expect` (`study/custom/run.rs:393`) and 1 `panic!` (`study/extended.rs:439`), ui 2 `unwrap` (`text_input.rs:643,653`), openapi 1 `expect` (`auth/oauth.rs:76`) |
| Raw `unwrap/expect/panic!` including tests | wyck 214, chart 287, config 413, openapi 451, ui 2 |
| `println!/eprintln!` in library code | 0 (only examples and `tests/live.rs`) |
| `#[allow(dead_code or unused)]` | 6 (all `unused_variables` in `drawing_ui.rs`) plus `tests/support/mod.rs` |
| TODO, FIXME, HACK | 0 |
| `Arc<Mutex<..>>` or `Arc<RwLock<..>>` | 8 |
| Literal `px(<number>)` | 184 (app 143, ui 39, other) |
| Color constructors `rgb(`, `rgba(`, `hsla(` | 232 total (44 in the app outside `dashboard/marks.rs` hex palette) |
| `unsafe` | forbidden by `[workspace.lints.rust]` |

## Comment ratio

Lines starting with `//` (including `///` and `//!`) over all lines: wyck 3110 of 56,217, wyck-chart 2448 of 37,252, wyck-config 1249 of 6,756, wyck-openapi 2983 of 17,540, wyck-ui 643 of 5,956. Doc plus comment lines per line of code (blank lines excluded) from the sub-agent pass: app 6.2 %, chart 7.5 %, ui 13.3 %, openapi src about 30 %, config src about 28 %.

## Tests

Passed tests per binary (from the run above): wyck 246 (+1 ignored), wyck-chart 517, wyck-ui 21 (+ doctests), wyck-config 93 unit + 10 in `tests/lifecycle.rs`, wyck-openapi 160 unit plus integration `account` 7, `auth` 7, `client` 21, `handle` 2, `margin` 5, `market` 13, `properties` 16, `robustness` 13, `session` 31, `trading` 9, `live` 12 ignored. `wyck-openapi`: unit tests in most modules, integration tests in `tests/{client,session,robustness,market,trading,account,margin,auth,handle}.rs` against a scripted local WebSocket server, 16 proptest properties in `tests/properties.rs`, and 13 `#[ignore]` live tests in `tests/live.rs`. No `#[gpui::test]` anywhere.

## CI (`.github/workflows/ci.yml`)

`cargo fmt --check`; per OS (Linux, Windows, macOS) `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` and `cargo test --locked --workspace --all-features`; `cargo clippy -p wyck-openapi --no-default-features --lib`; `cargo doc` with `-D warnings` on Linux; release build on main; a single `ci-ok` gate job. `audit.yml` runs `cargo-deny` and RustSec. No coverage.

## Existing repo files

`AGENTS.md` (git attribution rule and writing style only), `CLAUDE.md` (`@AGENTS.md`), `CONTRIBUTING.md`, `README.md`, `RELEASING.md`, `SECURITY.md`, `SUPPORT.md`, `deny.toml`, `rust-toolchain.toml` (stable), `scripts/` (one shell, three python release helpers). No `docs/`, no `.claude/`, no `xtask`.

## Unused dependencies, duplication, catch-all modules (T-004, partial)

- `cargo machete --with-metadata`: `chrono-tz` unused in `crates/wyck/Cargo.toml`. Nothing else reported.
- `cargo +nightly udeps --workspace --all-features` (4 min 12 s): "All deps seem to have been used". It misses `chrono-tz` in `wyck`; `rg "chrono_tz|chrono-tz" crates/wyck` finds only the `Cargo.toml` line (`:68`), so the dependency is unused and `machete` is right. T-013 removes it.
- Catch-all modules: `rg "mod (utils|common|helpers)"` finds none. Hypothesis H7 is wrong on this point.
- `tokei crates`: Rust 275 files, 105,899 code lines, 1,305 `//` comment lines, 7,294 blank lines (doc comments `///` and `//!` are counted by tokei as 8,356 Markdown lines inside Rust).
- `cargo dupes stats` (AST units, 97,752 lines): 5,484 units, 234 exact groups (679 units, 3,289 lines, 3.4 %) and 72 near groups (150 units, 1,176 lines, 1.2 %). Most exact groups are one-line accessors (group 1: 25 `len`, `is_empty`, `dir` style getters), so the raw percentage overstates the problem. The groups worth acting on:
  - 19 near identical `fn label(&self)` enum matches (`wyck/src/appearance/mod.rs:46`, `trading/panel/prefs.rs:153,221,243`, `wyck-chart/src/export.rs:184,273,324,379`, `footprint.rs:37`, `options.rs:136`, `tpo.rs:82`, `volume.rs:41,78,124`...); a small macro or derive removes them.
  - `ColorField::{label,get}` (`wyck/src/appearance/mod.rs:102-140`) vs `ColorKey::{label,get}` (`wyck/src/chart/chart_settings_ui.rs:121-161`): near copies (94 and 97 % similar), plus `Period::label` (`wyck-openapi/src/market/bars.rs:110`).
  - `Session::subscribe_spots` vs `subscribe_depth` (`session/mod.rs:406-432`, `518-544`) and the matching `unsubscribe_*` pair (`:440-459`, `:551-570`).
  - `TradingClient::amend_order` vs `amend_position_sl_tp` (`trading/client.rs:107-127`, `164-182`); `AccountDataClient::deals_by_position` vs `orders_by_position` (`account/client.rs:174-225`); `cash_flow_history` vs `symbols_for_conversion`.
  - `ColumnKey::code` vs `default_name` (`wyck-chart/src/export.rs:735-772`, `821-858`, 99 % similar, 38 lines); `ChartKind::label` vs `code` (`settings.rs:76-119`).
  - Test helpers repeated in `wyck-openapi/tests/{session,margin,trading}.rs`.
- `jscpd --min-lines 8 --min-tokens 70` (Rust only, `tests/` excluded): 255 files, 118,117 lines, 38 clones, 492 duplicated lines (0.42 %). Largest: `trading/account.rs:1024-1039` vs `:1100-1116` (place and batch), `trading/panel/customize.rs:489` vs `trading/ticket/customize.rs:593`, `panel/dialogs.rs:425` vs `ticket/view/order.rs:33` (the two local `select` copies), `panel/view.rs:266` vs `:869`, `multichart/drawing_ui.rs:575` vs `:600` (swatch rows).
- Duplication threshold proposed for `xtask`: fail on more than 3.5 % exact AST duplication at the start, lower it after M2 and M6. Re-measure then.
- Both tools show that the real redundancy is local boilerplate and a few UI blocks, not copies across crates. H1 stays "partial".

## Still open

- Flakiness check by repeated runs (T-009).
- Avoidable duplicate dependency versions (T-013).
- Release build time: skipped on purpose.

## Flakiness check (T-009)

Four full `cargo test --workspace` runs (one in T-004, three in T-009) at `c805e65` plus the new tests of T-005 and T-006:

| Run | Result |
|---|---|
| T-004 | 1,201 passed, 0 failed |
| T-009 run 1 | **1 failed**: `wyck-config` `tests/lifecycle.rs` `many_threads_saving_one_document_never_tear_it`; cargo stopped before the later test binaries (867 tests ran) |
| T-009 run 2 and 3 | 1,219 passed, 0 failed, 16 ignored |

Cause, reproduced: the test alone passes 40 of 40 times, but 8 processes in parallel (CPU load) failed 5 of 120 runs, always with `PermissionDenied` ("Acces refuse", OS error 5) on `layout.toml`, either in `DocumentStore::save` (`atomic_write`, `fs_util.rs:44-67`, the `fs::rename` over an existing file) or in `DocumentStore::load` (a read during another thread's rename). On Windows a rename over a file that another thread is replacing or reading can fail transiently. The data is not torn (the test's invariant held), but a save returns an error. This is a real Windows race in production code, not only in the test; tracked as T-033.

`a_chart_computes_a_script_it_holds_through_the_config` (listed as flaky in the old project notes) did not fail in these four runs. Not reproduced; keep the note until a longer run says otherwise.
