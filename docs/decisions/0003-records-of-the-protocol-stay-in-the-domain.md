# 0003: Positions, orders and deals stay the records of the protocol

- Status: accepted
- Date: 2026-10-09

## Context

`domain::trading::types` holds `Position`, `Order` and `Deal` as the serde records the server
sends, with raw integers for the side, the status and the type, and a typed accessor next to each
(`side()`, `status()`). The audit found two risks in that: a side the app does not know was read
as a buy, and the same account maths existed twice (the order ticket and the position tool).

A second set of domain structs behind a mapping layer in `infra::ctrader` would remove the raw
integers from the screens. It would also mean converting every record twice, keeping two sets of
fields in step with the protocol, and rewriting the 60 or so places that read them, with no screen
to try the result on.

## Decision

Keep one set of records. Check them where they enter the app instead:

- `AccountBook` leaves out a position, an order or a deal whose side it cannot read (and logs it),
  so everything taken from the book has a side. `TradeData::is_buy` and `Deal::is_buy` replace the
  free function that took an unknown side for a buy.
- Enumerations of the protocol are built from their enum (`TimeInForce`, `TradeSide`,
  `OrderType`), not from numbers written at the call site.
- Risk sizing is one formula, `contract::quantity_for_risk`, used by the ticket and the position
  tool. The pip of a symbol comes from `market::pip_size`; the guess from decimals
  (`pip_size_from_digits`) is only for the moment before a contract is known.

## Consequences

Cheaper and safer than a second model, and the protocol types stay where the tests are. The cost
is that the screens still see wire shaped records. Revisit if the protocol changes shape often, or
if a second broker connection is added: then a domain model is worth its price.
