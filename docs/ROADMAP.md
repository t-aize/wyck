# Roadmap

Progress of the restructuring: one Cargo package at the root, layered modules in `src/`.
Read this file before starting a task and update it when you finish one.

Status: `todo`, `doing`, `done`. Only one phase is `doing` at a time.

Not compiled since phase 0: the restructuring is done without running `cargo`. Run
`cargo check --all-targets` after pulling and fix what it reports before building on top.

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
| 8 | Move modules into `domain/`, `infra/`, `app/`, `ui/` | todo |
| 9 | Cut dependency cycles, enforce layers (checkpoint B) | todo |
| 10 | Design system and screen migration | todo |
| 11 | Sign-in modal | todo |
| 12 | Indicator inputs v2 (checkpoint C) | todo |
| 13 | Domain and cTrader client hardening | todo |
| 14 | Documentation pass | todo |

## Phase 0 checklist

- [x] Baseline is commit `c805e65` (last commit of `main` before the restructuring). The tag could not be
      pushed from the cloud session: run `git tag pre-restructure c805e65 && git push origin pre-restructure`.
- [ ] Record the exact test count (`cargo test --workspace --all-features`) on the maintainer's machine.
- [ ] Fill in the build times table of `docs/decisions/0002-build-times.md` (maintainer's machine).
- [x] Manual smoke checklist written in `docs/testing.md`.
- [x] Key binding collisions fixed: the rectangle tool moves to `Alt+X`, the tool finder to
      `Ctrl+Shift+F`; `src/keymap_guard.rs` fails on any duplicate binding in one context.
- [x] Create this roadmap and the decision records folder.

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
