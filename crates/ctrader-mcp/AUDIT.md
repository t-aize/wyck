# ctrader-mcp production readiness audit

Audit date: 2026-09-22

Audited commit: `0a8b2d90ce0470bd981e69976be15c6d6040fd12`

Crate version: `0.1.0`, unpublished
Target: a professional, typed, pure Rust wrapper for every cTrader Local and
Remote MCP tool. Trading policies, user confirmation, and business workflows belong
above this crate.

## Decision

**NO-GO for production trading as a complete SDK.**

The crate has a good Rust baseline. Formatting, linting, tests, documentation, release
build, and the RustSec scan all pass. The production code contains no explicit `unsafe`,
test-only panic paths do not leak into it, secrets use `SecretString`, read retries are
bounded, mutations are not automatically retried, and sessions have an explicit shutdown
path.

Those strengths do not close the production case. The current implementation:

- starts only the legacy MCP initialization lifecycle even though its `rmcp` dependency
  supports the current protocol lifecycle;
- does not provide typed request and response contracts for the full advertised cTrader
  surface;
- applies Remote rate limits at the wrong granularity and exposes a bypass;
- claims a post-amend safety check without performing the required server re-read;
- exposes unit conversions and workflows whose behavior or contract is unsafe for trading;
- has not been tested against either live cTrader server in this audit;
- has no long-running memory, connection churn, or resource leak evidence.

Production approval requires all P0 acceptance gates in the roadmap and successful live
validation against both Remote and Local demo servers.

## Scope and method

The audit covered:

- all production, test, example, and package files under `crates/ctrader-mcp`;
- the workspace dependency declarations and continuous integration workflow;
- MCP transport, lifecycle, tool discovery, result decoding, authentication, retries,
  cancellation, and shutdown behavior;
- Local and Remote DTO coverage, validation, unit handling, known server quirks, and
  mutation behavior;
- concurrency ownership patterns and likely leak sources;
- the current MCP specification, the official Rust SDK, MCP conformance tooling, and
  official cTrader MCP documentation.

This was a source audit, build audit, and test audit. It was not a live certification.
The environment had no `CTRADER_REMOTE_TOKEN`, `CTRADER_REMOTE_URL`,
`CTRADER_LOCAL_URL`, or `CTRADER_CONFIRM_DEMO` configuration, and no Local server was
listening on `127.0.0.1:9876`. No live schema was captured and no demo mutation was sent.

## Release gates

| Gate | Result | Evidence |
| --- | --- | --- |
| Rust formatting | Pass | `cargo fmt --all -- --check` |
| Clippy, all targets and features | Pass | `cargo clippy -p ctrader-mcp --all-targets --all-features -- -D warnings` |
| Unit and integration tests | Pass | 108 tests: 91 unit and 17 integration |
| Rust documentation warnings | Pass | `RUSTDOCFLAGS=-D warnings cargo doc -p ctrader-mcp --all-features --no-deps` |
| Locked release build | Pass | `cargo build -p ctrader-mcp --all-features --release --locked` |
| Known dependency vulnerabilities | Pass | 292 lockfile packages, 0 vulnerabilities and 0 warnings in RustSec database commit `eac8bd26b6593aafeb228d0f7adeb12c373e9f84` |
| Current MCP lifecycle | Fail | CTMCP-001 |
| Full typed cTrader surface | Fail | CTMCP-002 |
| Correct Remote throttling | Fail | CTMCP-003 |
| Trading mutation invariants | Fail | CTMCP-004, CTMCP-006, CTMCP-007 |
| Live Remote demo contract | Not run | Credentials and endpoint absent |
| Live Local demo contract | Not run | Local server absent |
| Memory and resource soak | Not run | No soak harness or live endpoint |

## Findings

### CTMCP-001: the transport always uses the legacy MCP lifecycle

Severity: High
Production gate: Fail

`McpSession::connect` calls `ClientIdentity.serve(transport)` in
[`transport.rs`](src/transport.rs#L84). In `rmcp` 3.4.0 that entry point selects the
legacy `Initialize` lifecycle. The dependency also offers `ClientLifecycleMode::Auto`
and `serve_with_lifecycle`, but this crate does not use them.

MCP 2026-07-28 uses per-request protocol metadata and does not use the old initialize
handshake. A client supporting current and older servers should discover the modern
mode and fall back to initialization. The dependency is current enough to do this; the
crate integration is not.

Impact:

- a current stateless MCP endpoint can reject the connection before any tool call;
- passing tests against the crate's mock server does not demonstrate current-protocol
  interoperability;
- the SDK cannot state support for the current MCP version.

Required action: use `ClientLifecycleMode::Auto`, add explicit compatibility tests for
2026-07-28 and 2025-11-25 or older servers, and record the negotiated lifecycle in
diagnostics.

### CTMCP-002: the SDK is neither complete nor fully typed

Severity: High
Production gate: Fail

The stated target is one typed method for every live cTrader tool. The implementation
does not meet it:

- Remote `get_server_time`, `amend_order`, and `cancel_order` return
  `serde_json::Value` instead of response DTOs. The two mutation cases are visible in
  [`remote/client.rs`](src/remote/client.rs#L274).
- Local template, workspace, watchlist, alert, plugin, and chart-object operations use
  raw JSON responses in many methods.
- official Local capability groups for opening and controlling charts, market news,
  indicator discovery, UI panels and tabs, application focus, and related desktop
  control have no complete typed wrapper surface here.
- `list_tool_names` throws away each tool's input and output schema and retains only its
  name in [`transport.rs`](src/transport.rs#L253). There is no runtime schema comparison
  to detect drift.

The exact live tool list is server-build-dependent, so source inspection alone cannot
name every missing method with certainty. That uncertainty is itself a release blocker.
The server's live `tools/list` response must be the inventory of record.

Impact:

- callers must inspect arbitrary JSON and reimplement validation;
- schema drift can remain invisible until a trade or desktop command fails;
- the crate cannot make the advertised claim that every documented tool is wrapped.

Required action: preserve full tool descriptors, capture `tools/list` from each supported
server build, maintain a checked compatibility manifest, and implement typed request and
response DTOs for every tool. Raw protocol access may remain as an explicitly unstable
escape hatch, but it must not substitute for SDK coverage.

### CTMCP-003: Remote rate limiting is incorrect and bypassable

Severity: High
Production gate: Fail

The Remote server contract distinguishes a general 50 requests per second limit from a
5 requests per second limit for historical calls such as `get_trendbars`,
`get_order_history`, and `get_deals`. The client instead applies one 5 requests per
second limiter to every call at [`remote/client.rs`](src/remote/client.rs#L13).

The limiter is acquired once for a logical read call, before the retry loop at
[`remote/client.rs`](src/remote/client.rs#L66). Each retry is therefore an additional
wire request without another permit. Concurrent failing reads can exceed the historical
limit. `RemoteClient::session()` also exposes unrestricted direct calls at line 86, which
bypasses all Remote throttling.

The limiter describes itself as fair, but it releases its mutex before sleeping. Waiting
tasks then race for the next token, and no test establishes fairness or freedom from
starvation.

Impact:

- historical workloads can receive avoidable 429 responses;
- all other calls are limited to one tenth of the documented throughput;
- callers can accidentally bypass the protection through the public session accessor.

Required action: classify tools into limit buckets, acquire a permit for every wire
attempt, keep raw calls behind the same policy, honor server retry metadata when present,
and add concurrent timing tests for both buckets and retries.

### CTMCP-004: the safe amend helper does not perform its documented post-flight read

Severity: High
Production gate: Fail

The Remote server has an omit-removes behavior for position protection legs. The helper
reads the current position, preserves both values, and sends both in the amendment. It
then validates only `result.position` if that optional field was embedded in the mutation
response. See [`quirks.rs`](src/quirks.rs#L39).

It does not call `get_position_details` after the mutation. When the response omits the
position object, the helper returns success without checking either leg. This contradicts
its own documentation and the client documentation, both of which require a post-flight
re-read.

Impact: an accepted amendment can silently leave a live position without its intended
stop loss or take profit while the SDK reports success.

Required action: always re-read the position after a successful amendment, verify the
position identifier and both legs, and return a distinct indeterminate-state error if the
read cannot confirm state. A demo live test must exercise both changing one leg and
changing both legs.

### CTMCP-005: Local lot conversion can turn a valid volume into zero

Severity: High
Production gate: Fail

The Local server has been observed reporting `lotSize: 1` while accepting fractional
volumes such as `0.01`. `lots_to_units` promises integer Local base-asset units and floors
`lots * lot_size` in [`math/units.rs`](src/math/units.rs#L13). With `0.01` lots and a lot
size of `1`, it returns `0`.

This also conflicts with the Local DTOs, which correctly model volume as `f64`. A helper
that yields zero for a valid minimum volume is unsafe to expose as the standard conversion.

Impact: callers can submit an invalid zero volume or size a trade incorrectly.

Required action: remove the universal integer assumption. Model Local and Remote volume
encodings as separate checked types derived from live symbol metadata. Add fixtures for
`lotSize: 1`, fractional volume steps, non-FX instruments, and Remote cent-volume
encoding. Since sizing policy is outside a pure wrapper, move lot-based sizing helpers to
an upper crate unless they only encode a wire contract.

### CTMCP-006: retry idempotence is promised but not implemented

Severity: High
Production gate: Fail

`RemoteSessionContext::idempotency_prefix` says the prefix lets a retry detect and skip a
duplicate order in [`workflows/bootstrap.rs`](src/workflows/bootstrap.rs#L49). The crate
only generates the random prefix at line 170. It has no lookup, reconciliation, duplicate
detection, or persisted operation key that implements the promise.

The client correctly avoids blind automatic retries for mutations. That is not sufficient
when a timeout occurs after the server may have accepted a request. The outcome remains
unknown and the advertised duplicate protection does not exist.

Impact: an upper layer relying on the documented guarantee can place a duplicate trade.

Required action: delete the guarantee from this crate. Expose enough typed identifiers and
labels for an upper layer to reconcile an uncertain result. If an idempotency facility is
kept, define its persistence and lookup rules and prove them with timeout-after-acceptance
tests.

### CTMCP-007: `safe_flatten` can report success after skipping open items

Severity: High
Production gate: Fail while the workflow remains public

`safe_flatten` silently skips a position without both `position_id` and `volume`, and an
order without `order_id`, at [`safe_flatten.rs`](src/workflows/safe_flatten.rs#L46).
`FlattenReport::fully_flattened` only checks whether `errors` is empty at line 20.

Impact: a malformed or drifted response can leave exposure or an order open while
`fully_flattened()` returns `true`.

Required action: move this business workflow out of the pure wrapper. Until then, record
every skipped item as an error, perform a final server re-read, and define success only as
an empty post-flight position and order set within the requested scope.

### CTMCP-008: unchecked numeric conversions can serialize corrupt values

Severity: Medium
Production gate: Fail for public trading helpers

`money_to_raw` and `price_to_pipettes` return `i64` directly in
[`common.rs`](src/common.rs#L198). They do not reject NaN, infinity, negative values where
invalid, or values outside the integer range. Rust float-to-integer conversion saturates
or produces zero for these cases instead of returning an error. `math::units` re-exports
the same behavior, creating a second public route with the same problem.

Impact: invalid user or market data can become a plausible but wrong wire integer.

Required action: use one checked conversion API returning `Result`, define rounding per
wire field, and remove the duplicate public entry points.

### CTMCP-009: several public validation contracts do not match behavior

Severity: Medium
Production gate: Fail before public SDK release

Examples:

- Remote `CreateOrderParams` rejects negative `slippageInPoints` but accepts zero at
  [`remote/dto.rs`](src/remote/dto.rs#L705), while the server field is positive.
- `AmendOrderParams` does not apply the same complete validation discipline as order
  creation, including identifiers and slippage-related fields.
- Local `close_position_partial` says it rounds down to `volumeStep`, but it only checks
  that volume is positive and sends it unchanged in
  [`local/client.rs`](src/local/client.rs#L252).
- Remote symbol-list calls document symbol validation, but no live symbol cache guard is
  applied before sending the request.

Impact: the public Rust API promises rejection or normalization that does not happen.
Callers can receive server errors or believe a different volume was submitted.

Required action: make documentation and behavior identical. Prefer transparent typed
wire validation in the core SDK. Put risk-reducing rounding or symbol-selection policy in
the upper layer.

### CTMCP-010: server identity and profile checks are too weak

Severity: Medium

`RemoteClient::new` and `LocalClient::new` accept any `McpSession` without checking the
server family. `has_trading_profile` treats the presence of `create_order` alone as proof
of the trading profile at [`remote/client.rs`](src/remote/client.rs#L103). It does not
verify the complete expected mutation set or schema.

Impact: a wrong endpoint or partial server deployment is discovered only during later
calls. A partially exposed mutation surface can be reported as a valid trading profile.

Required action: validate a versioned capability manifest during connection and return a
structured compatibility report containing server family, build, protocol lifecycle,
missing tools, extra tools, and schema mismatches.

### CTMCP-011: result decoding is tolerant but not contract-checked

Severity: Medium

The decoder accepts structured content or JSON from the first text content block. DTOs
also use many optional fields and flattened extras. This helps tolerate additions, but it
does not validate advertised output schemas and can allow a malformed successful response
to travel far into trading code. Local plain-text error recognition is narrow and may not
classify non-order errors correctly.

Impact: schema drift can look like missing optional data rather than a compatibility
failure, as demonstrated by the `safe_flatten` behavior.

Required action: retain advertised output schemas, validate required fields at the DTO
boundary, preserve all content blocks for diagnostics, and add fixtures for structured,
text, error, and malformed results.

### CTMCP-012: no memory leak is visible, but leak freedom is unproven

Severity: Medium evidence gap

No crate-owned production `tokio::spawn`, explicit `unsafe`, obvious `Arc` cycle, or
unbounded collection was found. The rate limiter has one bounded mutex-protected state.
`McpSession` owns the `rmcp` running service and offers explicit shutdown. These are good
signs, not proof.

There is no long-running test that repeatedly connects, lists tools, performs calls,
cancels work, shuts down, and observes resident memory, task count, sockets, and handles.
The existing mock tests cannot detect leaks tied to a real Local process, Remote HTTP
behavior, TLS, or connection churn.

Required action: add a same-process mock soak to CI and scheduled live demo soaks for both
server families. Set explicit thresholds for resident memory slope, open handles, tasks,
and shutdown time. Investigate any monotonic growth with an allocator or OS profiler.

### CTMCP-013: the crate boundary does not match a pure wrapper SDK

Severity: Medium architecture issue

The crate publicly contains `math`, `quirks`, and `workflows`, and its package description
advertises composable trading workflows. Some wire quirks belong at the protocol boundary,
but position sizing, safe flattening, backfills, confirmation rules, and session workflows
are application policy.

Impact: the core SDK carries policy, duplicated conversion APIs, and safety claims that are
harder to version than the transport contract.

Required action: keep transport, discovery, typed DTOs, checked wire encoding, error
mapping, timeouts, cancellation, and server-contract quirks in `ctrader-mcp`. Move trading
workflows and sizing policy into a companion crate or application layer.

## MCP compliance assessment

| Area | Status | Assessment |
| --- | --- | --- |
| Streamable HTTP | Partial | Uses the official Rust SDK transport, but lifecycle selection is legacy-only. |
| Protocol version compatibility | Fail | No Auto or Discover lifecycle and no compatibility matrix tests. |
| Tool discovery | Partial | Pagination is delegated to `rmcp`, but schemas are discarded. |
| Tool calls | Partial | Typed calls exist for much of the surface; raw JSON and missing categories remain. |
| Authentication | Pass with live test pending | Bearer handling uses secret storage and has a header regression test. Deployment TLS and token rejection were not tested live. |
| Cancellation and timeout | Partial | Timeouts and service shutdown exist; cancellation under live in-flight requests is not certified. |
| Error reporting | Partial | Structured crate errors exist, but schema and Local plain-text error coverage are incomplete. |
| Conformance suite | Not run | The crate has no adapter or job for the official MCP conformance suite. Upstream `rmcp` conformance does not certify this crate's lifecycle choices and decoding. |

The official Rust SDK reports support for MCP 2026-07-28 and earlier versions. Staying on
`rmcp` 3.4.0 is reasonable; using its current lifecycle APIs is required.

## cTrader SDK assessment

### What is already good

- Local and Remote are separate client types, reducing accidental cross-server calls.
- Mutating methods do not use the automatic read retry path.
- Connection timeout, call timeout, retry limits, and backoff are configurable.
- Errors retain transport, server, decode, timeout, pre-flight, and invariant context.
- Sensitive bearer data uses `SecretString`, and tests cover header formatting and debug
  redaction.
- DTO flattening preserves unknown fields, which is useful for diagnostics during server
  evolution.
- Tests cover many known encoding rules and server quirks.
- CI runs on Ubuntu and Windows with warnings denied for Clippy.

### What prevents calling it a good production SDK today

A production SDK needs a stable and honest public contract. Here, some APIs return raw
JSON, some documented guarantees are not implemented, live schemas are not compared, and
the package mixes wire wrapping with application policy. The API can be a useful preview
or internal integration layer, but version `0.1.0` and `publish = false` correctly reflect
that it is not a released complete SDK.

## Test and CI gaps

Current CI provides a sound compile and unit baseline but lacks:

- dependency audit or policy enforcement in CI;
- code coverage thresholds for request and response DTOs;
- tests against modern and legacy MCP lifecycle modes;
- a generated contract test for every live tool and schema;
- timeout-after-server-acceptance mutation tests;
- concurrent rate-limit and fairness tests;
- cancellation and shutdown-under-load tests;
- connection churn and memory soak tests;
- scheduled Remote and Local demo integration tests;
- official MCP conformance execution or an equivalent client adapter.

The dependency tree contains normal transitive version duplication. No security problem
was reported by RustSec. Dependency count alone is not evidence of a leak.

## Demo test environment

Add an opt-in local environment loader for examples and live integration tests. Use
`dotenvy` as a development dependency, not as a runtime dependency of the SDK. The core
library must continue to receive configuration through `ConnectionConfig` and must never
read `.env` implicitly. This keeps production configuration explicit and avoids surprising
secret lookup in applications that embed the crate.

The repository currently ignores both `.env` and `.env.*`. Keep those rules and add
`!.env.example` so the documented template can be committed. Never commit `.env`, tokens,
account identifiers, captured authorization headers, or live test output containing them.
CI secrets must come from the CI secret store, not from an environment file.

The committed `crates/ctrader-mcp/.env.example` must retain complete comments and safe
defaults matching this contract:

```dotenv
# Copy this file to crates/ctrader-mcp/.env for local demo testing.
# Never commit .env. Never put a real-money token in this file.

# Remote MCP endpoint. Keep the official demo endpoint or use a controlled test proxy.
CTRADER_REMOTE_URL=https://mcp.ctrader.com/trading/mcp

# Bearer token for a DEMO account only. Leave empty in the committed example.
# Obtain and rotate the token through the official cTrader authorization flow.
# Treat this value as a password. Do not print it or include it in test snapshots.
CTRADER_REMOTE_TOKEN=

# Local MCP endpoint exposed by cTrader Desktop with a DEMO account selected.
# The trailing slash is intentional.
CTRADER_LOCAL_URL=http://127.0.0.1:9876/mcp/

# Destructive live tests are disabled unless this exact value is 1.
# Set it only after verifying that every configured account is a demo account.
# Tests must still use minimum volume, re-read state, and clean up created artifacts.
CTRADER_CONFIRM_DEMO=0

# Optional account allowlist for live tests. The harness must refuse mutations when the
# active account does not match this value. Use the stable account identifier returned by
# the target server. Leave empty for read-only tests.
CTRADER_DEMO_ACCOUNT_ID=

# Enable live tests explicitly. Ordinary cargo test runs must remain offline and hermetic.
CTRADER_RUN_LIVE_TESTS=0
```

The test harness must load this file only from live examples or tests, using an explicit
call such as `dotenvy::from_path`. Missing `.env` must skip live tests with a clear reason,
not fail normal unit tests. Before any mutation it must require all of the following:

- `CTRADER_RUN_LIVE_TESTS=1`;
- `CTRADER_CONFIRM_DEMO=1`;
- a non-empty account allowlist that matches the server's active account;
- successful server-family and trading-profile detection;
- an explicit cleanup plan and a post-mutation server re-read.

Secret values must use `SecretString` after loading. Errors and debug output must name a
missing variable without echoing its value. Add tests proving `.env` is ignored, the
example contains no token, mutation defaults are off, account mismatch is rejected, and
token values do not appear in logs or error chains.

## Roadmap to production

### P0: correctness and contract

1. Select `ClientLifecycleMode::Auto` and test both modern stateless discovery and legacy
   initialization.
2. Capture full `tools/list` descriptors from current Remote data, Remote trading, and
   Local demo servers. Store sanitized fixtures with server build metadata.
3. Create a capability manifest and typed DTOs for every advertised tool. Remove raw JSON
   from the stable typed API.
4. Split Remote throttling into general and historical buckets, acquire per wire attempt,
   and close the raw-session bypass.
5. Fix `amend_position_preserving_legs` to perform and validate the post-flight read.
6. Replace universal volume conversion with server-specific checked wire types. Make all
   numeric conversions fallible.
7. Remove false idempotency and rounding guarantees. Expose indeterminate mutation state
   explicitly.
8. Move workflows and sizing policy out of the pure wrapper. Until extraction, fix
   `safe_flatten` so skipped items and post-flight exposure make it fail.
9. Keep the documented `.env.example` and `.gitignore` exception, then add the
   `dotenvy`-based live test bootstrap described above. Keep environment loading outside
   the library API.

P0 acceptance:

- every live tool appears exactly once in the manifest and has typed request and response
  coverage;
- no stable wrapper method returns `serde_json::Value`;
- modern and legacy lifecycle contract tests pass;
- all mutation invariants have timeout, malformed-response, and post-flight tests;
- all current commands in the release-gate table pass.

### P1: live certification and operations

1. Run read-only contract tests on Remote data, Remote trading, and Local demo endpoints.
2. With explicit demo-only confirmation, run create, amend, partial close, full close,
   cancel, alert, watchlist, chart, template, workspace, and plugin mutation cases that the
   endpoint advertises.
3. Verify server state after every mutation. Clean up each test artifact and fail the run
   if cleanup is incomplete.
4. Run the official MCP conformance suite through a client adapter where applicable.
5. Add a one-hour mock soak and scheduled multi-hour live connection-churn soaks. Record
   memory, handles, sockets, rate-limit responses, latency, retries, and shutdown time.
6. Publish a compatibility table keyed by crate version, `rmcp` version, MCP version,
   cTrader server family, and observed server build.
7. Run live jobs only from protected CI environments with demo-only secrets, account
   allowlisting, concurrency control, short token lifetime, and guaranteed cleanup.

P1 acceptance:

- zero unexpected schema differences on every supported profile;
- no unexplained 429 response at supported concurrency;
- every demo mutation is confirmed by a post-flight read and cleaned up;
- no statistically meaningful upward memory or handle slope after warm-up;
- in-flight cancellation and shutdown complete within documented bounds.

### P2: public SDK release discipline

1. Add RustSec auditing and dependency policy checks to CI.
2. Add coverage reporting, API compatibility checks, and scheduled tool-schema drift jobs.
3. Define semantic versioning rules for DTO additions, server drift, and unstable raw
   access.
4. Add a changelog, supported-server table, deprecation policy, minimal examples, and an
   operations guide for timeouts, rate limits, and uncertain mutation outcomes.
5. Publish only after the public API review and P0/P1 evidence are complete.

## Live validation checklist

The following evidence is still required. It must run only on demo accounts.

Remote:

- capture data-profile and trading-profile `tools/list` with full schemas;
- record version and build-time tools;
- verify 50 requests per second general and 5 requests per second historical scheduling;
- create and cancel a pending order, create and close a small position, and amend both
  protection legs with a post-flight read;
- force or simulate timeout-after-acceptance and reconcile by label and server state;
- verify 401, 403, 429, timeout, disconnect, malformed result, and shutdown behavior.

Local:

- capture full `tools/list` with schemas and server build;
- test fractional volume with the live `lotSize` and `volumeStep` values;
- exercise every advertised typed desktop and trading tool;
- verify pending-order stop-loss and take-profit units independently;
- confirm cleanup of charts, objects, alerts, watchlists, and other created artifacts;
- verify behavior when the desktop process exits during a call and restarts.

## Sources

Primary protocol and SDK sources:

- [MCP 2026-07-28 transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports)
- [MCP protocol versioning](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)
- [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
- [MCP authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)
- [Official MCP Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [Official MCP conformance suite](https://github.com/modelcontextprotocol/conformance)

Primary cTrader sources:

- [cTrader AI Agent Connect](https://help.ctrader.com/ctrader-ai-agent-connect/)
- [cTrader Local MCP](https://help.ctrader.com/ctrader-ai-agent-connect/local-mcp/)
- [cTrader Remote MCP setup](https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/setup/)

Repository evidence:

- [`README.md`](README.md)
- [`src/transport.rs`](src/transport.rs)
- [`src/remote/client.rs`](src/remote/client.rs)
- [`src/local/client.rs`](src/local/client.rs)
- [`src/quirks.rs`](src/quirks.rs)
- [`src/math/units.rs`](src/math/units.rs)
- [`src/workflows/bootstrap.rs`](src/workflows/bootstrap.rs)
- [`src/workflows/safe_flatten.rs`](src/workflows/safe_flatten.rs)
- [`../../.github/workflows/ci.yml`](../../.github/workflows/ci.yml)

## Final assessment

This crate is a promising internal preview with a clean compile and test baseline. It is
not ready to be the sole production boundary for live trading. The fastest credible path
is to narrow it to a pure protocol wrapper, make the live schema the contract of record,
fix the lifecycle and trading correctness findings, and then collect demo evidence from
both cTrader server families before approving real-money use.
