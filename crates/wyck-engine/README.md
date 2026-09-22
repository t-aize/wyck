# wyck-engine

The headless trading core of wyck. It owns the cTrader connection, keeps an immutable
snapshot of the account, sizes orders from risk, sends them through a safety pipeline, and
warns about trading risk. No user interface code: a GUI, a terminal tool or a
service drives the same `EngineHandle`.

```text
front end  <--- state snapshots, events ---  Engine  --->  Broker (Remote | Local | mock)
           --- async commands --------->
```

## What it gives a front end

- **`EngineState`**: one snapshot with session, account, positions, quotes and warnings.
  Read it, or await `watch_state()` for changes.
- **`Event`**: what changed and why, on a bounded broadcast channel. Lagging subscribers
  resynchronize from the state.
- **`plan_entry`**: "buy EURUSD, stop 30 pips, risk 1%" becomes an exact, validated plan.
  Volume is rounded down and the stop rounded up, so the loss at the stop never exceeds
  the target.
- **`submit`**: dry-run by default. Real orders need `arm`, single-use plans, one order in
  flight per symbol, and are never replayed: a lost reply is settled by reading positions
  back.
- **Guardrails**: warnings for per-trade and total risk, stale data and wide spreads.
  They warn; they never block. The application handles economic news.

## Use

```rust
let engine = Engine::start(EngineConfig::default())?;      // own runtime, real cTrader
let handle = engine.handle();
handle.connect(ConnectRequest::from_profile(&wyck_config, &profile_id)?).await?;

let plan = handle.plan_entry(EntryIntent { /* symbol, side, size, stop, target */ }).await?;
let outcome = handle.submit(plan.id).await?;               // DryRun until you arm
```

Every method can be awaited from any executor, including UI frameworks that are not built on
Tokio. See the crate documentation (`cargo doc -p wyck-engine --open`) for the safety model,
the Remote and Local differences, and the known limitations.

The Open API OAuth helper starts a localhost callback listener and returns the consent URL:

```rust,ignore
let authorization = handle
    .begin_openapi_authorization(client_id, client_secret, 8765)
    .await?;
open_browser(&authorization.url)?;
let grant = authorization.finish().await?;
// Let the user select one of grant.accounts before saving the profile.
```

The Open API broker adapter is not connected to `EngineHandle::connect` yet.

## Examples

```text
cargo run -p wyck-engine --example dry_run_order --features testing   # no network
WYCK_SERVICE=remote WYCK_TOKEN=... cargo run -p wyck-engine --example headless   # read only
```

## Testing

`tests/broker_contract.rs` runs one behavior contract against the mock broker and against the
real Remote adapter talking to a scripted in-process MCP server. `tests/engine_scenarios.rs`
drives the whole engine with paused time (reconnects, lost replies, unknown outcomes, double
taps, flatten). Enable the `testing` feature to use `MockBroker` in your own tests.

## Status

Order placement has not been verified against a live server yet. Use a demo account first.
See `TODO.md` at the repository root.
