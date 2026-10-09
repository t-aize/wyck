# 0001: One Cargo package at the repo root

- Status: accepted
- Date: 2026-10-09

## Context

The repo is a workspace of five crates (`wyck`, `wyck-chart`, `wyck-config`, `wyck-openapi`,
`wyck-ui`) that produce a single application. The real disorder is inside the `wyck` crate: no
internal layers, one dependency cycle across seven modules, ten files over 1500 lines.

## Decision

Replace the workspace with one package: one `Cargo.toml`, one `src/`. Internal boundaries come
from four layers (`domain`, `infra`, `app`, `ui`) with `pub(crate)` by default and an
architecture test that fails CI on a forbidden import.

## Consequences

- Any edit recompiles the whole package. Build times are measured in phase 0 (see 0002) and a
  threshold is set there. If the edit-and-rerun loop exceeds it, `domain` and then
  `infra::ctrader` are extracted back into crates; the strict layers make that mechanical.
- The compiler no longer stops a bad import between layers. The architecture test does.
- Domain tests no longer build in isolation; filter at run time with `cargo nextest run domain::`.
