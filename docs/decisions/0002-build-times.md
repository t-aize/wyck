# 0002: Build time baseline and fallback threshold

- Status: proposed
- Date: 2026-10-09

## Context

Merging five crates into one package makes any edit recompile the whole package. The cost has to
be measured, not guessed. The baseline below is still empty: it must be filled in on the
maintainer's machine from the commit tagged `pre-restructure`.

## Baseline (workspace, tag `pre-restructure`)

| Measure | Value |
|---|---|
| `cargo build` from clean | to measure |
| Rebuild after touching a file in `wyck-chart` | to measure |
| Rebuild after touching a file in `crates/wyck/src/trading` | to measure |
| `cargo check` after touching one file | to measure |

Command: `cargo build --timings`, then `touch <file>` and rebuild.

## Decision

Keep the single package while the rebuild after one edit stays under a threshold written here
once measured (proposed: no more than 1.5 times the baseline for the same edit).

## Consequences

If the threshold is exceeded, extract `domain` first, then `infra::ctrader`, back into crates.
The layer rules (see `docs/architecture.md`) make that a mechanical move.
