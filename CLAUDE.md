@AGENTS.md

## Claude Code

- Use plan mode before changing `crates/wyck-openapi/src/session/`, `crates/wyck-openapi/src/transport/`,
  the order path in `crates/wyck/src/trading/account.rs`, or the layer rules.
- Use subagents for read-only exploration across crates; keep the main context for edits.
- Before saying a task is done, run `cargo xtask check` and report its result.
