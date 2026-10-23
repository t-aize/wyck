---
name: new-crate
description: Add a crate to the Wyck workspace. Use when the user asks for a new crate, or when code needs a home that no crate under crates/ fits.
---

# Add a crate

A new crate is rare: first check that no existing crate owns the job (`AGENTS.md` at the root has
the map). If one does, put the code there.

1. **Decide its place.** Which crates may it use, and which will use it? The arrows go down only
   (see the graph in the root `AGENTS.md`).
2. **Layering first.** Add a `Layer` for it in `LAYERS` in `xtask/src/deps.rs` (the workspace crates
   it may use, the external crates it must not). If it owns an external crate, add that to `OWNERS`.
3. **Manifest.** Create `crates/wyck-<name>/Cargo.toml` like `crates/wyck-clock/Cargo.toml`:
   package fields as `x.workspace = true`, a one-line `description`, dependencies as
   `name.workspace = true`, and
   ```toml
   [lints]
   workspace = true
   ```
4. **Root.** Add `wyck-<name> = { path = "crates/wyck-<name>" }` to `[workspace.dependencies]` of the
   root `Cargo.toml`, in alphabetical order. `members = ["crates/*", "xtask"]` already finds it.
5. **Source.** `src/lib.rs` starts with a `//!` of one to three lines (role and invariants). Modules
   are private; re-export what other crates need.
6. **Docs.** `AGENTS.md` (copy the sections of a neighbour: a title `# wyck-<name>`, a role, `Depends
   on:`, `Used by:`, `## Module map`, `## Invariants and traps`, `## Tests`, `## Where to add X`,
   `## Do not`) and `CLAUDE.md` that starts with `@AGENTS.md` and says which commands to run before
   saying "done". Add the crate to the table and the graph of the root `AGENTS.md`, and to the
   crates table of `README.md`. The `Used by:` line of every crate it depends on changes too.
7. **Check.** `cargo xtask check --full`. `deps` and `docs-check` say what is missing.
