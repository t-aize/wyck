# wyck-trading

The trading model: what an account holds, how an order is sized and protected, and the checks an
order goes through before it is sent. Pure code: no socket, no file, no window.

Depends on: `wyck-core`, `wyck-market`, `serde`, `serde_json`, `tracing`.

## Module map

| Module | What it holds |
|---|---|
| `types` | The account payloads (`Trader`, `Position`, `Order`, `Deal`...), the number enums (`TradeSide`, `OrderType`, `OrderStatus`, `TimeInForce`...), `money`, `volume_units`. |
| `events` | `ExecutionEvent` (the answer to every trading request), `OrderErrorEvent`, `TrailingSlChangedEvent`. |
| `margin` | Expected margin, margin call thresholds, margin events. |
| `contract` | `Contract` (how a symbol trades: lot, volume limits, pip), volume stepping, risk sizing, profit marks, the conversion chain and the account `Summary`. |
| `math` | The ticket's arithmetic on top of `contract`: `SizeMode`, `Offset`, `Scale`, money formatting. It re-exports `contract` (a known wart, see below). |
| `guard` | `RiskPrefs` (the limits the user sets), `check` (an order against the limits), `lock`, `DuplicateGuard`, `Mode` (demo or live), `Cleared` and `clear`/`clear_batch` (the proof an order passed `check`), `client_order_id`. |
| `doubt` | `InDoubt`: a new order whose answer never came, and `find`: what a fresh reading of the account shows of it (working, executed, missing). |
| `plan` | `ExitPlan` (an order cut into exits), `Label` (what an order carries in its label), break-even, OCO, time stop. |
| `follow` | `AtrFollow`: keeping the stop of a pending order on the ATR while it waits. |
| `atr_stop` | `AtrStop`: where a stop loss goes as a multiple of the ATR. |
| `book` | `AccountBook`: the account state kept current from the server's answers and events, and the `Outcome` of an execution event. |

## Invariants and traps

- Volumes are what the server counts: hundredths of a unit. A lot is `Contract::lot_size` of those.
  Money is an integer scaled by `10^moneyDigits` on the wire (`money()` converts). Prices of
  positions and orders are ordinary decimals, unlike the integer prices of bars and ticks.
- Every number enum has `from_number` giving `None` for an unknown number: a server that adds a
  value shows up as a missing label, not as a crash. Keep that.
- `guard` is the one place that decides if an order may be sent. An order that only reduces what is
  open is never blocked: closing must always work. A limit of 0 is off. `RiskPrefs::normalized`
  repairs numbers that are not numbers.
- Everything an order needs to be found again after a restart is in its label (`plan::Label`, with
  `AtrFollow::encode`). The label is a format: old orders carry old labels, so `decode` must keep
  reading what `encode` ever wrote.
- `Outcome` and `TicketProblem` carry codes, not sentences: the words belong to the screen. (A few
  strings written for people are still in `guard` and `book`; they will move out.)
- The ATR limits (`atr_stop::MAX_LENGTH`, `MAX_MULTIPLIER`) are shared by the setting and by the
  label that stores it.

## Known warts (to be fixed in the phase that splits mixed files)

- `math` re-exports all of `contract` (`pub use crate::contract::*`), so `Contract` has two paths.
- `contract` holds several things: the contract itself, risk sizing, profit marks, the conversion
  chain and the account summary. `types` holds the payloads and the number enums.

## Tests

Unit tests next to the code, plus doc-tests on `Contract`, `MarginCall::new` and `money`. The guard and
the risk sizing are the tests that must never be weakened. Run: `cargo test -p wyck-trading`.

## Where to add X

- A field the server sends on an order, position or deal: `types`, with a `flex` reader.
- A new safety limit: `guard::RiskPrefs` (default off or safe), `normalized`, `check`, a test.
- A new kind of exit or label field: `plan`, keeping `Label` decoding backward compatible.

## Do not

- Do not add a request or response type: those belong to `wyck-ctrader`.
- Do not depend on anything above `wyck-market` or on a UI or I/O crate: `cargo xtask deps` fails.
- Do not weaken or bypass `guard::check` for a new way to send an order. The connection asks for a
  `Cleared` (`TradingClient::new_order`), and only `clear` and `clear_batch` make one: do not add a
  constructor for it, not even for a test or an example.
