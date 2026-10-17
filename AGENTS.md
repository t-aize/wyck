# Wyck

A desktop trading app for cTrader accounts, written in Rust on gpui: charts, drawing tools,
indicators written as scripts, an order ticket with a safety guard, alerts, backups. This file is
the map; the `AGENTS.md` of each crate says what is inside it, and is loaded when you work there.

## Commands

Run them from the root. `cargo xtask` is plain Rust, so it works the same on Windows, macOS and Linux.

```
cargo xtask check          # format, clippy, tests, cargo-deny, rustdoc, dependency rules, docs files
cargo xtask check --full   # the same, then cargo-machete, typos, taplo, zizmor, actionlint (the missing ones are skipped)
cargo xtask ci             # what the `check` job of CI runs: format, clippy, tests, cargo-deny, rustdoc
cargo xtask deps           # layering and dependency rules only
cargo xtask docs-check     # AGENTS.md / CLAUDE.md files only
cargo test -p wyck-chart   # one crate (doc-tests included); `cargo test --workspace` for all of them
cargo run -p wyck          # the app; a debug build keeps its state apart from the installed app
cargo run -p wyck-ctrader --example ctrader_prices   # the cTrader client against a demo account
```

Do not run `cargo build --release`: fat LTO is slow and CI builds the release.
The window needs a signed-in account (a real cTrader login). Never type real credentials in a
session; to see startup, run the debug binary for a few seconds with `WYCK_CONFIG_DIR`,
`WYCK_DATA_DIR` and `WYCK_KEYRING_SERVICE` pointing at a temporary folder.

## The workspace

One crate per area in `crates/`, plus `xtask/`. An arrow goes down only; `cargo xtask deps` fails on
any other.

| Crate | Owns |
|---|---|
| `wyck-clock` | the one wall-clock read of the workspace |
| `wyck-core` | tiny shared vocabulary: number formats, `ta` arithmetic, appearance, text scale |
| `wyck-market` | symbols, bars, ticks, quotes, trading hours, `Timeframe` |
| `wyck-trading` | orders, plans, the safety guard, margin, the account book, `atr_stop` |
| `wyck-drawings` | drawing tools, their shapes, the drawing book and its undo |
| `wyck-indicators` | indicators, their maths, the Rhai scripts engine |
| `wyck-chart` | the chart as a model: series, view, scene (draw commands), export |
| `wyck-platform` | log files, panic reports, the tokio runtime, build facts |
| `wyck-storage` | where files live, documents, migrations, backups, secrets, encryption |
| `wyck-ctrader` | the cTrader Open API client (the only network code) |
| `wyck-app` | gpui entities with no view: sign-in, account, alerts, workspace, updates |
| `wyck-ui-kit` | the design system (the only crate that names `gpui-kit`) |
| `wyck-ui-chart`, `wyck-ui-trading`, `wyck-ui-editor`, `wyck-ui-shell` | the screens |
| `wyck` | the binary: `main`, packaging, the two guards that read every screen's sources |
| `xtask` | the developer commands above |

```
clock, core <- market <- trading <- drawings <- indicators <- chart
platform <- storage                  ctrader -> core, market, trading, clock
app -> chart, indicators, drawings, trading, market, storage, platform, ctrader, clock
ui-kit -> core ; ui-chart -> ui-kit, app, chart ; ui-trading, ui-editor -> ui-chart
ui-shell -> every ui-* crate ; wyck -> ui-shell, ui-editor, ui-kit, app, platform, storage
```

## Rules that hold everywhere

- **Layers.** The domain crates (`core` to `chart`) have no `gpui`, `tokio`, `reqwest`, `keyring` or
  file access. Only `wyck-ctrader` talks to the network, only `wyck-storage` writes the user's files
  and the keyring, only `wyck-ui-kit` names `gpui-kit`. A screen reaches the broker, the files and
  the system through the `broker`, `storage` and `system` doors of `wyck-app`.
- **Orders.** An order goes through `Account::place_with` (or `place_batch`) in `wyck-app`: validated,
  cleared by the guard of `wyck-trading`, never resent after a timeout. There is no way around it;
  do not add one.
- **Secrets.** No token, passphrase or account number in a log line, a panic report, a notice or an
  error message. Credentials live in the keyring or encrypted files, never in `config.toml`.
- **Saved formats are a promise.** A field has a serde default and is repaired in `normalized`; a
  change of shape needs a `SCHEMA_VERSION`, a migration step and a fixture of the old file; a
  `Tool` or `ChartKind` code is never renamed.
- **Time.** Read it through `wyck-clock`; `clippy.toml` forbids `SystemTime::now`, `Utc::now` and
  `Local::now`, and `thread::sleep`.
- **Lints** (`[workspace.lints]`, every crate takes them): no `unwrap`, `expect`, `panic`, `todo`,
  `dbg!` or unchecked indexing outside tests; `unsafe` is forbidden; pedantic clippy is on. Say why
  in `#[expect(lint, reason = "...")]`; `#[allow]` is not used. Make an item `pub(crate)` unless
  another crate needs it (`unreachable_pub` warns).
- **Dependencies** are written once, in `[workspace.dependencies]` of the root `Cargo.toml`; a crate
  says `name.workspace = true` and lists its own features. Errors are `thiserror` enums in
  libraries; `anyhow` is only in `xtask` and in `wyck-ui-editor` (its completion provider returns it).
- **Global state** is rare and listed in the `AGENTS.md` of the crate that holds it. Tests must not
  set `wyck_core::text_scale`: other tests read it.
- **Writing.** English. Code, comments and committed text files use plain ASCII: no typographic
  dashes or quotes, no accents. A module doc (`//!`) is 1 to 3 lines (role and invariants); `///` is
  for a unit, an encoding, a panic or a reason, never for what the name already says.

## Done means

1. `cargo xtask check` passes (`--full` before a pull request).
2. The `AGENTS.md` of every crate you touched is still true: its module map, its invariants, its
   `Depends on` and `Used by` lines (`cargo xtask docs-check` checks the last two).
3. A change to a saved format, an order path or a secret is said so in your answer.
4. Nothing is pushed, no pull request is opened and no GitHub setting is changed without being asked.

## Where to read

| File | What it says |
|---|---|
| `README.md` | what the app is, how to install, run and build it |
| `CONTRIBUTING.md` | how to report an issue (pull requests are closed to others for now) |
| `SECURITY.md` | how to report a vulnerability, what is protected |
| `SUPPORT.md` | where to ask for help |
| `CODE_OF_CONDUCT.md` | the code of conduct |
| `crates/wyck-clock`, `crates/wyck-core`, `crates/wyck-market`, `crates/wyck-trading` | `AGENTS.md` of each |
| `crates/wyck-drawings`, `crates/wyck-indicators`, `crates/wyck-chart` | `AGENTS.md` of each |
| `crates/wyck-platform`, `crates/wyck-storage`, `crates/wyck-ctrader`, `crates/wyck-app` | `AGENTS.md` of each |
| `crates/wyck-ui-kit`, `crates/wyck-ui-chart`, `crates/wyck-ui-trading`, `crates/wyck-ui-editor`, `crates/wyck-ui-shell` | `AGENTS.md` of each |
| `crates/wyck`, `xtask` | `AGENTS.md` of each |
| `.claude/rules/` | rules that load when Claude Code reads files of one area |
| `.github/workflows/` | CI (`ci.yml`), the weekly audit, the release (`release.yml`) |
