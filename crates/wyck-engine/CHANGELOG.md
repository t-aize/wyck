# Changelog

## Unreleased

First version of the engine.

- Normalized domain model with exact integer volumes.
- `Broker` port with Remote and Local adapters and a scriptable `MockBroker`.
- Session supervision: refresh, ping, reconnect with backoff, automatic disarm.
- Immutable state snapshots plus a broadcast event stream.
- Risk-based order planning and non-blocking guardrails.
- Order pipeline: dry-run default, explicit arming, single-use plans, in-flight and interval
  limits, label-based reconciliation, two-step flatten.
- Economic calendar hosting with news warnings for the traded currencies.

### Local account kind (2026-09-20)

- The Local adapter no longer reports `Unknown` for a demo account whose `traderId` is missing from
  `get_accounts_list`: it recognizes the listed account by broker, currency, account type and
  balance to the cent, and only when every matching listed account agrees on `isLive`. Anything
  less certain stays `Unknown`, which a front end treats as possibly live.

### Live validation against demo accounts (2026-09-19)

Fixes that came from running the engine against the real Remote and Local servers.

- **Breaking:** `Volume` now keeps hundredths of a unit (it was whole units), so symbols that
  trade in fractions of a coin (BTCUSD, minimum 0.01) can be sized. `Volume::from_units` and
  `Volume::units` keep their meaning; use `Volume::as_units` for arithmetic. The serialized
  form is the count of hundredths.
- Remote prices: raw quotes are always in units of 1e-5 (not per-symbol `pipDigits`, which the
  server never sends), and position, order and deal prices are display decimals. The adapter
  and the `ctrader-mcp` DTOs now follow that. Before, decoding a position failed and a
  protection change would have sent pipettes as prices.
- Remote price digits are inferred from quotes; the pip size follows from them.
- Remote has no `get_server_time`: `server_time` says so instead of calling a missing tool.
- Local: volumes are sent as `units` with the required `volumeType`, symbol rules are read in
  units, the currency comes from `depositAsset`, the server time from `unixMs`, and the account
  kind is `Unknown` unless the account is in `get_accounts_list`.
- `AccountKind::from_token` reads the real Remote token (base64url JSON), not only JWTs.
- Per-symbol volume rules in `AssumedSpecs::symbols`, reported as `SpecsSource::Configured`.
- An unknown order is now reconciled by symbol, side and volume when the server does not echo
  the label (Remote never does).
- A failed quote read during `Broker::instrument` on Remote is an error instead of a silent
  fallback to 5 decimals.
- Live tests, `#[ignore]`d and env-gated: `tests/live_remote.rs`, `tests/live_local.rs`.
- Local positions carry `stopLossPrice` and `takeProfitPrice`; the decoder read `stopLoss` and
  saw no protection. `place_market_order` answers with a `positionId`, now used.
