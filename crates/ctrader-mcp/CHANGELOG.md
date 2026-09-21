# Changelog

The crate follows the workspace version. This file also records live checks of
server behavior, which can change without a crate release.

## Unreleased

### Fixed

- Redact the bearer token in `ConnectionConfig` debug output.
- Recover whole units and cents lost to floating-point drift when converting lots.
- Retry read-only Local raw getters after transient failures, and apply the Remote
  request limiter to tool listing.

### Added

- Mock MCP tests for history backfill, safe position amendment, two-step market
  entry, Remote bootstrap, and Local raw getter retries.
- A crate README and a current module map.

### Verification record

- 2026-09-21: `cargo build --workspace`, `cargo test --workspace`, and
  `cargo clippy -p ctrader-mcp --all-features` passed.
- 2026-09-21: Mock MCP integration tests passed. A live Remote probe could not run
  because `CTRADER_REMOTE_TOKEN` was absent. A live Local probe could not run because
  no server answered on `127.0.0.1:9876` and `CTRADER_LOCAL_URL` was absent. MCP
  session header behavior and non-order Local error prefixes remain unverified.
  The 2026-07-28 MCP specification removes the initialization handshake and
  `Mcp-Session-Id`; this crate's transport still uses the earlier protocol flow.
- 2026-09: Existing source observations in `remote/dto.rs`, `quirks.rs`, and
  `time.rs` record selected Remote wire behavior, including ISO trendbar bounds and
  a 100-bar response with `hasMore: false`. These were not rechecked on 2026-09-21.
- 2026-05-14: The `ctrader-mcp-servers` skill records its last full audit against
  Remote `rest-proxy 1.0.18` and a Local build observed that day.

When both servers are available, run `probe_remote` and `probe_local` and record
the handshake, `Mcp-Session-Id` header behavior, `tools/list`, build identifiers,
and relevant error prefixes here. A transport change toward a stateless MCP server
needs a separate implementation review. Do not infer Local mutation error prefixes
from the `Order error:` placement case.
