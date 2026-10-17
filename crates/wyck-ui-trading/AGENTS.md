# wyck-ui-trading

The trading screens of the desktop app: the order ticket (`ticket`) where an order is put together
and sent, the account panel under the charts (`panel`) with its tabs of positions, orders, trades,
exposure and alerts, and the lines (`lines`) that show orders, positions and alerts on the charts.
The account, the safety limits and the arithmetic of lots, pips and risk are not here: they are
`wyck-app` (`account`, `alerts`) and `wyck-trading` (`math`, `plan`, `book`).

Depends on: `wyck-app`, `wyck-chart`, `wyck-core`, `wyck-drawings`, `wyck-indicators`,
`wyck-market`, `wyck-trading`, `wyck-ui-chart`, `wyck-ui-kit`, `gpui`, `chrono` (the time of a
trade) and `serde_json` (the extra fields of an order); `toml` for the tests of the saved layouts.
Used by: `wyck-ui-shell`.

## Module map

| Module | What it holds |
|---|---|
| `lines` (`lib.rs`) | `lines(book, alerts, profit, currency)`: the `ChartLine`s of every symbol (positions with their profit, protections, working orders, active alerts). The shell hands them to the multichart. |
| `ticket` (`ticket.rs`) | `OrderTicket`, `TicketEvent`; the fields, the plan they add up to (`Plan`), `prefill` (what the chart asks of the ticket) and `same_shape`. |
| `ticket::{build, edit, planning, helpers}` | Building the fields and the subscriptions; what a click changes; the plan, the lines the ticket puts on the chart and the margin asked; the small arithmetic and formats. |
| `ticket::{send, problem}` | Turning the plan into orders, the safety check and the confirmation; the sentence for each `TicketProblem`. |
| `ticket::{link, templates}` | Following a position drawing of the chart (`PositionLink`, `PositionPlan`); the setups saved under a name and the time stop. |
| `ticket::{customize, view}` | The panel that customizes the ticket; how the ticket looks (one file per block under `view/`). |
| `panel` (`panel.rs`) | `AccountPanel`, `PanelEvent`; `open_alert` is public. |
| `panel::{data, view, stats, dialogs}` | Every row of every tab worked out from the account (`data`, with one file per tab); how the panel looks; small computations; the dialogs that modify a position or an order and the one that edits an alert. |

## Invariants and traps

- **The ticket decides nothing about risk.** The plan comes from `wyck_trading::ticket` and `wyck_trading::risk` (sizing,
  risk, margin) and from the fields; what stops an order is a `TicketProblem` that `problem::text`
  words.
- **The safety limits come before the confirmation.** `send` asks the account to `assess_batch` the
  orders: a hard limit (`Verdict::Block`) refuses with a toast before anything opens; a warning, or a
  risk above the share the user set (`layout.high_risk`), goes into the confirmation. One-click
  trading sends at once only when there is nothing to warn about. The limits themselves are in
  `wyck-app`; do not copy them here.
- **A new order leaves with `place_batch` on the account, nowhere else.** Closing, reversing,
  trailing and protecting a position also go through methods of the account. The only direct use of
  the broker session is the read of margins in `ticket::planning`.
- **A ticket linked to a position drawing follows it** until the order is sent or the user unlinks:
  the stop of an ATR stop and the take profit of a multiple of the risk are worked out by the ticket
  and are read-only (`same_shape` ignores them when it asks whether the drawing changed).
- **Money comparisons are exact only where numbers are copied**, never computed (`same_shape`, the
  one `#[expect(float_cmp)]`). Sizes and prices are converted with the helpers of
  `wyck_core::math`, not with `as`.
- **No `[]` on a slice or a vector**: the workspace denies it. Use `get`, a slice pattern or an
  iterator; a control that picks by position goes through `Kind::ALL.get(index)` and does nothing when the
  position is out of range.

## Global state

None. The layout of the ticket and the tab of the panel are preferences in `wyck-app` (`prefs`),
handed back to the shell through `TicketEvent::Settings` and `PanelEvent::Settings`.

## Tests

19 unit tests next to the code: the arithmetic and formats of the ticket (`helpers`), the saved
setups (`templates`), the rows and totals of the tables (`panel::data`), the small computations
(`panel::stats`) and the lines. The screens themselves are not unit tested. Run:
`cargo test -p wyck-ui-trading`. The guards that read the sources of the screens live in `wyck`.

## Where to add X

- A block in the ticket: its view in `ticket/view/<block>.rs`, its place in the layout of
  `wyck_app::prefs::ticket::Layout`, its page in `ticket/customize`.
- A kind of order or a protection: the order itself in `wyck-trading` and the account in `wyck-app`;
  here the fields (`build`), the click (`edit`), the plan (`planning`) and the order (`send`).
- A column or a tab of the panel: `panel::data` (the cells) and `Tab` in `wyck_app::prefs::panel`.
- A line on the chart: `lines()` for what the account holds, `ticket::planning` for the ticket's own.

## Do not

- Do not add a way to send an order that skips `assess_batch` and the confirmation.
- Do not work out risk, margin or lots here: `wyck_trading::ticket` and `wyck_trading::risk` do.
- Do not reach the network, the files or the system: go through `wyck-app`.
- Do not re-export a type of another crate: import it from its owner.
