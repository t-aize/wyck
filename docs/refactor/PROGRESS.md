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
- T-006: 26 contract tests (edge cases of sizing, rounding, protection, rates, chain, summary) in `wyck-openapi/src/trading/contract.rs`.
- T-007: capture checklist in `audit/20-ui.md`; captures themselves still to be taken by the maintainer. `docs/refactor/shots/` is git-ignored.
- T-008: `#[gpui::test]` works with `gpui-pre` 0.3.6 (locked) when `gpui` is a dev-dependency with `features = ["test-support"]`. Verified with a throwaway test in `wyck-ui`, since reverted.
- T-009: four full test runs. One failure, in `wyck-config` `many_threads_saving_one_document_never_tear_it`; reproduced under CPU load (5 of 120), cause is a transient Windows `PermissionDenied` on file replace and read. Task T-033 added to M2. See `audit/00-baseline.md`.

M0 is complete except the before captures, which only the maintainer can take (T-007).

## Next

1. M1: T-010 starts with the clippy fix, then `xtask check`, lints, unused deps, `AGENTS.md`.
2. Before M8: decide the `.claude/` git-ignore question (`PLAN.md` section 9).

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
