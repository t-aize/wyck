# Progress

Read this first in a fresh session. Then open `TASKS.md` at the first unchecked task of the current milestone.

## Current milestone

M0 Safety net. Nothing in the project has been changed yet. Only `docs/refactor/` was added (uncommitted).

## Done

- Audit of all 5 crates (3 read-only sub-agent passes plus direct measurements). Evidence is in `audit/`.
- `cargo check --workspace --all-targets`: 1 min 37 s, 0 warnings.
- `cargo clippy --workspace --all-targets -W clippy::pedantic`: 1949 warnings (counts in `audit/00-baseline.md`).
- Plan approved with four decisions (see top of `PLAN.md`).

## Next

1. T-001 tag and branch (needs the maintainer's go-ahead).
2. T-002 commit these documents.
3. T-003 to T-009 finish the baseline and the safety net.

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
