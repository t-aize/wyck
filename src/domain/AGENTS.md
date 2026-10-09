# domain

Pure logic: market data, trading maths, indicators, drawings, chart models. Read the layer table
in `docs/architecture.md` first.

- Imports: `domain` only. Never gpui, tokio, reqwest, keyring, rodio, `std::fs`, `std::net`,
  `std::env`, `std::process`. `tests/architecture.rs` fails on them.
- No user-facing text that belongs to a screen. Error codes stay here, sentences go to `ui`.
- Time comes in as an argument; do not call `now()` inside a calculation.
- A calculation is a function with a unit test (reference values for indicators).
- Wire shapes of the cTrader protocol live in `infra::ctrader`, not here, except the plain data
  types that both sides share (`market`, `trading::types`).
