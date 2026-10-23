# xtask

The developer commands of the workspace, run as `cargo xtask <command>` (the alias lives in
`.cargo/config.toml`). Plain Rust, so they work the same on Windows, macOS and Linux with no `make`,
shell or Python to install; CI runs the same commands, so what passes here passes there.

Depends on: `wyck-clock`, `anyhow`, `chrono`, `clap`, `percent-encoding`, `semver`, `serde`,
`serde_json`, `sha2`, `toml` (`tempfile` in tests only).
Used by: nobody; it is a tool, not a part of the app.

## Module map

| Module | What it holds |
|---|---|
| `main` | The command line (`check`, `ci`, `deps`, `docs-check`, `release`). |
| `run` | Running steps of a command, timing them, skipping a tool that is not installed, and the summary. |
| `ci` | What CI runs: format, clippy, tests, cargo-deny, rustdoc, all strict. |
| `check` | `ci` plus the rules of the workspace; with `--full` the tools that need installing. |
| `deps` | The `LAYERS` table: which crate may depend on which, what is forbidden, who owns an external crate. |
| `sources` | `deps` on the `.rs` files: `MODULES` (which module of a crate may name which) and `OWNERS` (the files that may name an external crate). |
| `docs` | `docs-check`: the `AGENTS.md` and `CLAUDE.md` files against `Cargo.toml`, and the Markdown links. |
| `release` | `name-assets` and `manifest`, the steps of `release.yml` written in Rust. |

## Invariants and traps

- A new crate is added to `LAYERS` in `deps.rs` first, and gets its `AGENTS.md` and `CLAUDE.md`.
- `deps` and `docs` read the manifests as TOML; they do not call `cargo metadata`, so they run in a
  second.
- `sources` is textual: it cuts `//` comments and reads `crate::module` paths, nothing more. A
  module reaches a sibling with `crate::`, never `super::super::`. When crates are merged into
  modules, the old dependency chain is written into `MODULES` and the old crate owners into `OWNERS`
  in the same change as the move.
- `release` names the files that `release.yml` uploads and signs: a change to a name is a change to
  the update manifest the installed apps read. Keep the platform keys as they are.
- The commands run child processes without the `CARGO_PKG_*` variables of this package: some tools
  (cargo-machete) would otherwise look at `xtask` instead of the workspace.

## Tests

Unit tests next to the code (the layering rules, the Markdown link reader, the release naming).
Run: `cargo test -p xtask`.

## Where to add X

- A check of the workspace: a module, a `Command` in `main.rs`, and a step in `check.rs` if it
  should run with everything.
- A tool that needs installing: `external_tools` in `check.rs`, with the command that installs it.

## Do not

- Do not depend on a crate of the workspace other than `wyck-clock`: this is a tool, not part of the
  app.
- Do not print secrets: the release steps read signatures, never keys.
