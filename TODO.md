# wyck: TODO

The full working plan, as of 2026-09-19. It is written to be usable cold, in a fresh
conversation: every item says what to do, why it matters, where the code lives, and how to
know it is done. Layout and data flows are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md);
the rules for writing in this repository are in [AGENTS.md](AGENTS.md).

Contents

1. [Where things stand](#1-where-things-stand)
2. [Working rules](#2-working-rules)
   - 2A. [URGENT: the cTrader Open API](#2a-urgent-the-ctrader-open-api-real-ticks-seconds-one-connection-choice)
3. [P0: live validation before any real order](#3-p0-live-validation-before-any-real-order)
4. [P1: decisions and foundations](#4-p1-decisions-and-foundations)
5. [P2: wyck-engine, what is left](#5-p2-wyck-engine-what-is-left)
6. [P3: the GUI](#6-p3-the-gui)
7. [P4: other crates](#7-p4-other-crates)
8. [Repository and infrastructure](#8-repository-and-infrastructure)
9. [Assumptions to confirm](#9-assumptions-to-confirm)
10. [Risks and open questions](#10-risks-and-open-questions)
11. [v1 vision](#11-v1-vision)
12. [Reference](#12-reference)

---

## 1. Where things stand

| Crate | State | Notes |
|---|---|---|
| `ctrader-mcp` | Complete for the documented tools | Remote read and order paths verified live on a demo account (order, amend, close, cancel). Remote position, order and deal prices are display decimals, not pipettes (fixed). Local read and order paths verified on a demo account. Has a `test-support` feature exposing the mock MCP server, and a `raw_call` example for looking at raw answers. |
| `wyck-config` | Complete, tested | Profiles in TOML, tokens in the OS keyring or an encrypted file. |
| `wyck-calendar` | Complete, tested | ForexFactory weekly feed: client, tolerant parser, filters, refresh service, alerts. Tuned to the feed's real rate limit. |
| `wyck-engine` | Validated live on Remote and Local (demo accounts) | Session, state and events, risk planning, dry-run-first order pipeline, guardrails, news hosting, Remote and Local adapters, `MockBroker`. Section 3 has the results and what is still open. |
| `wyck-app` | Application layer, GPUI shell, connection screens | `cargo run` opens a window with a custom title bar and the connection flow (local session, token, saved account), wired to the engine, with global hotkeys that plan dry-run orders. The trading views come next. The ratatui TUI was removed (`crates/wyck`, in git history before commit `9f5a8ba`). |

Other facts:

- Everything lives on `main`. CI (fmt, clippy with warnings denied, tests, docs with
  warnings denied, release build with `--locked`) runs on every push, on ubuntu and windows.
  The runs for `a1a0623` and `53cb4aa` were cancelled by later pushes and never finished:
  check `gh run list --branch main` first.
- Test suites: `cargo test --all-features` runs everything. The engine has unit tests, a
  broker contract suite (mock and real Remote adapter against a scripted MCP server that
  mimics the real answers), 29 paused-time scenario tests, a news integration test and
  doctests. The live tests (`live_remote.rs`, `live_local.rs`) are `#[ignore]`d and need a
  demo account, see 3.1.
- The engine has an example that needs no network:
  `cargo run -p wyck-engine --example dry_run_order --features testing`.

---

## 2. Working rules

From `AGENTS.md`, restated because they are easy to forget:

- Commit and push straight to `main`. No feature branches, no pull requests unless asked.
- No AI attribution in commit messages or PR descriptions (no `Co-Authored-By` line).
- Repository text stays plain ASCII (section sign excepted): no em or en dashes, arrows,
  ellipsis characters, curly quotes, emoji. Reword instead of swapping characters. The check
  command is at the bottom of `AGENTS.md`.

Before every commit:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps            # default features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo build --release --locked
```

Docs must build with and without `--all-features` (the `testing` feature hides
`MockBroker`; never link to feature-gated items with intra-doc links).

---

## 2A. URGENT: the cTrader Open API (real ticks, seconds, one connection choice)

Researched on 2026-09-20 from the official documentation and forum. Nothing of this is built yet.
It is placed before P0 on purpose: it decides what data the chart can ever show below one minute,
and it changes the connection screen and the engine boundary, so it is cheaper to plan now than
after more UI depends on the MCP-only shape.

### 2A.1 Why

The two MCP servers cannot give sub-minute data. `get_trendbars` starts at one minute (M1 to MN1,
nine periods), `get_spot_prices` is a snapshot the engine polls once a second, and neither has a
tick history, a tick stream, or a per-tick volume. So today: no tick chart, no 1s or 5s bars, and
the last bar is built from one price per second (see `docs/ARCHITECTURE.md`, "The price chart").

The Open API is the protocol cTrader itself offers for this. Goal: a third way to connect, next to
"MCP Local" and "MCP Remote", where the user enters their own application credentials (client id,
client secret, callback address) and signs in with their cTrader ID. The engine then gets live
ticks and tick history, records them, and the chart aggregates them into any time bar or tick bar.

### 2A.2 What the research found

| Topic | Finding |
|---|---|
| Access | Anyone with a cTID on a cTrader-affiliated broker. The application must be registered at `openapi.ctrader.com`; it starts as "submitted" and Spotware approves it by email, so this can take time. No fee is mentioned. |
| Endpoints | `live.ctraderapi.com` and `demo.ctraderapi.com`. Port `5035` is Protobuf, port `5036` is JSON. Both accept TCP (with TLS) and WebSocket. Demo and live are separate: one connection per environment, accounts cannot be mixed. |
| Framing | Protobuf over TCP: a 4 byte big endian length, then a `ProtoMessage` envelope (`payloadType`, `payload`, optional `clientMsgId` to match replies). WebSocket needs no length prefix. JSON uses the same envelope as `{clientMsgId, payloadType, payload}`. |
| Heartbeat | Send a heartbeat at least every 10 seconds or the connection is dropped. |
| Limits | 50 requests per second per connection, 5 per second for historical requests. Over it: `REQUEST_FREQUENCY_EXCEEDED`. Limits are per connection, not per user. No documented limit on subscribed symbols. |
| Sign in | OAuth 2 authorization code. Register a redirect URI (a loopback such as `http://localhost:8765` is the documented way for a desktop app). The user grants a scope (`accounts` is read only, `trading` is full). The code lives one minute, is exchanged at `openapi.ctrader.com/apps/token` for an access token (about 30 days) and a refresh token (no expiry until used). Then: `ProtoOAApplicationAuthReq` (client id and secret), `ProtoOAGetAccountListByAccessTokenReq`, `ProtoOAAccountAuthReq` per account. The default redirect URI of the portal is for its playground only. |
| Live ticks | `ProtoOASubscribeSpotsReq` then `ProtoOASpotEvent` on every bid or ask change (fields `bid`, `ask`, `trendbar[]`, `sessionClose`, `timestamp`). A spot event may carry only one side. |
| Tick history | `ProtoOAGetTickDataReq`: a symbol, a quote type (bid or ask, so two requests for both), a range of at most one week (604 800 000 ms), a per-response cap that depends on the broker's backend, and `hasMore`. Ticks come **newest first** and their timestamps are **deltas** (the first is absolute, each next one is added), which is a known trap. Prices are integers to divide by 100 000. The docs do not say how long a broker keeps ticks. |
| Volume | A historical tick is a price and a time only. There is no per-tick volume. A trendbar has a volume, and `ProtoOASubscribeDepthQuotesReq` gives the order book with sizes (divide by 100). So "volume" for tick charts can only be a tick count, and real footprint style volume is not available. |
| Bars | `ProtoOAGetTrendbarsReq` with a per-period maximum range, `hasMore`, and live bars via `ProtoOASubscribeLiveTrendbarReq` (needs the spot subscription). A forum report shows silent gaps when many maximum size requests are chained: use small windows and check the seams (we hit a related server behavior on MCP, see 3.8). |
| SDKs | Official: C# (`OpenAPI.Net`) and Python (`OpenApiPy`). No official Rust. `ctrader-rs` on crates.io is a young 1K line client (three releases), useful as a reference, not to depend on. Proto files: `github.com/spotware/openapi-proto-messages`. |

Sources: help.ctrader.com/open-api (getting started, messages, model messages, symbol data,
account authentication, register an application, proxies and endpoints, protobuf and JSON, FAQ),
the forum threads on tick data timestamps (37490) and missing trendbars (41452), and the
`ctrader-rs` page on lib.rs.

### 2A.3 Decisions to take first

- [x] **A new crate, `ctrader-openapi`** (built, see `crates/ctrader-openapi`), beside `ctrader-mcp`. It shares nothing with rmcp, and
      keeping it separate keeps `ctrader-mcp` complete and small. Keep it simple: a connection, the
      sign in flow, and typed calls for what the app uses (symbols, spots, trendbars, tick data,
      depth). No trading calls in the first version.
- [x] **JSON on port 5036 first, Protobuf later if it matters.** JSON needs no `protoc`, no code
      generation and no vendored `.proto` files (`serde_json` plus a WebSocket client), so it is the
      simplest thing that works. Hide it behind a small trait so a Protobuf transport (with `prost`
      and a pure Rust generator such as `protox`) can replace it without touching callers. Revisit
      if tick volume makes JSON too slow.
- [x] **TLS backend**: `rmcp` is pinned to `native-tls` (see `docs/gpui-dependency.md` and the
      workspace notes: do not swap it). Use the same for the WebSocket client so the build does not
      carry two TLS stacks.
- [ ] **Where it plugs in**: a new `MarketData` port in `wyck-engine` (ticks, tick history, bars),
      separate from the `Broker` port, so a session can trade over MCP and read data over the Open
      API, or read data only. `ServiceKind` gets a third value and `ConnectRequest` an Open API
      variant. Decide whether one session may hold both.
- [ ] **Scope**: ask for `accounts` (read only) first. Trading over the Open API is a later,
      separate item, because it would need the whole order pipeline and its safety rules ported.

### 2A.4 What the user has to do (cannot be done in code)

- [ ] Register an application at `https://openapi.ctrader.com/`, with a full description (it speeds
      up approval), and wait for the approval email.
- [ ] Add the redirect URI the app will listen on (for example `http://localhost:8765`; check that
      the portal accepts a plain http loopback address) and note the client id and secret.
- [ ] Keep the client secret out of the repository, the logs and the profile TOML: it goes in the
      same secret store as the MCP tokens (`wyck-config`).

### 2A.5 Work, in order

The crate itself (steps 1 to 3, the tests and the docs for it) is done and pushed; what is left is
the live run and everything that plugs it into the app (steps 4 to 7). Items marked done were
checked against a mock server, not against the real one.

1. **`ctrader-openapi`: transport** (`crates/ctrader-openapi`)
   - [x] WebSocket (wss) connection to `live` or `demo` on port 5036, JSON envelope with
         `clientMsgId`, replies matched to requests, unsolicited events on a channel.
   - [x] Heartbeat every few seconds (under the 10 second limit) and detection of a dead link.
   - [x] Reconnection is left out on purpose: the client never reconnects by itself, so the
         supervisor above it (the engine, step 6) owns backoff, authorizing again and resubscribing.
   - [x] Two rate limiters: 50 per second, and 5 per second for historical calls (queue, do not fail).
   - [x] Typed errors: `REQUEST_FREQUENCY_EXCEEDED` (retry later), maintenance (`retryAfter`,
         `maintenanceEndTimestamp`), an invalid token, an unauthorized account. Never retry a
         mutating call (there are none yet).
   - [x] A mock server for tests (in process, same style as `ctrader-mcp`'s `test-support`).
2. **Sign in** (same crate, plus `wyck-config`)
   - [x] Loopback OAuth: build the grant URL (scope `accounts`), open the browser, listen on the
         redirect port, take the code, exchange it within a minute, verify the `state` value.
   - [x] `examples/sign_in.rs`: runs that flow from the terminal and lists the accounts, to get a
         first token and to settle whether the server echoes `state`. Not run yet (no approved
         application).
   - [ ] Store the access and refresh tokens in the secret store, never in the profile file.
   - [ ] Refresh before the 30 day expiry, and on `ProtoOAAccountsTokenInvalidatedEvent`; if the
         refresh fails, send the user back to the sign in screen with the reason. The crate gives
         `TokenSet::expires_within`, `OAuthClient::refresh` and the invalidation event; the
         scheduling and the storage belong to the caller.
   - [x] Application auth, account list, account auth. Letting the user pick the account when
         several exist is UI work (step 7).
   - [ ] Profile fields in `wyck-config`: service tag `ctrader-openapi`, environment (demo or
         live), client id, redirect URI; the secret and tokens in the secret store.
3. **Market data calls** (same crate)
   - [x] Symbols list and details (digits, pip position, lot size), archived symbols excluded.
   - [x] Spot subscription and events: keep the last bid and last ask, since an event may carry one.
   - [x] Trendbar history with small windows and a seam check; live trendbar subscription.
   - [x] Tick history: decode the newest first delta timestamps, request bid and ask, follow
         `hasMore`, windows well under a week, watch the 5 per second historical limit.
   - [x] Depth quotes (optional, later): sizes divided by 100.
4. **Tick store and recorder** (`wyck-app` `chart/`, or a new `wyck-marketdata` module)
   - [ ] Store ticks in the redb file, in their own tables, keyed by symbol and time, with the price
         as an integer and the time as a delta from a block start, so a day of ticks stays small.
   - [ ] A coverage list per symbol (like `chart::coverage`) for the ranges fetched from history,
         and a separate marker for the ranges the app recorded live.
   - [ ] A background recorder that writes the live ticks in batches, so the app builds a history
         longer than the broker keeps.
   - [ ] A pruning setting (size or age), since tick data grows fast; expose the size on disk.
   - [ ] Decide bid, ask or mid as the bar price, and make it a chart setting.
5. **Aggregation** (pure, tested without a window, like the rest of `chart/`)
   - [ ] Ticks to time bars for any length (1s, 5s, 15s, and so on), to tick bars (every N ticks),
         and later range and volume bars. The volume of a bar is its tick count, and the chart must
         say so.
   - [ ] Incremental: a new tick updates the last bar without rebuilding; a seam between recorded
         and fetched ticks must not create a fake bar.
   - [ ] Property tests: the bars of any partition of the same ticks agree; M1 built from ticks
         matches the broker's own M1 within the expected difference.
6. **Engine** (`wyck-engine`)
   - [ ] The `MarketData` port and a tick `broadcast` channel next to the `EngineState` watch.
   - [ ] `ServiceKind::CtraderOpenApi`, `ConnectRequest` variant, `ConnectFlow` states for the new
         sign in (waiting for the browser, exchanging the code, choosing an account).
   - [ ] Keep the rule that no order is sent without arming; the Open API adapter has no order
         methods until a separate item adds them.
7. **UI** (`wyck-app`)
   - [ ] The first screen offers three choices: cTrader Desktop (MCP Local), a token (MCP Remote),
         and "Open API" (client id, secret, callback address, then "Sign in with cTrader").
   - [ ] Screens for the waiting browser, a failed or expired sign in, and the account picker,
         written like the existing connection screens (plain, with the reason and what to try).
   - [ ] Chart: new time frames (tick counts and seconds) shown only when a tick source is
         connected; a small label of the data source; a clear message when the connected source has
         no ticks. With MCP only, nothing changes.
   - [ ] The live bar folds real ticks instead of one bid per second, and the 15 second tail refresh
         is no longer needed for an Open API session.
8. **Tests and validation**
   - [x] Unit tests for the delta decoding, the rate limiters, the token refresh and the OAuth
         `state` check; contract tests against the mock server.
   - [x] A live test, ignored by default and run on a demo account only It is written (`tests/live.rs`) but not run: it needs an approved application., that signs in, subscribes to
         one symbol, reads a minute of ticks, and requests a day of history.
   - [ ] Findings from the live run go to section 3.8, like the MCP quirks.
9. **Docs**
   - [x] `docs/ARCHITECTURE.md`: the new crate, the port, the tick flow; an ADR for "a separate crate,
         JSON first".
   - [x] README of the new crate with a worked example, and the credential setup steps for the user.

### 2A.6 Risks and unknowns

- **Approval time and rules** of the application are outside our control, and the documentation says
  nothing about limits on users or accounts per application.
- **Tick retention** is not documented, so how far back history goes is only known by trying, per
  broker.
- **No tick volume**: order flow style charts (footprint, volume profile from real trades) stay out
  of reach; a tick count is what can be offered.
- **The client secret in a desktop app** is a known weak point of the OAuth flow; the design where
  each user enters their own application credentials (as planned) avoids shipping one shared secret.
- **Demo and live are separate connections**, so an account switch between them means a second
  connection, not a setting.
- **Windows firewall and port use**: the loopback listener must pick a free port and match a
  registered redirect URI exactly; handle "port busy" with a clear message.

### 2A.7 What the first live run showed (2026-09-20, demo account, `Spotware` broker)

Run with `tests/live.rs` on a demo account while the market was closed (Sunday).

- [x] **Confirmed**: the JSON WebSocket on `demo.ctraderapi.com:5036` works with this client;
      application sign in, account list (permission scope 0, view), account sign in, the symbol list
      (830 symbols) and symbol details (`digits 5`, `pipPosition 4`, `lotSize 10000000`, volumes in
      hundredths of a unit, schedule time zone `America/New_York`), the proxy version (`101`), the
      spot subscription, and a first spot event that carries the last price and its timestamp even
      with the market closed (as the `.proto` comment says). Ids come as plain numbers.
- [x] **Probably confirmed**: enumerations as numbers. The tick and bar requests carry a quote type
      and a period as numbers and the server answered without an error.
- [ ] **Still open**: history. Both tick sides and a day of M1 bars came back empty, but the range
      ended at the clock and the market had been closed for about a day, so it proves nothing. The test
      now ends the range at the time of the last price seen. Run it again, and once more with the
      market open, then settle: the tick price and time encoding, which end a truncated bar answer
      holds, the range limit per period, the tick count per response, and how far back ticks go.
- [ ] Run `examples/sign_in.rs` to learn whether the consent page echoes `state`.

#### Second live run and what it changed

The run with the history anchored at the last price (about 47 hours before the clock, the market
closed) gave 14 208 bid ticks and 13 752 ask ticks for six hours, so the request and the paging
work. It also showed two things the documentation did not say, both now fixed and tested:

- [x] **Tick prices are differences too.** The oldest bid tick read `1` and the oldest ask tick `-1`
      beside a newest price of `114880`. Like the time, every tick after the first is a step from
      the one before it, so `types::decode_ticks` now keeps a running sum of the price as well. (The
      `.proto` comment only says so for the time.) The live test now fails if the prices of a history
      spread wider than a few percent.
- [x] **`BLOCKED_PAYLOAD_TYPE`.** After the six hour bid and ask histories, the next tick request was
      refused with this code, "You are being rate limited", `retryAfter` 1. The `.proto` says the
      field is **seconds until that payload type is unblocked** (I had read milliseconds; the same
      goes for `maintenanceEndTimestamp`, Unix seconds). So this server blocks one type of request
      for a while, and did so at our documented 5 per second. The client now keeps under the limits
      (40 and 4 per second), and resends a request refused for its rate after the wait the server
      asks for, up to three times (`rate_limit_retries`, `max_retry_wait`).
- [x] Find the real limit for tick requests: not needed so far, the margin and the retries were
      enough on the third run (see below). Keep an eye on it with a long history.

#### Third live run: confirmed

- [x] **Tick prices are right end to end**: bid 1.14585 to 1.14904 with the last at 1.14880, ask
      1.14586 to 1.14904 (14 208 bid and 13 752 ask ticks over six hours), so the running sum of
      prices and times is correct. The first bid and ask ticks of the range differ by one
      unit and the ask sits one unit under the bid, which is a tick pair about 0.6 second apart,
      not a decoding error.
- [x] **The refusal did not come back.** With the margin (4 per second) and the retries, the
      six hour bid and ask histories, a one hour pair and a day of M1 bars ran without a block.
- [x] **Volume of the history**: 2 158 quotes in the last hour before the anchor, 1 400 M1 bars in
      one request range of a day (a full trading day is about 1 380), so a day of M1 is not cut.
- [ ] The live test now also prints the first and last bar and compares the high and low of each
      minute with the bid ticks of that minute. Run it once more to see how many minutes match:
      that settles the bar decoding (a low plus offsets) and which side the bars are built on.


---
---

## 3. P0: live validation before any real order

The engine has now been run against the real servers. **Remote and Local are both validated
end to end on demo accounts** (read paths, market orders with protection, protection changes,
partial and full close, flatten; on Remote also rejections, a lost reply, a closed market).
Validation found real defects, all fixed and pinned by tests; they are listed in 3.8 and in
`crates/wyck-engine/CHANGELOG.md`.

**Do not arm the engine on a funded account until 3.7 is empty**, or each item left there has
a written reason and a workaround.

### 3.1 The live tests (done)

Files: `crates/wyck-engine/tests/live_remote.rs`, `live_local.rs`, shared helpers in
`live_support/mod.rs`. Every test is `#[ignore]`d, so `cargo test` and CI never touch a
server.

```sh
# Remote, read only:
WYCK_LIVE_REMOTE_TOKEN=... cargo test -p wyck-engine --test live_remote -- --ignored read_only
# Remote, places and closes real orders (demo token only):
WYCK_LIVE_REMOTE_TOKEN=... WYCK_LIVE_CONFIRM_DEMO=1 \
    cargo test -p wyck-engine --test live_remote -- --ignored --test-threads=1
# Local, read only (prints the active account id and kind):
cargo test -p wyck-engine --test live_local -- --ignored read_only --nocapture
# Local, orders on the ACTIVE cTrader Desktop account (see 3.7 before using it):
WYCK_LIVE_CONFIRM_DEMO=1 WYCK_LIVE_LOCAL_TRADER_ID=<id> \
    cargo test -p wyck-engine --test live_local -- --ignored --test-threads=1 lifecycle
```

Variables: `WYCK_LIVE_REMOTE_TOKEN`, `WYCK_LIVE_REMOTE_ENDPOINT`, `WYCK_LIVE_LOCAL_ENDPOINT`,
`WYCK_LIVE_LOCAL_TRADER_ID`, `WYCK_LIVE_CONFIRM_DEMO`, `WYCK_LIVE_SYMBOL` (default `BTCUSD`,
which trades at the weekend; `EURUSD`, `USDJPY` and `XAUUSD` are also sized in
`live_support::sizing`), `WYCK_LIVE_ORDER_TIMEOUT_MS`.

Safety built into the tests:

- A test that places orders needs `WYCK_LIVE_CONFIRM_DEMO=1`. The Remote tests also refuse a
  token whose `environment` claim is not `demo`. The Local test also needs the trader id of the
  account to be spelled out and equal to the active one.
- They panic when the account already has a position or a working order, so they can never
  flatten someone's trades.
- They always flatten what they opened, even when an assertion fails (the lost-reply test
  cleans up with a fresh engine, because its own timeout would cut the cleanup short).
- The token only ever comes from the environment. Never write it to a file in the repository.

Cost of the whole Remote suite on the demo account: about 2 to 5 USD of spread.

Also added: `crates/ctrader-mcp/examples/raw_call.rs` (prints raw answers for tools read from
stdin, refuses state-changing tools without `CTRADER_CONFIRM_DEMO=1`), and an order round trip
in `probe_remote.rs` behind the same switch.

### 3.2 Remote, read only (done, demo account)

- [x] Connects and reaches `Ready`; currency, balance, equity, free margin match the platform.
- [x] `AccountKind` is `Demo`. It was `Unknown`: the real token is base64url JSON, not a JWT.
      Fixed in `AccountKind::from_token`, pinned by a test.
- [x] `get_server_time`: **the tool does not exist on Remote** (16 tools, none for time).
      `RemoteBroker::server_time` now says so; the engine falls back to the local clock. The
      Local shape is pinned in `parse_server_time` (3.4).
- [x] Symbols and precision: `get_symbols` has **no `pipDigits`**. Raw quote prices are always
      integers in units of 1e-5 (USDJPY `15676800` is 156.768, XAUUSD `437857000` is 4378.57).
      The adapter uses that constant and infers the decimals from quotes (EURUSD 5, USDJPY 3,
      XAUUSD 2, BTCUSD 2 where the truth is 3, which is the safe direction). Pinned with
      captured quotes and by the live test.
- [x] Volume rules: not published, as expected. Per-symbol rules are now configurable
      (`AssumedSpecs::symbols`, reported as `SpecsSource::Configured`). The live tests use the
      values Local publishes for the same broker. Confirmed on Remote for BTCUSD only (0.01
      accepted as volume 1 cent); the forex values are still unconfirmed on Remote (3.7).
- [x] Quotes: bid and ask decode correctly, `timestamp` is milliseconds.
- [x] `get_positions` does **not** echo the order `label` (or `comment`). Reconciliation of an
      unknown order now falls back to symbol, side and volume in the background refresh too,
      not only while confirming (this was a real gap, see 3.8).
- [x] Pending orders: `limitPrice`, `stopPrice`, `stopLoss`, `takeProfit` are display prices.
      Checked with a LIMIT order and its cancellation.
- [x] Money on positions: only ever `0` so far (commission, swap). The scale of a non-zero
      value is unconfirmed (3.7). There is **no per-position P&L** on Remote.
- [x] Margin: `get_balance` has no margin fields, but with a position open
      `equity - freeMargin` equals the margin in use. `RemoteBroker::account` now derives used
      margin and margin level from it.

### 3.3 Remote, order lifecycle (done, demo account, BTCUSD)

- [x] Engine armed with the acknowledged kind `Demo`.
- [x] Market order with stop loss and take profit through `plan_entry` and `submit`: the
      position appears with the planned volume, and the SL and TP are exactly the planned
      distances from the fill (relative distances in points of 1e-5 are right).
- [x] `set_protection` on one leg keeps the other. Also verified raw that **Q-R10 still holds
      on build 1.0.18**: an `amend_position` without `takeProfit` removed it, whatever the
      tool description says ("omit to leave unchanged"). Keep re-reading and sending both legs.
- [x] Partial close, then full close. Volume is in cents of a unit on the wire.
- [x] `flatten` with two open positions: preview, token, execute, report.
- [x] Rejected order (zero volume sent straight to the adapter): `Rejected` with the server's
      message, no position created.
- [x] Market closed (EURUSD at the weekend): HTTP 409 "Trading is not available: Market is
      closed." is reported as `Rejected` (`certainly_not_sent` classifies it correctly).
- [x] Lost reply: with `order_timeout` at 150, 400 and 900 ms the outcome was `Filled` after
      confirmation, with exactly one position each time, never two. With a 1 ms timeout the
      request never leaves and the outcome is `Unknown` with no order on the account, which
      is also correct. The path where the order lands but its confirmation fails is covered by
      the mock scenarios (`a_late_order_is_reconciled_by_shape_...`), not by a live run.
- [ ] Repeat the lifecycle on `EURUSD`, `USDJPY` and `XAUUSD` when the forex market is open
      (Monday): `WYCK_LIVE_SYMBOL=EURUSD ... --ignored --test-threads=1`. This is what confirms
      the configured forex volume rules, the precision inference and the stop rounding on Remote
      for the symbols most people trade. Note whether a stop distance that is not a multiple of
      the symbol's tick is rejected.

### 3.4 Local, read only (done, on whatever account is active)

- [x] Connects and reaches `Ready`. Currency is `depositAsset` (the DTO only knew
      `currency`, so sizing would have had no account currency).
- [x] Account kind: `get_balance` has no demo or live field (`accountType` is the margin mode,
      `Hedged`). The adapter reads `isLive` from `get_accounts_list`, by identity when `traderId`
      is listed and otherwise by matching broker, currency, type and balance (see 3.7). On the
      validation machine `traderId` was **not** in the list.
- [x] `get_symbol_details`: `minVolume`, `maxVolume` and `volumeStep` are **in units**, not
      lots (EURUSD lot 100000, min 1000, step 1000; XAUUSD 100, 1, 1; BTCUSD 1, 0.01, 0.01).
      The adapter read them as lots. Fixed and pinned with captured answers. `digits` and
      `pipSize` are per symbol (BTCUSD digits 3, pip 0.1).
- [x] Every volume tool **requires `volumeType`** (`lots` or `units`); `ctrader-mcp` did not
      send it, so every Local order would have been refused. Now always sent as `units`.
- [x] `get_symbols` names the field `name`, not `symbolName`. Fixed with an alias.
- [x] `get_spot_prices` answers "No live quote ... unsubscribed" for a symbol that is not in
      the Market Watch (BTCUSD here). The adapter treats a `Rejected` error as no quote.
- [x] `get_server_time` returns `unixMs`, `utcTime` and `localTime`. The engine looked for
      other names and always fell back to the local clock. Fixed and pinned.
- [x] `get_positions` field names, read from a live position: `id`, `symbolName`, `tradeSide`
      (`"Buy"`), `entryPrice`, `stopLossPrice`, `takeProfitPrice` (not `stopLoss`), `netProfit`,
      `label`, `comment`, `volumeInUnits`, `volumeInLots`, `pips`. The decoder read no stop or
      take profit until fixed. `place_market_order` answers `{"positionId", "status":"opened"}`.
- [ ] `get_pending_orders` field names: no working order was read yet (5.1 needs them).

### 3.5 Local, order lifecycle (done, demo account 3382707, BTCUSD)

Run through `live_local.rs::lifecycle_...` on 2026-09-19, the user having confirmed the active
account is a demo:

- [x] Market order with SL and TP as pip distances: volume, SL and TP exact.
- [x] `set_protection` changes one leg and keeps the other.
- [x] Partial close (`close_position_partial` with `volumeType`), full close.
- [x] `flatten` with two positions (the engine closes item by item; Local also has
      `close_all_positions`).
- [x] Local echoes `label` and `comment` on positions.
- [ ] Rejected order and closed-market answers on Local, and `certainly_not_sent` on them.
- [ ] Fractional pips: the schema takes a number, the adapter sends whole pips (A9).

### 3.6 Failure behavior (partly done)

- [x] Lost reply, never a second order (3.3).
- [ ] Kill the network during a session: state goes `Reconnecting`, trading disarms, the session
      recovers, positions are reconciled. Manual test with a demo session (disable the adapter
      or pull the cable), not automated.
- [ ] Interrupt cTrader Desktop while connected to Local: same expectations.
- [ ] Rate limits: run the headless example for an hour with four watched symbols and confirm no
      throttling errors. The engine calls `get_positions` twice per refresh (5.9).

### 3.7 What is still open (the gate)

1. **Local account kind is inferred, and `Unknown` when it cannot be.** The active account
   (`traderId` 3382746 on 2026-09-20) is not in `get_accounts_list` under its `id` or `login`,
   but the one listed account has the same broker, currency, account type and balance to the
   cent, and no other tool says demo or live. The adapter now takes the kind from listed accounts
   that match on all four and agree on `isLive`, and reports `Unknown` in every other case (no
   match, a cent of difference, a demo and a live account with the same figures). Not proof: it
   rests on the two answers being read a moment apart. Arming acknowledges whatever kind is
   reported, and a front end must keep treating `Unknown` as possibly live. The Local order test
   still only runs with `WYCK_LIVE_LOCAL_TRADER_ID` naming the account.
2. **Forex on Remote** (3.3, last item), needs the forex market open. Same for Local with a
   forex symbol in the Market Watch.
3. **Rejections on Local** (3.5, last items) and pending order fields (3.4).
4. **Money scale on Remote positions**: `swap`, `commission` and a non-zero value of any money
   field. `RemoteBroker::money` reads a whole number as scaled by `moneyDigits` and a fractional
   one as decimal. Confirm with a symbol that charges commission (forex, on a weekday).
5. **Failure drills** (3.6): network loss, cTrader Desktop restart, an hour of polling.
6. **CI**: the pushes for the engine core and the previous TODO were cancelled by newer pushes
   before finishing. Run `gh run list --branch main` and confirm the last run is green.

### 3.8 What the live servers actually do (findings)

Each row replaced an assumption in the code. Section 9 tracks the status of the rest.

| Topic | Assumed | Real (2026-09-19, `rest-proxy 1.0.18`, Local 2026-09) |
|---|---|---|
| Remote token | a JWT | base64url JSON `{"plant","environment","token"}` |
| Remote quote scale | per-symbol `pipDigits` | always 1e-5, `pipDigits` never sent |
| Remote position, order, deal prices | integer pipettes | display decimals |
| Remote amend and create prices | integer pipettes | display decimals (relative SL and TP stay in points of 1e-5) |
| Remote server time | `get_server_time` | no such tool |
| Remote order label on positions | echoed | not echoed |
| Remote margin | not available | `equity - freeMargin` while a position is open |
| Remote pending order reply | a position only for fills | a placeholder position with volume 0 and an id |
| Local volume unit | lots of `lotSize` | units, with `volumeType` required on every volume tool |
| Local currency field | `currency` | `depositAsset` |
| Local symbol name field | `symbolName` | `name` |
| Local server time | `timestamp`, `serverTime`, ... | `unixMs`, `utcTime`, `localTime` |
| Local account kind | `accountType` says demo or live | `accountType` is the margin mode; `isLive` only in the account list |
| Volume representation | whole units | hundredths of a unit (BTCUSD minimum is 0.01) |
| Reconciling an unknown order | by label | by label, else symbol, side and volume |

Two defects that would have hurt money if the order path had gone live unchecked: an
`amend_position` that sent pipettes as prices (a stop at 8,095,969), and an unknown order that
could never be reconciled on Remote because reconciliation only matched labels.

---

## 4. P1: decisions and foundations

### 4.1 GUI framework: GPUI (decided)

Decided on 2026-09-19: **GPUI**, through `gpui-kit` (GPUI plus `gpui-component`). The visual
design is the owner's, so the application ships no visuals. The reasons, the options not chosen,
the risks and the fallback are in [docs/decisions/0001-gui-framework.md](docs/decisions/0001-gui-framework.md).
How the dependency is pinned, built and upgraded is in [docs/gpui-dependency.md](docs/gpui-dependency.md).

- [x] Decision made and recorded.
- [x] Dependency policy written and checked: `gpui-kit =0.6.4` resolves and builds with the
      workspace on `rustc 1.98.1` (Windows), with a single `reqwest` and TLS stack.
- [x] **Async bridge.** `cx.spawn` awaiting `watch_state().changed()`, no `gpui_tokio`, no blocked
      executor: the shell follows the engine from Disconnected to Ready.
- [x] **Global hotkey** fires while another program has the focus, on GPUI's own message loop,
      with no extra thread and no `unsafe`.
- [x] **Link check.** A binary that uses GPUI links and runs with `--locked`, and the window
      renders (Direct3D 11.1).

The rest is what the validation still has to prove. Each item passes or triggers the fallback to
egui, see the decision record:

- [ ] **Floating panel.** A `WindowKind::PopUp` window (`focus: false`) stays on top of cTrader
      Desktop and never takes its focus. Check the transparent border issue (#61508) if
      transparency is wanted.
- [ ] **Held key.** No frame stall under key repeat (#61469), no phantom Alt on window
      activation (#62404).
- [ ] **Tables.** A few hundred rows in the `gpui-component` data table, virtualized and
      keyboard navigable.
- [x] **Charts.** A candlestick chart with pan, zoom, crosshair, auto and log scale, live bar and a disk
      cache is built on GPUI canvas painting (see `docs/ARCHITECTURE.md`). Still to do: indicators,
      drawing tools, chart types, several charts.
- [ ] **Testing.** Whether GPUI's test support helps beyond the view-model tests that already
      run without a window.
- [ ] **Packaging.** Installer, code signing, auto-update and binary size (also 6.5).
- [ ] **Linux.** GPUI needs system packages there; CI builds `wyck-app` without the `gui` feature
      on ubuntu for now.

### 4.2 `wyck-app` (done)

- [x] Crate `crates/wyck-app`: depends on `wyck-engine`, `wyck-config` and, behind the `gui`
      feature, `gpui-kit`. It does not depend on `ctrader-mcp` or `wyck-calendar`.
- [x] `cargo run` opens the window. The binary is thin; the logic is a library tested without a
      window (59 tests).
- [x] No visuals: an empty dark window, and one function in `main.rs` where your own views plug in
      (`shell::run`). `presentation` returns strings and tones, never colors.
- [x] `docs/ARCHITECTURE.md` and `crates/wyck-app/README.md` updated.
- [ ] Your views.

### 4.3 Engine boundary

- [x] Decision: the engine runs **in the app's process**, on its own runtime, behind
      `EngineHandle`. It works today and is what `wyck-app` does. RPC only if a headless
      always-on engine or several front ends are wanted (5.14).
- [ ] If a separate process is wanted later: define the wire protocol from the existing serde
      types (`EngineState`, `Event`, `OrderPlan`, `EntryIntent`, ...), pick a transport (local
      socket, gRPC, or JSON over a pipe), and add a client crate implementing the same handle
      surface. See 5.14.

### 4.4 Application shell concerns

- [x] **Logging.** `wyck-app::logging`: a daily rolling file (kept 14 days) under the data
      directory, filtered by `WYCK_LOG`, plus stderr in debug builds. A test keeps tokens out of
      log lines.
- [ ] **Engine configuration persistence.** `EngineConfig` is `serde` with defaults but nothing
      stores it. Add a section to `wyck-config`'s `AppConfig` (or a sibling file) and load it at
      startup; validate with `EngineConfig::validate` and show errors instead of failing silently.
      Until then the app takes its settings from environment variables (`wyck-app::settings`).
- [x] **Error presentation.** `wyck-app::messages::describe_error` maps every `EngineError` to a
      notice once, in one place.
- [x] **Startup flow.** `wyck-app::startup`: environment or active profile to a `ConnectRequest`,
      then connect. A first run without a profile is a banner, not a crash.
- [ ] **Profile creation.** Profiles are made with `wyck-config` for now; the app has no screen
      for it.
- [x] **Crash safety.** A session marker: the next start warns about a session that did not end
      cleanly **only if it was at risk** (engine armed, an order in flight, or an order of unknown
      outcome, see `session_marker::at_risk`). A dry-run session stopped from an editor or killed
      is logged, not announced: the app cannot arm yet (6.1), so nothing could be in flight, and a
      warning at every start would only teach the user to ignore it. A marker that cannot be read
      is still reported.

---
## 5. P2: wyck-engine, what is left

Crate: `crates/wyck-engine`. Start with its crate docs (`cargo doc -p wyck-engine --open`).

### 5.1 Pending orders (limit, stop, stop-limit)

Only market orders are placed today. The engine can list and cancel working orders.

- [ ] Extend `Broker` (`src/broker/mod.rs`) with `place_pending(&PendingOrderRequest)` and
      `amend_pending(...)`. Remote: `CreateOrderParams::limit/stop` (`ctrader-mcp`), with absolute
      SL/TP allowed on non-market orders (Q-R4 only forbids them on MARKET). Local:
      `place_limit_order`, `place_stop_order`, `place_stop_limit_order`, `amend_order`.
- [ ] Planning (`src/risk.rs`): `EntryIntent` gains an entry type and an entry price. Risk is
      computed from the entry price, not the market. Validate the side of the market (a buy limit
      below the ask, a buy stop above it) and reject inverted orders.
- [ ] Pipeline (`src/trading.rs`): same guarantees as market orders (armed, single-use plan, in
      flight lock, no replay). Confirmation reads **pending orders**, matched by label.
- [ ] Expiration (`GOOD_TILL_DATE`): Remote wants integer epoch milliseconds only (Q-R2).
- [ ] Local quirk to verify live: SL and TP semantics on pending orders are asymmetric (Q-L2,
      `normalize_pending_order_take_profit` in `ctrader-mcp`).
- [ ] Scenario tests with `MockBroker`, contract tests for both adapters, docs.
- [ ] Events: `OrderPlanned` and `OrderResult` already exist; add a pending-order variant if the
      outcome type needs one (working, filled later).

### 5.2 Order and position management extras

- [ ] Move stop loss to break-even (with an optional offset), as one command.
- [ ] Trailing stop helper (server-side trailing is exposed by Remote through `trailingStopLoss`;
      Remote's `AmendPositionParams` supports it). Decide server-side versus engine-side.
- [ ] Close by percentage (25, 50 percent) with correct rounding to the step.
- [ ] Reverse position (close and open opposite) as one guarded command.
- [ ] Scale in: add to an existing position with re-sized stop.
- [ ] Bracket orders across several targets (partial take profits): needs order groups.
- [ ] Amend pending orders (price, SL, TP, expiry).

### 5.3 Sizing improvements

- [ ] **Cross-currency conversion.** Today only a direct or inverse spot pair to the account
      currency is used (`conversion_rate` in `src/trading.rs`). `ctrader-mcp` has
      `math::conversion::compute_chain` (multi-hop) that could be reused; needs a quote fetch for
      the chain's symbols.
- [ ] Sizing modes beyond risk percent and fixed volume: fixed lots by symbol preset, fixed
      dollar risk per symbol, volatility-based (ATR) stops.
- [ ] Commission and swap aware sizing (subtract expected commission from the risk budget).
- [ ] Round-trip cost estimate shown in the plan (spread plus commission).
- [ ] Use `ctrader-mcp` `workflows::pre_trade_briefing` and `compare_trading_costs` where useful
      (spread and swap context in the plan).
- [ ] Margin check: needs leverage tiers, which neither server provides. Investigate whether the
      Local `calculate_margin` capability (found by the probe, undocumented) can supply a number.
- [ ] Instrument categories (forex, metals, indices, crypto) to pick defaults and sanity limits.

### 5.4 Per-symbol volume rules (blocks trusting Remote sizing)

Remote publishes no lot size, minimum or step. Local publishes them (in units), and the same
broker's values are the best available seed: EURUSD and USDJPY lot 100000, min and step 1000,
max 10,000,000; XAUUSD lot 100, min and step 1, max 10,000; BTCUSD lot 1, min and step 0.01,
max 50 (read from Local on 2026-09-19).

- [x] Per-symbol rules in `EngineConfig` (`assumed_specs.symbols`, validated, reported as
      `SpecsSource::Configured`).
- [ ] A way to edit them in the GUI and to persist them in `wyck-config`.
- [ ] Seed them automatically: when a Local session is also available, read
      `get_symbol_details` for the watched symbols and offer those values for Remote (same
      broker, same numbers), instead of asking the user to type them.
- [ ] Learn from rejections: when the server rejects a volume, capture the message and suggest an
      override.
- [ ] Keep the planner warning for assumed specs until values are confirmed. Remote volume rules
      are confirmed for BTCUSD only (3.3).

### 5.5 Multi-account

The API already carries `AccountId` on every command and event.

- [ ] Several `Broker` sessions at once, each with its own supervisor, state slice and gate.
- [ ] `EngineState` becomes a map of account states plus a global part; keep single-account
      accessors for the simple case.
- [ ] Order routing: an order names its account; a broadcast-to-several-accounts command with
      per-account results.
- [ ] Aggregated P&L and exposure across accounts.
- [ ] Per-account arming (arming one must not arm another).
- [ ] Profile switching without losing the other sessions.

### 5.6 Guardrails

`src/guardrails.rs` has per-trade risk, total open risk, stale data and news. Warn, never block.

- [ ] Prop-firm rules: daily loss limit, max drawdown, max open lots, allowed trading hours,
      consistency and news restrictions. Configurable rule sets; warnings only.
- [ ] Multi-account conflict warnings (the README roadmap item): same symbol and side across
      several funded accounts.
- [ ] Spread guardrail thresholds in config (currently a fixed quarter of the stop distance).
- [ ] Max positions per symbol, max trades per hour (overtrading), loss-streak warning.
- [ ] Weekend and rollover warnings; market-closed detection from symbol sessions.
- [ ] Correlation warning (several trades on correlated pairs).
- [ ] Better open-risk estimate: currently counts only positions quoted in the account currency
      and reports the rest as unknown. Reuse the conversion rates once 5.3 lands.

### 5.7 News (engine side of `wyck-calendar`)

- [ ] Persist the last good calendar to disk so a restart does not spend a request (feed limit
      about 2 per 5 minutes per IP) and does not start empty offline.
- [ ] Start-up storm protection: several app restarts within minutes trip the limiter; the
      calendar service waits out `Retry-After`, but the UI should say why news is missing.
- [ ] Week rollover: the feed serves only the current week. On a Friday evening "nothing
      upcoming" must not read as "no news". Expose `week_ends_at` in `NewsView`.
- [ ] Watchlist and minimum impact configurable and persisted (the filter is built from the
      traded symbols today, with a fixed high-impact minimum).
- [ ] Optional: hold-off window after a release for extra caution (already a grace period).
- [ ] Optional: a second data source as fallback (the feed also exists as `.xml` and `.csv`, same
      host and same limit, so it is not a real fallback; a different provider would be).

### 5.8 Data the engine does not expose yet

- [ ] Order history and deals (Remote `get_order_history`, `get_deals`; Local equivalents), for a
      journal and for realized P&L today and this week.
- [ ] Symbol sessions (`get_symbol_sessions`, Local) to know when a market is open.
- [x] Trendbars: `Broker::bars` and `EngineHandle::candles` (Remote and Local `get_trendbars`), used by
      the chart. Still open: an ATR from them, and a pruning setting for the disk cache.
- [ ] Quote stream instead of polling every second: check whether either server can push. Today
      `refresh_quotes` polls the watched symbols and open-position symbols.
- [ ] Realized P&L, daily statistics, equity curve snapshots.

### 5.9 Efficiency and robustness details found while writing the engine

- [ ] Remote `get_positions` returns positions and orders together, but the adapter calls it
      twice per refresh (`positions()` and `pending_orders()`). Add a combined `snapshot()` to the
      `Broker` trait or cache within a refresh tick.
- [ ] Local `quotes()` makes one call per symbol, sequentially. Fine for a few symbols; measure
      and parallelize if the watch list grows.
- [ ] `RemoteBroker::close` only marks intent; the MCP session closes on drop. Verify there is no
      lingering task after `Engine::shutdown` when the broker `Arc` is still referenced elsewhere.
- [x] `RemoteBroker::account` derives used margin and margin level from `equity - freeMargin`
      (checked live with one position open).
- [ ] Remote positions carry no P&L. Floating profit is available for the account as a whole
      (`equity - balance`); per position it would need quotes and the contract size.
- [ ] Remote `create_order` on a pending order returns a placeholder `position` (volume 0, with an
      id). Do not read it as a fill when pending orders are added (5.1).
- [ ] Instrument cache never expires. Symbol lists are stable per session, but a reconnect should
      clear the cache (currently only the session state is reset; check `instruments` in `Inner`).
- [ ] `Inner::plan_entry` reads the account every time (correct for sizing) but not in parallel
      with the quote; measure planning latency and overlap the two reads if it matters.
- [ ] Hotkey latency budget: measure `plan_entry` plus `submit` end to end on the mock (should be
      microseconds of engine time) and with real latency; write the number down. Target for the
      engine's own overhead: well under 5 ms.
- [ ] `EngineState` is cloned copy-on-write per update; with large quote maps this may allocate
      often. Profile before optimizing.
- [ ] Event broadcast capacity default 256; confirm the GUI can keep up, and handle `Lagged`
      by re-reading state (documented, needs a UI-side test).
- [ ] Activity log (`recent_events`) is in memory only; a persisted audit log of every order
      decision (plan, submit, outcome) would help post-trade review and dispute resolution.
- [ ] Config validation for cross-field sense (for example `confirm_delay * confirm_attempts`
      should fit inside `order_timeout` scale).
- [ ] Time handling: the engine uses the local clock for event stamps and a measured offset for
      news timing. Remote has no server clock, so there the offset is never measured and the local
      clock is used (a warn log at connect, nothing visible). A clock that is off makes news timing
      wrong; consider a visible warning, or measure the offset from a fresh quote timestamp when
      the market is open.

### 5.10 Observability

- [ ] `tracing` spans per command carrying the `CommandId`, so a log line can be tied to a
      hotkey press. Events already carry it.
- [ ] Counters worth having: refresh failures, reconnects, orders by outcome, plan-to-submit
      latency. A small metrics struct exposed on the handle is enough.
- [ ] A diagnostics bundle (recent events, state, config with secrets stripped) the user can copy
      when reporting a problem.

### 5.11 Testing to add

- [ ] Live tests (section 3.1), `#[ignore]`d and env-gated.
- [ ] Property tests for `diff_positions` and for guardrail thresholds.
- [ ] Contract tests for `LocalBroker` against a scripted Local MCP server (only the Remote
      adapter has one today; the Local adapter has unit tests for instrument building only).
- [ ] Long soak test with the mock: hours of simulated time, random failures, invariant checks
      (never two orders for one plan, never armed while not ready, state revision monotonic).
- [ ] Fuzz the feed parser and the `parse_server_time` shapes.
- [ ] Concurrency stress: many simultaneous handle calls from several threads.
- [ ] Coverage report in CI (`cargo llvm-cov`) with a floor for the engine.

### 5.12 Documentation to add

- [ ] A "trading day" guide: connect, watch, plan, arm, execute, flatten, with the safety model
      explained in user terms.
- [ ] A failure-modes table (what the engine does when X happens) kept next to the tests that
      prove it.
- [ ] An integration guide for the GUI (GPUI pattern: `cx.spawn` plus `watch_state`, handling
      `Lagged`, error mapping). Write it after the spike (4.1).
- [ ] Update the workspace `README.md` "Configuration" section (still says the format is in flux).
- [ ] `CHANGELOG.md` kept per crate as behavior changes.

### 5.13 API stability

- [ ] Review every `pub` item in `wyck-engine` for whether it must be public; mark enums that will
      grow `#[non_exhaustive]` (done for most).
- [ ] Decide the versioning policy once there is a second consumer (the GUI); until then `0.x`
      and breaking changes are allowed but recorded in the changelog.
- [ ] Add `cargo semver-checks` to CI once the API settles.

### 5.14 RPC boundary (optional, later)

- [ ] Client and server crates over the existing serde types; authentication between processes;
      versioned protocol; same conformance tests as `EngineHandle`.
- [ ] Only worth doing for a headless always-on engine or several front ends at once.

---

## 6. P3: the GUI

Talks only to the engine (`EngineHandle`). Framework: GPUI, see 4.1.

### 6.1 Screens and panels

- [ ] **Connection bar**: profile picker, session state, account id, account kind badge (Demo,
      Live, **Unknown shown as unknown**), latency, clock skew warning.
- [ ] **Arming control**: two steps, shows account and kind, requires typing or a deliberate
      gesture for Live. A permanent, unmistakable "ARMED" indicator when armed, and a red frame
      for a live account. Disarm always one keystroke away.
- [ ] **Account panel**: balance, equity, free margin, margin level, day P&L.
- [ ] **Positions table**: symbol, side, volume (lots and units), entry, SL, TP, P&L, R multiple,
      age; row actions: close, partial close, move stop, break-even.
- [ ] **Working orders table** with cancel and amend.
- [ ] **Order ticket**: symbol, side, risk (percent or amount) or fixed size, stop, target
      (risk/reward), shows the plan (volume, risk, cost, warnings) before anything is sent.
- [ ] **Warnings area**: news warnings, risk warnings, `UnknownOrder` notices with a dismiss
      action that calls `dismiss_warning`.
- [ ] **News panel**: filtered table with freshness indicator (`NewsFreshness`), countdown to the
      next high-impact event, forecast and previous, currency filter and watchlist editing.
- [ ] **Activity log**: recent events with command correlation (`recent_events`).
- [ ] **Settings**: engine config (limits, guardrails), per-symbol volume overrides, hotkeys,
      theme, news filter, logging level.
- [ ] **First run and account management**: first run, token entry with secure storage,
      connection test and service choice (Remote or Local) are done (`wyck-app` connection flow).
      Still to do: add several profiles, edit and remove them.
- [ ] **Watchlist and instrument cycling**.
- [ ] **Charts**, if they are in scope: see 4.1 and section 11.

### 6.2 Hotkeys (from the README's planned table, adjust after usage)

| Key | Action |
|---|---|
| `b` | Plan and send a buy |
| `s` | Plan and send a sell |
| `c` | Close current position |
| `Up` / `Down` | Adjust position size or risk |
| `[` / `]` | Adjust stop loss / take profit |
| `Tab` | Cycle instrument |
| `q` | Quit |

- [ ] Decide the flow for `b` and `s`: one-key execution with the configured risk and stop
      (fast, needs a very visible armed state) versus a confirm step. Make it a setting.
- [ ] Key repeat: the engine's in-flight lock and minimum interval already stop double orders;
      the UI should still ignore auto-repeat for order keys.
- [ ] Global hotkeys while another window has focus (framework dependent, see 4.1).
- [ ] Rebindable keys, conflicts detection.

### 6.3 UX safety requirements (non-negotiable)

- [ ] Never show demo styling when the account kind is `Unknown`.
- [ ] Every order shows its plan first; a dry-run banner while disarmed.
- [ ] `Unknown` order outcomes are loud and persistent until reconciled or dismissed.
- [ ] Disable order controls while the session is not `Ready`.
- [ ] No action hides behind a mouse-only path; all critical actions have a key.

### 6.4 Floating instant-trade panel

From the README roadmap (Axiom.trade or GMGN style):

- [ ] Compact always-on-top panel over whatever chart is in use, hotkey driven, never stealing
      focus.
- [ ] Configurable preset buttons for order size (count and values), not hardcoded.
- [ ] Unit-aware sizing: lots, shares, contracts or base units depending on the instrument.

### 6.5 Testing and packaging

- [ ] Logic tests without a window (view models against `MockBroker` and paused time).
- [ ] Screenshot or interaction tests if the framework supports them.
- [ ] Installers per platform, code signing, auto-update, crash reporting policy.
- [ ] Performance: input to order latency measured and budgeted.

---

## 7. P4: other crates

### 7.1 `ctrader-mcp`

- [ ] Order round trip in the live probes (section 3.6).
- [ ] The 15 Local capabilities found by `probe_local.rs` and not called yet: `close_chart`,
      `open_chart`, `listAvailableIndicators`, `get_news`, `calculate_margin`, `get_asp_tab` and
      `set_asp_tab`, `get_current_app`, `get_layout_mode` and `set_layout_mode`,
      `get_market_watch_panel` and `set_market_watch_panel`, `get_trade_watch_tab` and
      `set_trade_watch_tab`, `switch_app`. No behavioral spec exists, so this waits for a concrete
      need. `get_news` and `calculate_margin` are the ones the engine could use (see 5.3, 5.7).
- [ ] Rate limiter (`src/rate_limit.rs`) is tuned to the documented 5 requests per second and is
      unverified under sustained load; revisit if a real throttle response is observed.
- [ ] Typed responses for the `serde_json::Value` returning methods (chart templates, workspaces,
      watchlists, price alerts, plugins), once their shapes are known.
- [ ] A Local `bootstrap` workflow inside `ctrader-mcp` (the engine has its own light version in
      the Local adapter); consider moving it down so other consumers benefit.
- [ ] Document the `test-support` feature in the crate README and keep it out of the stable API.

### 7.2 `wyck-calendar`

- [ ] Confirm whether the feed ever emits `Holiday` as an impact value (handled defensively, only
      Low, Medium and High were observed).
- [ ] Optional gzip support (the host compresses on request; the payload is about 14 KB).
- [ ] Decide whether a `304` counts against the request budget (unknown; treated as counting).
- [ ] Optional: display times in a chosen zone helper; the feed uses US Eastern with DST.
- [ ] No `actual` value exists in the feed, so surprise-versus-forecast cannot be computed. A
      different source would be needed for that (see the journal and news ideas in section 11).

### 7.3 `wyck-config`

- [ ] `schema_version` (`crates/wyck-config/src/app_config.rs`) exists but no migration path;
      write real logic the day the schema changes.
- [ ] Store engine preferences, news filter and watchlist, per-symbol volume overrides.
- [ ] Multiple tokens per profile or per account for the multi-account work (5.5).
- [ ] Keyring failure UX: what the app shows when the OS keyring is unavailable and the encrypted
      file fallback needs a passphrase (the crate deliberately does not prompt).
- [ ] Config file backup and restore.

---

## 8. Repository and infrastructure

- [ ] CI: add `cargo deny check` (licenses, advisories, duplicate versions) and `cargo audit`.
- [ ] CI: scheduled run (weekly) to catch dependency drift and new lints on latest stable.
- [ ] CI: coverage job and a floor for `wyck-engine`.
- [ ] CI: MSRV check (the workspace declares `rust-version = 1.98`; confirm it is really the
      minimum and test it).
- [ ] Dependabot or Renovate for dependency updates, with a policy for the pinned `rmcp` and
      the `native-tls` note in the root `Cargo.toml`.
- [ ] Benchmarks (criterion) for planning and state publication.
- [ ] Publish docs (GitHub Pages) from `cargo doc`.
- [ ] `CONTRIBUTING.md` and a security policy (`SECURITY.md`): how tokens are handled, how to
      report an issue.
- [ ] Release process: versioning, changelog, tags, build artifacts.
- [ ] Clean the workspace: the root `Cargo.toml` still lists `directories`, `toml`, `argon2` and
      friends for `wyck-config`; check for unused entries after each crate change.
- [ ] README: rewrite the Configuration and Usage sections once there is something to run; the
      roadmap checkboxes there should mirror this file.

---

## 9. Assumptions to confirm

Each row is a place where the code guessed. Section 3 has the live results; the status column
says where each stands. A corrected row was wrong, is fixed in the code and pinned with a test
using the real answer. An open row still needs a live check.

| # | Assumption | Where | Status |
|---|---|---|---|
| A1 | Local volumes are lots of `lotSize` | `broker/local.rs` | **Corrected.** `minVolume`, `maxVolume`, `volumeStep` are in units and every volume tool needs `volumeType`; the adapter sends `units`. Position volume is `volumeInUnits`, confirmed. |
| A2 | Remote volume rules default to lot 100,000, min 1,000, step 1,000 units | `config.rs`, `broker/remote.rs` | **Partly confirmed.** Not published by Remote. Per-symbol rules exist (5.4). BTCUSD confirmed on Remote (0.01); forex open until the market is open (3.3). |
| A3 | Remote `pipDigits` gives the pip size | `pip_size_for_digits` in `broker/remote.rs` | **Corrected.** `pipDigits` is never sent. Digits are inferred from quotes, pip size follows the forex convention and is only trustworthy for currency pairs (BTCUSD real pip is 0.1, inferred 0.01). Prefer price-distance stops on Remote for anything but forex. |
| A4 | Remote relative stop distances are integer points of one pipette (1e-5) | `RemoteBroker::place_market` | **Confirmed** on BTCUSD: 500.00 sent as 50,000,000 gave a stop exactly 500.00 away. Open for symbols with a coarser tick (3.3). |
| A5 | `get_server_time` returns a time under one of several guessed names | `parse_server_time` | **Corrected.** Local answers `unixMs`, `utcTime`, `localTime`. Remote has no such tool. |
| A6 | Remote positions echo `label` or `comment` | `decode_position` | **Corrected.** Not echoed. Reconciliation uses symbol, side and volume, in the confirmation reads and in the background refresh. |
| A7 | Remote quote `timestamp` is in milliseconds | `RemoteBroker::quotes` | **Confirmed.** |
| A8 | Local P&L key, and account kind from `accountType` | `broker/local.rs` | **Corrected.** Kind comes from `isLive` in the account list, else `Unknown`. P&L is `netProfit`, confirmed. |
| A9 | Local `place_market_order` takes pip distances as integers, at least 1 | `LocalBroker::place_market` | **Partly confirmed.** Whole pips work (BTCUSD 5000 pips gave 500.00). The schema says `number`, so fractional pips are accepted and not used (3.5). |
| A10 | A `Rejected` broker error means the order was not sent; timeouts and connection errors are ambiguous | `certainly_not_sent` in `trading.rs` | **Confirmed on Remote**: a zero volume and a closed market (HTTP 409) come back `Rejected` and leave nothing; a client-side timeout is settled by reading positions. Open on Local. |
| A11 | The mock broker behaves like the real servers | `broker/mock.rs`, `tests/support/mod.rs` | **Narrowed.** The scripted Remote server now returns the real shapes. The mock still lacks a broker that does not echo labels, but the scenarios cover it. |
| A12 | The calendar feed rate limit is about 2 requests per 5 minutes per IP | `wyck-calendar` docs | Verified earlier; service defaults are conservative (5 minute minimum gap). |
| A13 | Remote money on positions (`swap`, `commission`, P&L) is scaled by `moneyDigits` like the balance | `RemoteBroker::money` | **Open.** Only `0` seen. A whole number is read as scaled, a fractional one as decimal (3.7). |
| A14 | Q-R10: an `amend_position` without one leg removes it | `quirks::amend_position_preserving_legs` | **Confirmed** still true on build 1.0.18, despite the tool description. |

---

## 10. Risks and open questions

- **Order path validated on demo accounts only.** Remote and Local have each run end to end (3.3,
  3.5); nothing has touched a funded account. Mitigations in place: dry-run default, arming with
  acknowledgement, single-use plans, no replay, label or shape reconciliation. Before any funded
  account: empty the gate in 3.7.
- **The reference notes were wrong in several places.** The `ctrader-mcp` DTOs and the skill
  notes were written from documentation; the live servers disagreed on token format, price
  encoding, volume units and required fields (3.8). Expect more of the same on paths not yet run
  (forex on both servers, rejections on Local). The `raw_call` example and the live tests are
  the way to check.
- **Volume model differences between servers** (A1, A2). Local's are now read from the broker;
  Remote's are configured per symbol and confirmed for BTCUSD only.
- **Charts on GPUI.** If price charts are in scope for v1, they are ours to build on GPUI's
  primitives or on the `gpui-component` charts; a candlestick chart with pan and zoom is unproven
  (4.1 spike).
- **Unofficial calendar feed.** No SLA, single host, throttled. The app must work without it.
- **cTrader MCP servers are young.** Behavior and quirks can change between builds
  (`rest-proxy 1.0.18` is the reference in the skill notes). Check the build id at connect and
  warn on a version the workarounds were not written for (the Remote bootstrap already reads
  it; the engine does not compare it yet).
- **f64 for prices and money.** Matches `ctrader-mcp` and is acceptable while volumes are exact
  integers and comparisons avoid float equality. Revisit if a rounding discrepancy is observed.
- **Single-account assumption in state shape** (5.5). API is ready; internals are not.
- **Open question: one-key execution.** Speed versus safety for the hotkey flow (6.2).
- **Open question: journal and backtesting scope** for v1 (section 11).
- **Open question: license and distribution.** The repository is Apache-2.0 and `publish = false`;
  decide if any crate is meant to be published.

---

## 11. v1 vision

Mirrors the README roadmap. Not next, listed with the crates each touches.

- [ ] Engine and UI split with an RPC boundary (5.14).
- [ ] Multi-account (token) management and multi-order management across accounts (5.5).
- [ ] Backtesting engine: strategy or ruleset against historical data (needs trendbars, 5.8, and a
      new crate).
- [ ] Trade journal: entries, exits, notes, running statistics (needs order history and deals,
      5.8, plus storage, likely SQLite, and a UI).
- [ ] Economic calendar with real filters: engine and `wyck-calendar` side done; the panel and
      watchlist editing are 6.1.
- [ ] Floating instant-trade panel (6.4).
- [ ] Prop-firm guardrails as non-blocking warnings (5.6).
- [ ] Multiple customizable charts, custom indicators, saved templates and layouts.
- [ ] Futures and order-flow support; other brokers and platforms (a new `Broker` adapter plus a
      new service tag in `wyck-config`).
- [ ] TradingView alert webhook ingestion for semi-systematic setups (README roadmap).

---

## 12. Reference

### 12.1 Where things are

| Need | Look at |
|---|---|
| Crate roles and data flows | `docs/ARCHITECTURE.md` |
| Repository writing and git rules | `AGENTS.md` |
| Engine overview and safety model | `crates/wyck-engine/src/lib.rs` (rendered: `cargo doc -p wyck-engine --open`) |
| Session, refresh, reconnect | `crates/wyck-engine/src/core.rs` |
| Order pipeline, arming, flatten | `crates/wyck-engine/src/trading.rs` |
| Sizing arithmetic | `crates/wyck-engine/src/risk.rs` |
| Warnings and thresholds | `crates/wyck-engine/src/guardrails.rs`, `config.rs` |
| Calendar hosting | `crates/wyck-engine/src/news.rs` |
| Broker port and adapters | `crates/wyck-engine/src/broker/` |
| Scenario tests | `crates/wyck-engine/tests/engine_scenarios.rs` |
| Scripted Remote server for tests | `crates/wyck-engine/tests/support/mod.rs` |
| Mock MCP server (library) | `crates/ctrader-mcp/src/test_support.rs` (feature `test-support`) |
| Old TUI engine, for reference | `git show 883f7ab:crates/wyck/src/engine.rs` |
| cTrader server behavior notes | the `ctrader-mcp-servers` skill references (local and remote server docs, known quirks, self-healing playbook, trader workflows) |

### 12.2 Decisions already made (and why)

- **Dry-run by default, explicit arming.** A front end bug or a dropped connection must not be
  able to send an order.
- **Warn, never block** for guardrails. The person decides; only structural problems refuse.
- **Never replay a mutating call.** A lost reply is settled by reading state back.
- **The engine owns a Tokio runtime**, and its handle can be awaited from any executor. This is
  what keeps the GUI choice open (GPUI is not Tokio based).
- **Engine and profile management are separate.** The GUI edits profiles with `wyck-config`
  directly and hands the engine a `ConnectRequest`.
- **One `Broker` trait behind `dyn`.** Remote, Local and the mock share one behavior contract.
- **Volumes are exact integers, in hundredths of a base-asset unit; prices and money are `f64`**
  display values. Hundredths because BTCUSD trades in steps of 0.01 of a coin.
- **The ratatui TUI was dropped** in favor of a native GUI.
- **GPUI with `gpui-component` for the GUI** (2026-09-19), pinned to exact versions through the
  crates.io snapshot. egui is the fallback. See `docs/decisions/0001-gui-framework.md`.
- **Calendar defaults follow the feed's real limit** (about 2 requests per 5 minutes), verified
  live; the service polls every 30 minutes and never twice within 5.
- **Push straight to `main`**, no branches, no attribution lines, plain ASCII text.

### 12.3 Calendar feed facts (`nfs.faireconomy.media`)

- URL: `https://nfs.faireconomy.media/ff_calendar_thisweek.json`, no authentication.
- Behind Cloudflare: `Cache-Control: public, max-age=60`, weak `ETag`, `Last-Modified`;
  conditional requests return `304`.
- Rate limit about 2 requests per 5 minutes per IP: then `429` with `Retry-After: 300` and an
  HTML body.
- Only the current week exists; `nextweek` and `lastweek` are `404`. `.xml` and `.csv` variants
  exist with the same limit.
- Records have six fields (`title`, `country`, `date`, `impact`, `forecast`, `previous`); no
  `actual`. `country` is a currency code or `All`. `date` carries a UTC offset (US Eastern).

### 12.4 Useful commands

```sh
cargo test --all-features                                   # everything
cargo test -p wyck-engine --test engine_scenarios           # engine scenarios
cargo run -p wyck-engine --example dry_run_order --features testing
WYCK_SERVICE=remote WYCK_TOKEN=... cargo run -p wyck-engine --example headless   # read only
# Live validation, demo accounts only (3.1):
WYCK_LIVE_REMOTE_TOKEN=... cargo test -p wyck-engine --test live_remote -- --ignored read_only
WYCK_LIVE_REMOTE_TOKEN=... WYCK_LIVE_CONFIRM_DEMO=1 \
    cargo test -p wyck-engine --test live_remote -- --ignored --test-threads=1
cargo test -p wyck-engine --test live_local -- --ignored read_only --nocapture
# Raw server answers, one call per line on stdin (`tools`, `tools <name>`, `get_balance`, ...):
CTRADER_SERVICE=remote CTRADER_REMOTE_TOKEN=... cargo run -p ctrader-mcp --example raw_call
CTRADER_SERVICE=local cargo run -p ctrader-mcp --example raw_call
cargo doc -p wyck-engine --open --all-features
gh run list --branch main --limit 3                         # CI status
```
