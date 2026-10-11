# Roadmap

Progress of the restructuring: one Cargo package at the root, layered modules in `src/`.
Read this file before starting a task and update it when you finish one.

Status: `todo`, `doing`, `done`. Only one phase is `doing` at a time.

Checked with `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (lib,
integration, doc) and `cargo doc` (with `-D warnings`) after every phase. No release build was made
and the app was never run: everything marked "to verify by hand" below is untested on a screen.

| Phase | Goal | Status |
|---|---|---|
| 0 | Safety net and baseline measures | done |
| 1 | Move the app crate to the repo root, keep `crates/` as a transitional workspace | done |
| 2 | Merge `wyck-ui` into the package (now `src/ui/kit`), split `lib.rs` and `main.rs` | done |
| 3 | Merge `wyck-chart` (now `src/chart_core`, split in phase 8) | done |
| 4 | Merge `wyck-config` (now `src/infra/storage`) | done |
| 5 | Merge `wyck-openapi` (now `src/openapi`), drop the `client` feature | done |
| 6 | One manifest, no workspace (checkpoint A) | done |
| 7 | Native window title bar | done |
| 8 | Move modules into `domain/`, `infra/`, `app/`, `ui/` | done |
| 9 | Cut dependency cycles, enforce layers (checkpoint B) | done, the baseline is empty |
| 10 | Design system and screen migration | done |
| 11 | Sign-in modal | done, to try by hand |
| 12 | Indicator inputs v2 (checkpoint C) | done for HLCC4, text inputs, tooltips, groups and a higher timeframe average; a second symbol's prices are not wired (see `docs/indicators.md`) |
| 13 | Domain and cTrader client hardening | done, see the notes below for what was left out |
| 14 | Documentation pass | done |
| 15 | Structure pass: typed errors, long files split, tests stay in their modules, no re-exports | done, see below |

## Phase 0 checklist

- [x] Baseline is commit `c805e65` (last commit of `main` before the restructuring). The tag is local; push it with
      `git push origin pre-restructure`.
- [x] Test count (`cargo test --all-features`, 2026-10-11): 1234 passed, 0 failed, 16 ignored.
- [x] Manual smoke checklist written in `docs/testing.md`.
- [x] Key binding collisions fixed: the rectangle tool moves to `Alt+X`, the tool finder to
      `Ctrl+Shift+F`; `src/keymap_guard.rs` fails on any duplicate binding in one context.
- [x] Create this roadmap and the decision records folder.

## Phase 13 notes

Done:

- A connection that says nothing for `silence_timeout` (30 s) is dropped and replaced; the
  transport pings on every heartbeat.
- A token endpoint answering 5xx, 429 or 408 is retried; only an explicit refusal ends the session.
- A token store that fails to save a refreshed pair no longer ends the session
  (`SessionEvent::TokensNotSaved`).
- The backoff starts over only after a connection that lasted `stable_after` (30 s).
- Tokens are renewed on a live connection when they come within `refresh_margin` of expiry.
- A refused sign in has its own `ErrorKind::SignIn`; the callback listener no longer spins on
  `accept` errors.
- Order time in force is built from `TimeInForce` (`NewOrderReq::with_time_in_force`).
- Dead code removed: `SymbolTable`, `SpotTracker`, `DepthBook`, `Client::refresh_tokens`.
- `SoundError` and `LoadConfigError` replace two `Result<_, String>`; `ConfigError` is
  `non_exhaustive`.

Left out, on purpose:

- The OAuth `state` stays optional in the redirect: the portal's documentation does not say it
  echoes the parameter, and a mandatory check could lock every user out. Make it mandatory after
  a live sign in confirms the echo (`auth/callback.rs`, `parse_redirect`).
- The pip guessed from decimals (`market::pip_size_from_digits`) stays for the moment before a
  contract is read; every other pip comes from `domain::market::pip_size`.
- No second domain model for positions, orders and deals: see decision 0003. The side is checked
  where the records enter the account book, and the risk formula is shared.

## Decisions

See `docs/decisions/`.

## To verify by hand after phase 7 [a verifier]

- Windows 11, macOS, X11, GNOME Wayland: the system title bar shows "Wyck" ("Wyck (dev)" in debug
  builds), the buttons work, the window can be dragged and cannot go below 900 x 600.
- On a Wayland desktop without server-side decorations, `ui::kit::window_bar::fallback` draws a
  minimal bar.
- The dashboard bars changed height by the removed 34 px bar: `BARS` in `dashboard/trade.rs`,
  the layout menu (`layout_menu.rs`) and the symbol picker (`picker.rs`) were re-tuned by hand.
- Windows: the executable carries `assets/app-icon.ico` through `build.rs` (the `winresource`
  crate, Windows hosts only). Check Explorer and the taskbar. The icon files and `assets/logo.png`
  come from the two images in `assets/brand/` through `scripts/make_brand_assets.py`: run it
  again when an image changes. The mark shows on the first sign-in step and on the About page.

## Release v0.2.0

`Cargo.toml` is at 0.2.0 and `RELEASING.md` describes the chain, which has never run end to end.
Before pushing the `v0.2.0` tag: CI green on all three systems, the hand checks above and the
smoke checklist in `docs/testing.md` done on a demo account. After the release, check that
`latest.json` is in its assets (the updater downloads it from there).

## Phase 15 notes

- No `Result<_, String>` is left. The order ticket reports `TicketProblem` codes (domain) and
  `ui::features::trading::ticket::problem` words them; the others have their own enum
  (`UpdateError`, `ImportSkip`, `RasterError`, `HexError`, `ExitsProblem`, `SoundError`).
- No `unwrap` or `expect` is left outside tests.
- Unit tests stay inside the module they test. When a module is split, a test goes in the file
  that holds most of the code it exercises; a test that needs a fixture shared with others stays
  with the fixture, so some roots still carry many tests (`domain/drawings/book.rs`,
  `domain/chart/export.rs`, `domain/chart/scene.rs`).
- More than thirty files above 600 lines were split by responsibility. Callers use the real
  paths, with no re-export, and the root files qualify their calls to the children
  (`axes::dash_pattern(..)`). 22 files still have more than 600 lines of code (40 counting their
  tests); the largest, `infra/ctrader/transport/connection.rs`, `app/alerts/model.rs`,
  `domain/drawings/figures.rs` and `app/prefs/ticket.rs`, are under 770 lines of code and are left
  for when they change.
- `EncryptedFileSecretStore` stays: it is the documented store for a machine with no keyring,
  and the storage tests use it as their secret store. The app itself uses the keyring only.
