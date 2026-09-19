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
