# Conventions

## Code

- Rust 2024, formatted with `cargo fmt`, linted with `cargo clippy --all-targets -- -D warnings`.
- Modules: `foo.rs` plus `foo/`. No `mod.rs`, no `#[path]`, no `use super::*` outside tests.
- Visibility: `pub(crate)` by default; `pub` for what integration tests and examples use.
- Errors: a `thiserror` enum per module. Never `Result<_, String>` for a new error.
- Types, not numbers: an enum where a protocol uses an integer code; convert at the boundary.
- Names: `*View` renders, `*State` is an entity of the app, `*Req`, `*Res` and `*Event` are wire
  types and live only in `infra::ctrader`.
- Tests sit in the module they test (`mod tests`); integration tests in `tests/`.
- Files stay under about 600 lines apart from data tables. Split by responsibility.

## Comments and docs

A comment says what the name and the type do not.

Keep:
- units, scales and invariants ("prices are in 1/100000", "sorted newest first");
- protocol quirks and why a workaround exists;
- why an order of operations or a lock matters;
- `# Errors` and `# Panics` sections on public functions that need them;
- one `//!` line per module saying what it owns and what it does not.

Remove:
- a sentence that repeats the name ("The close button." on `Close`);
- one-line docs on plain fields and constructors;
- banner comments (`// ---- x ----`);
- numbering or names that went stale.

`scripts/doc_noise.py` lists one-line docs that only repeat the name of the item. It prints
candidates for a person to review, it deletes nothing.

Where things live: the code says why locally; `docs/` holds architecture, recipes and protocol
notes; `README.md` says what the app is and how to run it; `AGENTS.md` holds rules for agents;
`docs/decisions/` holds decisions. Write a fact once and link to it.

## Writing style

See `AGENTS.md`: plain ASCII text, no dashes, arrows or curly quotes in code, comments, docs or
commit messages.

## Commits

One topic per commit, an imperative subject, no attribution trailer.
