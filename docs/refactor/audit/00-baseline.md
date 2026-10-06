# Baseline

Measured on 2026-10-06 at commit `c805e65` (branch `main`, clean working tree), Windows 11, cargo 1.99.0, toolchain `stable`. Tools run read-only. Cells marked "not measured" are filled by T-004.

## Git

- Branch `main`, up to date with `origin/main`. Tags `v0.1.0` to `v0.1.8` exist. No `pre-refactor` tag yet (T-001).
- Remote branches: `claude/sharp-cori-dlknwx`, `dependabot/cargo/gpui-kit-0.7.0`.

## Build and lint

| Command | Result |
|---|---|
| `cargo check --workspace --all-targets` | 1 min 37 s, 0 warnings |
| `cargo clippy --workspace --all-targets -- -W clippy::pedantic` | 1949 warnings |
| `cargo tree --workspace -d` | 56 crates present in more than one version, mostly pulled by gpui (see `deny.toml` comment on `multiple-versions`) |
| `cargo test --workspace` | not measured (T-004) |
| `cargo build --release` | not measured (T-004) |
| `cargo machete`, `cargo udeps` | tools not installed (T-003) |
| `cargo dupes`, `jscpd` | tools not installed (T-003) |
| `tokei` | not installed; line counts below come from `wc -l` |
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

`#[test]` counts: app 247, wyck-chart 517, wyck-ui 21, wyck-config about 130. `wyck-openapi`: unit tests in most modules, integration tests in `tests/{client,session,robustness,market,trading,account,margin,auth,handle}.rs` against a scripted local WebSocket server, 16 proptest properties in `tests/properties.rs`, and 13 `#[ignore]` live tests in `tests/live.rs`. No `#[gpui::test]` anywhere.

## CI (`.github/workflows/ci.yml`)

`cargo fmt --check`; per OS (Linux, Windows, macOS) `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` and `cargo test --locked --workspace --all-features`; `cargo clippy -p wyck-openapi --no-default-features --lib`; `cargo doc` with `-D warnings` on Linux; release build on main; a single `ci-ok` gate job. `audit.yml` runs `cargo-deny` and RustSec. No coverage.

## Existing repo files

`AGENTS.md` (git attribution rule and writing style only), `CLAUDE.md` (`@AGENTS.md`), `CONTRIBUTING.md`, `README.md`, `RELEASING.md`, `SECURITY.md`, `SUPPORT.md`, `deny.toml`, `rust-toolchain.toml` (stable), `scripts/` (one shell, three python release helpers). No `docs/`, no `.claude/`, no `xtask`.

## To fill in M0

- Test run and known failures (T-004, T-009).
- Release build time (T-004).
- Unused dependencies (T-004), avoidable duplicate versions (T-013).
- Duplication report and the chosen threshold (T-004).
- `utils`, `common`, `helpers` modules (T-004).
