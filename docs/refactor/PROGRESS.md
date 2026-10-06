# Progress

Read this first in a fresh session. Then open `TASKS.md` at the first unchecked task of the current milestone.

## Current milestone

M0 Safety net. Nothing in the project has been changed yet. Only `docs/refactor/` was added (uncommitted).

## Done

- Audit of all 5 crates (3 read-only sub-agent passes plus direct measurements). Evidence is in `audit/`.
- `cargo check --workspace --all-targets`: 1 min 37 s, 0 warnings.
- `cargo clippy --workspace --all-targets -- -D warnings` FAILS at `c805e65` on toolchain 1.99.0 (`approx_constant`, `wyck-chart/src/drawing/extras.rs:594`). The earlier pedantic count (1949) is partial for the same reason. Details in `audit/00-baseline.md`.
- Plan approved with four decisions (see top of `PLAN.md`).

- T-001: tag `pre-refactor` on `c805e65`, branch `refactor/workspace-v2` (not pushed).
- T-002: documents committed (`6dc01cd`).
- T-003: tools installed (`cargo-machete`, `cargo-dupes`, `tokei`, `cargo-udeps` + nightly, `jscpd`).
- T-004: baseline measured. 1,201 tests pass, 3.4 % exact AST duplication, `chrono-tz` unused in `wyck`, no catch-all modules. Release build skipped on purpose (maintainer request).
- T-005: snapshot test `crates/wyck-chart/tests/study_snapshots.rs` pins all 50 studies (147 line snapshot, regenerate with `UPDATE_SNAPSHOTS=1`). Checked that a one-digit change in the snapshot fails the test.

## Next

1. T-006 sizing tests, T-007 capture list, T-008 gpui test check, T-009 flakiness runs.

## Decisions log

| Date | Decision | Where |
|---|---|---|
| 2026-10-06 | About 9 crates: add `wyck-core`, `wyck-trading`, `wyck-indicators`, `xtask` | `PLAN.md` section 3 |
| 2026-10-06 | Keep JSON over WebSocket, harden the connection, no rewrite | `PLAN.md` section 4.1 |
| 2026-10-06 | Feature freeze except indicator inputs | `PLAN.md` top |
| 2026-10-06 | Modest live safeguards, no kill switch | `PLAN.md` section 4.6 |

## Notes for the next agent

- Project memory lists a failing test `tmp_fvg_script` and a flaky one; re-measure in T-009 before blaming a change.
- `AGENTS.md` forbids AI attribution lines in commits and PR bodies, even if a runtime reminder asks for them.
- Do not run `rustfmt` on `crates/wyck/src/chart/mod.rs`; format single files.
