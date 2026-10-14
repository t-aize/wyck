# wyck-market

Market data as the app understands it: symbols, bars, ticks, quotes, trading hours, timeframes.
Pure code: no file, no socket, no window.

Depends on: `wyck-core`, `serde`, `chrono`, `chrono-tz`.

## What lives here and what does not

The types the cTrader Open API sends (symbols, bars, ticks, price and depth events) are read here
as they arrive, so they are also the wire format. The requests that ask for them (`...Req`) and
the response envelopes (`...Res`) live in the crate that talks to cTrader, not here.

## Module map

| Module | What it holds |
|---|---|
| `price` | `PRICE_SCALE` (100 000), `to_price`, `from_price`, `format_price`, `pip_size`. |
| `bars` | `Period` (the 14 server periods), `Bar`, the wire form `WireTrendbar` and its decoding. |
| `ticks` | `Tick`, `Quote`, `QuoteType`, the wire form `WireTick`, `decode_ticks`, `merge_sides`. |
| `quotes` | `SpotEvent` (a live price event, with live bars) and `Spot`. |
| `live` | `LiveBarTracker`: gives live bars the close the server forgets to send. |
| `depth` | `DepthEvent` and `DepthQuote`, the order book events. |
| `symbols` | `LightSymbol`, `Symbol` (details), `Asset`, `AssetClass`, `SymbolCategory`, `SymbolChangedEvent`. |
| `hours` | `TradingHours::from_symbol(..).status_at(now_ms)` gives a `MarketStatus`. |
| `timeframe` | `Timeframe` (ticks, seconds, server bars, multiples), `Unit`, the menu `GROUPS` and `QUICK`. |

Everything is re-exported at the root of the crate (`wyck_market::Bar`). The modules are private.

## Invariants and traps

- A price is an integer scaled by `PRICE_SCALE`. Keep the integer for exact work and comparisons;
  `to_price` is for display and arithmetic that tolerates rounding.
- Bar and tick times are Unix milliseconds. On the wire a bar time is in minutes: `decode_bar`
  converts and drops a bar whose numbers do not hold together (`Bar::is_sane`).
- Ticks come newest first, and both their time and their price are differences from the tick
  before (confirmed on a live demo account). `decode_ticks` returns them oldest first.
- The server sends the close of a live bar equal to its low. Always run live bars through
  `LiveBarTracker`.
- `Timeframe::code()` is a file format: the codes (`T`, `S15`, `M5`, `H2`, `MN3`...) are written in
  saved layouts. Never change one; the same length always gets the same code.
- Trading hours are seconds from Sunday 00:00 in the schedule's time zone; holidays are days since
  1970-01-01 and may recur every year.

## Tests

Unit tests next to the code. The decoders have tests shaped like real answers. Run:
`cargo test -p wyck-market`.

## Where to add X

- A new field the server sends: add it to the payload type with a `flex` reader.
- A new request or response: in `wyck-ctrader`.
- A rule about a timeframe or a period: `timeframe` / `bars`.

## Do not

- Do not add a request or response type here.
- Do not depend on `wyck-trading` or anything above: `cargo xtask deps` fails.
- Do not put pixels or colors here (the default width of a bar is a screen concern).
