# Roadmap

Progress of the restructuring: one Cargo package at the root, layered modules in `src/`.
Read this file before starting a task and update it when you finish one.

Status: `todo`, `doing`, `done`. Only one phase is `doing` at a time.

Not compiled since phase 0: the restructuring is done without running `cargo`. Run
`cargo check --all-targets` after pulling and fix what it reports before building on top.

| Phase | Goal | Status |
|---|---|---|
| 0 | Safety net and baseline measures | done |
| 1 | Move the app crate to the repo root, keep `crates/` as a transitional workspace | todo |
| 2 | Merge `wyck-ui` into the package | todo |
| 3 | Merge `wyck-chart` | todo |
| 4 | Merge `wyck-config` | todo |
| 5 | Merge `wyck-openapi`, drop the `client` feature, add `src/lib.rs` | todo |
| 6 | One manifest, no workspace (checkpoint A) | todo |
| 7 | Native window title bar | todo |
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
