# Roadmap

Progress of the restructuring: one Cargo package at the root, layered modules in `src/`.
Read this file before starting a task and update it when you finish one.

Status: `todo`, `doing`, `done`. Only one phase is `doing` at a time.

| Phase | Goal | Status |
|---|---|---|
| 0 | Safety net and baseline measures | doing |
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

- [ ] Tag `pre-restructure` on the last commit of `main`.
- [ ] Record the exact test count (`cargo test --workspace --all-features`).
- [ ] Record build times (`cargo build --timings`, then rebuild after touching one file in
      `wyck-chart` and one in `crates/wyck/src/trading`) in `docs/decisions/0002-build-times.md`.
- [ ] Write the manual smoke checklist in `docs/testing.md`.
- [ ] Fix the two key binding collisions in the `Dashboard` context (`alt-b`, `secondary-shift-k`)
      and add a test that fails on a duplicate binding in one context.
- [x] Create this roadmap and the decision records folder.

## Decisions

See `docs/decisions/`. Open questions are listed in the restructuring plan, section 11.
