# Roadmap

Progress of the restructuring: one Cargo package at the root, layered modules in `src/`.
Read this file before starting a task and update it when you finish one.

Status: `todo`, `doing`, `done`. Only one phase is `doing` at a time.

Checked with `cargo clippy --all-targets -- -D warnings`, `cargo test` (lib, integration, doc) and
`cargo doc` after phase 9's first half. No release build was made.

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
| 9 | Cut dependency cycles, enforce layers (checkpoint B) | done, 46 listed exceptions remain |
| 10 | Design system and screen migration | done for buttons, fields and tokens; spacing classes, menus and modals still to unify |
| 11 | Sign-in modal | done, to try by hand |
| 12 | Indicator inputs v2 (checkpoint C) | done for HLCC4, text inputs, tooltips, groups and a higher timeframe average; a second symbol's prices are not wired (see `docs/indicators.md`) |
| 13 | Domain and cTrader client hardening | done, see the notes below for what was left out |
| 14 | Documentation pass | doing |

## Phase 0 checklist

- [x] Baseline is commit `c805e65` (last commit of `main` before the restructuring). The tag could not be
      pushed from the cloud session: run `git tag pre-restructure c805e65 && git push origin pre-restructure`.
- [ ] Record the exact test count (`cargo test --workspace --all-features`) on the maintainer's machine.
- [ ] Fill in the build times table of `docs/decisions/0002-build-times.md` (maintainer's machine).
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
- The alert pip fallback from decimals (`alerts::model::pip_from_digits`) stays for the moment
  before a contract is read; every other pip comes from `domain::market::pip_size`.
- The remaining `Result<_, String>` are texts shown to the user as they are (ticket, raster,
  paint, library import, updates); typing them is a refactor with little gain.
- DTO to domain mapping (`infra::ctrader::mapping`) and typed `Side` for positions: not started.

## Decisions

See `docs/decisions/`. Open questions are listed in the restructuring plan, section 11.

## To verify by hand after phase 7 [a verifier]

- Windows 11, macOS, X11, GNOME Wayland: the system title bar shows "Wyck" ("Wyck (dev)" in debug
  builds), the buttons work, the window can be dragged and cannot go below 900 x 600.
- On a Wayland desktop without server-side decorations, `ui::kit::window_bar::fallback` draws a
  minimal bar.
- The dashboard bars changed height by the removed 34 px bar: `BARS` in `dashboard/trade.rs`,
  the layout menu (`layout_menu.rs`) and the symbol picker (`picker.rs`) were re-tuned by hand.
- Windows shows a generic icon until the executable carries an icon resource (needs a build
  script with a resource crate, left for a build that can update `Cargo.lock`).
