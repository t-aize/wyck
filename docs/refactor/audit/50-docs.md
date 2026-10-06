# Documentation audit

## Markdown files (root and crates)

| File | Verdict | Why |
|---|---|---|
| `AGENTS.md` | Rewrite (extend) | Only git attribution and writing style; add commands, layers, hard rules, traps. Keep both existing sections |
| `CLAUDE.md` | Keep, extend | One line `@AGENTS.md`; add a short Claude Code section |
| `README.md` | Keep, review later | Product readme (7.4 KB); check against the final architecture in M8 |
| `CONTRIBUTING.md` | Rewrite | "Issues only", empty code style section; point to `AGENTS.md` and `docs/CONVENTIONS.md` |
| `RELEASING.md`, `SECURITY.md`, `SUPPORT.md`, `CODE_OF_CONDUCT.md` | Keep | Out of scope |
| `crates/wyck-config/README.md` | Keep | Already exists; trim to 15 lines in M8 |
| Other crates | Add README | At most 15 lines: role, main API, what must not go in |
| `docs/` | Create | Per `PLAN.md` section 4.7 |

Not inspected in this pass: `.github/ISSUE_TEMPLATE/*`, `crates/wyck/README` (none found at root of the crate).

## Doc comments and comments

Doc plus comment lines per code line: app 6.2 %, chart 7.5 %, ui 13.3 %, openapi src about 30 %, config src about 28 %. Raw `//` line counts: wyck 3110, chart 2448, config 1249, openapi 2983, ui 643.

Delete (restate the code or signature):

- `wyck-openapi/src/transport/wire.rs:31-218`: about 80 lines like `/// ProtoOAApplicationAuthReq.` above a constant with the same name.
- `transport/messages.rs:19-22` (`/// The client id.`), `handle.rs:50-53` (`/// The account id.`), `session/mod.rs:346-350`, `connection.rs:306-310` (`/// Whether the connection has ended.`), `market/bars.rs:18-45` (`/// One minute.`), `rate_limit.rs:46-50`.
- `wyck-chart/src/data.rs:35,51,56`, `study/mod.rs:1079,1217`, `wyck-ui/src/tokens.rs:28`, `theme.rs:76`.
- `wyck/src/trading/account.rs:373,997,1327`, section separators `// ---- loading ----` (`account.rs:601,786,884`), `chart/export_ui.rs:63` (duplicated at `parts.rs:35`).
- The comment about "MCP servers" in `bars.rs:15` (refers to nothing in the repo).

Keep (explain a non obvious reason):

- `wyck-openapi/src/transport/connection.rs:444-445` (re-check `is_closed` after registering a waiter), `session/mod.rs:684-686` (token refresh is not cancellation safe), `config.rs:63-66` (why 4 req/s for history), `auth/oauth.rs:22-24` (secret in query string).
- `wyck/src/trading/account.rs:902-903` (no answer in time, button freed, account re-read), `:1373-1374` (close first, then reverse), `guard.rs:5-7` (single funnel), `runtime.rs:25-29` (why the runtime is leaked).
- `wyck-chart/src/drawing/position.rs:219-220` (round down so risk is not exceeded), `wyck-ui/src/theme.rs:122` (poisoned lock on a `Copy` value).
- `wyck-openapi/src/lib.rs` crate doc (180 lines): dense but useful (server limits, tick delta encoding, what was verified live). Keep, trim during T-021.

Add (one sentence): `Timeframe`, `Series`, `InputSpec`, `PlotSpec`, `AtrStop`, `Smoothing`, export option enums.

## Policy

Target from `PLAN.md` section 4.5: comments explain why, not what; doc comments only on useful public API, one sentence; no commented-out code; no `# Examples` filler; no generalized `#![deny(missing_docs)]`.
