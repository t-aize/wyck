# wyck-domain

The domain of the app, one top-level module per area, in this order: `base` (the small shared
vocabulary: number formats, `ta` arithmetic, the look of the interface, the text scale), `market`
(symbols, bars, ticks, quotes, trading hours, `Timeframe`), `trading` (orders, plans, the safety
guard, margin, the account book, `atr_stop`), `drawings` (tools, shapes, the drawing book and its
undo), `indicators` (the indicators, their maths and the Rhai scripts engine) and `chart` (the chart
as a model: series, view, scene, export). **A module names only the modules `MODULES` lists for it**
(`xtask/src/sources.rs`): `base` under all, then the chain `market` < `trading` < `drawings` <
`indicators` < `chart`, each using only some of the ones below it (`drawings` needs `trading`).
Write `crate::market::...` between modules, never `super::super::`. No screen, no file, no network, no `gpui`,
`tokio`, `reqwest` or `keyring`: `cargo xtask deps` fails on them.

Depends on: `serde`, `serde_json`, `toml`, `chrono`, `chrono-tz`, `rhai` (only `indicators::custom`),
`tracing`. Of the workspace nothing.
Used by: `wyck`, `wyck-app`, `wyck-ctrader`, `wyck-ui`.

The text below was written crate by crate before they became modules: a name like `chart` or
`base::math` in it is the module `chart` or `base::math` of this crate.

## What is here

### base

The smallest vocabulary every other crate shares. No domain knowledge, no I/O, no UI toolkit.

### market

Market data as the app understands it: symbols, bars, ticks, quotes, trading hours, timeframes.
Pure code: no file, no socket, no window.

### trading

The trading model: what an account holds, how an order is sized and protected, and the checks an
order goes through before it is sent. Pure code: no socket, no file, no window.

### drawings

The drawings a trader puts on a chart: what a drawing is, how it becomes shapes, how it is made and
changed with the pointer, and the undo history. Pure code: no window, no file, no socket. The chart
is reached through one trait, `geometry::Projection`.

### indicators

The indicators ("studies"): the built-in ones, the settings a user gives them (which is also what is
saved), the arithmetic that turns bars into lines, the catalog the menus read, and the Rhai scripts a
user writes. Pure code: no window, no file, no socket. Reading the scripts folder is done by the app,
which hands the sources to `custom::library`.

### chart

The chart as a model: the prices it holds, what the user set for it, what it draws worked out from
both, the flat list of drawing commands for one frame, and the export of its data. Pure code: no
window, no file, no socket. The painters that turn commands into pixels live in `wyck`.

### market: What lives here and what does not

The types the cTrader Open API sends (symbols, bars, ticks, price and depth events) are read here
as they arrive, so they are also the wire format. The requests that ask for them (`...Req`) and
the response envelopes (`...Res`) live in the crate that talks to cTrader, not here.

### chart: Public API

Flat at the root: `chart::Cmd`, `chart::Series`, `chart::ExportOptions`. Every module
is private and `lib.rs` re-exports what the app uses, group by group. No two public names are the
same, which is what makes this possible; keep it so. Entry points: `Series` and the joins of
`data` (`merge_bars`...), `ChartSettings` (`ChartContent` + `ChartLook`), `Display`, `View`,
`build(&Frame)` for a frame of commands, `table` then `render` for an export, `Zone`.

## Module map

### base

| Module | What it holds |
|---|---|
| `flex` | `serde` readers for integers that arrive as a number or as text (the cTrader JSON gateway sends 64 bit values either way). Every wire type uses `flex::int`, `flex::opt` or `flex::list`. |
| `format` | Number to text: `trim` (no trailing zeros), `grouped` (thousands), `signed`. |
| `math` | The conversions between integer and float types (`i64_to_f64`, `f64_to_i64`, `count_f64`...), each written down once instead of an unmarked `as`: what rounds, what saturates. |
| `ta` | Technical analysis arithmetic on series, for indicators: moving averages, deviations, oscillators. A series in, a series of the same length out, `NaN` where a value is not defined yet. `Series` reads and writes by index without a panic. |
| `defaults` | `yes()`, the `#[serde(default = "...")]` function of a setting that is on when absent. |
| `appearance` | The look as plain data: `Colors` (the one palette), `Look`, the scale bounds `SCALE_MIN` / `SCALE_MAX`, contrast and luminance. |
| `text_scale` | How big the text drawn on a chart is, as a percent. Process wide. |
| `intern` | Names that live for the whole run (`&'static str` for the names of user indicators). Process wide. |

### market

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

Everything is re-exported at the root of the crate (`market::Bar`). The modules are private.

### trading

| Module | What it holds |
|---|---|
| `types` | The account payloads (`Trader`, `Position`, `Order`, `Deal`...), `money`, `volume_units`. |
| `enums` | The number enums the server sends (`TradeSide`, `OrderType`, `OrderStatus`, `TimeInForce`...) and the `number_enum!` macro that makes them. |
| `events` | `ExecutionEvent` (the answer to every trading request), `OrderErrorEvent`, `TrailingSlChangedEvent`. |
| `margin` | Expected margin, margin call thresholds, margin events. |
| `contract` | `Contract` (how a symbol trades: lot, volume limits, pip), volume stepping (`Stepped`, `Limit`), lots and distances in the server's units. |
| `ticket` | What the order ticket works with: `SizeMode`, `Offset`, `Scale`, the pending kind a price makes, `TicketProblem`, `check_protection`. |
| `risk` | Sizing by risk: `quantity_for_risk`, `move_amount`, `lots_for_risk` (the one formula the ticket and the chart share). |
| `profit`, `conversion`, `summary`, `amounts` | A position's profit between two server answers (`PnlMark`, `live_net`); the conversion chain between assets (`chain_rate`); the account `Summary`; money written for people (`format_money`, `format_amount`). |
| `guard` | `RiskPrefs` (the limits the user sets), `check` (an order against the limits), `lock`, `DuplicateGuard`, `Mode` (demo or live), `Cleared` and `clear`/`clear_batch` (the proof an order passed `check`), `client_order_id`. |
| `doubt` | `InDoubt`: a new order whose answer never came, and `find`: what a fresh reading of the account shows of it (working, executed, missing). |
| `plan` | `ExitPlan` (an order cut into exits), `Label` (what an order carries in its label), break-even, OCO, time stop. |
| `follow` | `AtrFollow`: keeping the stop of a pending order on the ATR while it waits. |
| `atr_stop` | `AtrStop`: where a stop loss goes as a multiple of the ATR. |
| `book` | `AccountBook`: the account state kept current from the server's answers and events, and the `Outcome` of an execution event. |

### drawings

| Module | What it holds |
|---|---|
| `model` | `Drawing` (a tool, its points, its look), `Point` (time in ms + raw price), `DrawingsDoc` (the saved file), templates, and the size limits. Submodules: `tool` (`Tool`, its groups and how many points it takes), `style` (line, fill, text, levels), `capabilities` (which properties a tool has), `defaults` (the look a new drawing starts with). |
| `look` | The small pieces of a look beyond line and fill: the dash of a line (`Dash`, re-exported by `model::style`), text layout, line ends (`Caps`), level text, measure and volume-profile looks. |
| `prim` | The vocabulary under every tool's shapes: `Projection` (the chart seen from here), `Prim` (a shape to draw), `P`, `Anchor`, `Part` and the helpers the shapes are built from (`seg`, `lead`, `extended`, `fib`). It depends on nothing above it. |
| `geometry` | The public face of `prim`, plus `prims_with` (drawing -> shapes, by tool), `handles` and the hit tests. Submodules: `shapes`, `measure`, `hit`. |
| `book` | `Book`: the drawings of every symbol plus the pointer state machine (create, pick, hover, move, reshape, undo). Submodules: `create`, `pick`, `edit`, `actions`, `history`, `templates`. |
| `position` | `PositionSettings`: the sizing of a long or short position drawing (risk, leverage, lot step, point value) and its look beyond line and fill (colors, zone opacities, outer lines, tags, whether the zones take a press). |

Private, built on `prim` and reached through `geometry`: `figures` (pitchforks, Gann, patterns, Elliott waves), `extras`
(information lines, regression, ranges, cycles, curves, notes, tables, bars, VWAP, volume) and `decor`
(the words and the line ends put on any drawing). Only `ICONS`, `icon_key` and `wave_names` are
re-exported at the root, for the menus.

### indicators

| Module | What it holds |
|---|---|
| `kind`, `spec` | `StudyKind` (the indicators on offer; the names are written in saved files) and `Spec` (what one is made of: where it is drawn, its inputs, its plots). `kind/specs.rs` and `kind/specs/extended.rs` hold the fixed spec of each built-in. |
| `config` | `StudyConfig`: one indicator on a chart as the user set it, also its saved form. Inputs and plots are named by stable keys, never by position. |
| `style` | How plots, levels and fills look (`PlotStyle`, `LevelStyle`, `FillStyle`). |
| `compute`, `data` | `compute(&StudyConfig, &StudyInput) -> StudyOutput`: bars in, lines out. `data` holds what it reads and gives back (`StudyInput` is the bars as columns of numbers, `StudyOutput` the plots, fills and drawings), re-exported by `compute`; the scripts and the extended indicators build on `data`, not on `compute`. |
| `extended` | The built-in indicators past the first batch (private; reached through `compute`). |
| `higher` | Grouping the chart's bars into a higher timeframe and carrying a value back to the chart's bars. |
| `profile` | The volume profile of the bars on screen (not a line per bar, so not in `compute`). |
| `catalog` | `items()`: every indicator on offer, built-in and scripted, in one list. |
| `custom` | The scripts, in layers (an arrow only goes down): `library` and `behavior` on `run`, which compiles, declares and computes a script under limits and builds the engine; `api` is every function a script can call (`api/` by topic, `declare` for what the calls that describe an indicator share) and `draw_api` its drawings; under them `context` (the run that is going on, reached by `with_run`), `decl` (what a script declares), `color`, `slices` (the `'static` slices a spec holds) and `series` (the value a script works on: a number per bar); `library` is the compiled `Entry` of a file and the global `registry`; `other` aligns the bars of a second symbol; `draw_api` lets a script own chart drawings (`drawings`); `docs`, `assist`, `lexer` serve the editor (reference, completion, colors); `templates` holds the starters (`assets/starters/*.rhai`, compiled in). |

### chart

The modules below are private; a type is reached by its name at the root.

| Module | What it holds |
|---|---|
| `data` | `Series` (bars or ticks, sorted, bounded by `MAX_BARS` / `MAX_TICKS`) and the rules that keep a live chart correct: which bar a tick belongs to, how older history joins (`prepend_bars`), what a reconnect refill replaces (`merge_bars`, `splice_ticks`). |
| `settings`, `look` | `ChartSettings` (the saved form) is `ChartContent` (per chart: type, price scale, indicators) plus `ChartLook` (the same for every chart: grid, crosshair, colors, legend, time zone, lines). `ChartKind` is the 17 chart types. |
| `options` | What a chart shows around its prices: colors it overrides, crosshair, price lines, status line, which trading lines show. |
| `view` | `View`: the window on the data (index space, price scale). The horizontal axis counts points, not time. |
| `display` | `Display`: what is drawn, derived from the prices and the settings once per change (not per frame): the series laid out, and the output of every indicator. |
| `transform` | The chart types that are not one bar per period: Renko, line break, Kagi, point and figure, range. Each filters prices into its own sequence of bricks, lines, columns or bars. |
| `order_flow` | The chart types that look inside a bar, in four private modules: `volume` (volume candles and volume bars), `tpo` (market profile sessions), `footprint` (the settings and the analysis of one bar: point of control, value area, imbalances, stacks), `flow` (`Flow`, the order flow inferred from quotes). Their drawing is in `scene`. |
| `scene` | `build(&Frame) -> Vec<Cmd>`: the data, the view and the settings as drawing commands. `cmd` is the command set, `scale` the bands (the price maps are `price`, a module of their own), `palette` the colors; `bands` draws the prices band, the indicator panes and the crosshair, `labels` the axes over them; `candles` (what the other chart types fall back to), `series`, `studies`, `drawings`, `axes`, `tpo`, `footprint` draw one thing each (private). |
| `replay` | The pure core of the replay: `ReplayClock` (a cursor moved by play, step and seek, from wall times it is given; a step is `ClockStep`: a grid of a step that divides a day, a relative step in time, or a number of ticks), `ReplayStep` and `parse_step` (the steps of the menu, a custom one typed as `7 min`, `90s` or `50 ticks`), `ReplayFeed` (writes into a series the candles shown at a cursor: server bars that ended, the candle in progress rebuilt from minute bars by `replay_fold_minutes` or along `replay_partial_bar`'s synthetic path), `replay_random_start`, `replay_align_to_bar`. With real ticks the candle in progress is the minute bars that ended plus the ticks of the minute in progress (`replay_fold_hybrid`, never the bar of that minute), and a tick chart or a seconds chart is written by `ReplayTickFeed` and `ReplaySecondsFeed` from the ticks before the cursor. |
| `projection` | `ChartProjection`: the chart seen as a `drawings::geometry::Projection`, so drawings follow it. |
| `axis`, `zone`, `lines` | Round tick values and time labels; `Zone` (UTC, local or named time zone); `LineId` (what a horizontal line stands for). |
| `export` | `table` picks rows and columns into a typed `Table`; `render` writes it (delimited text, JSON, JSON lines, Markdown, SQL); `options`, `choices`, `columns`, `source`, `presets` describe what the user can ask for. |

## Invariants and traps

### base

- Series passed together to a `ta` function must have the same length. A shorter one reads as
  gaps (`NaN`) past its end instead of panicking, but the result is then meaningless: a caller that
  takes a series from outside (a script) compares the lengths first.
- A `NaN` in a series is a gap: averages restart after it. Keep outputs aligned index for index
  with the bars they were computed from.
- `ta` is here, and not in `indicators`, because `trading` needs `atr` below the
  indicators. Do not add a function here that only indicators use; add it to `indicators`.
- `flex` refuses what is not a whole number (`1.5`, `"x"`, a `u64` above `i64::MAX`) instead of
  rounding it.

### market

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

### trading

- Volumes are what the server counts: hundredths of a unit. A lot is `Contract::lot_size` of those.
  Money is an integer scaled by `10^moneyDigits` on the wire (`money()` converts). Prices of
  positions and orders are ordinary decimals, unlike the integer prices of bars and ticks.
- Every number enum has `from_number` giving `None` for an unknown number: a server that adds a
  value shows up as a missing label, not as a crash. Keep that.
- A replay locks new orders through the guard itself (`Standing::replay`, `Lock::Replay`), like the kill switch: closing and reducing orders still pass, and a limit of 0 does not turn it off.
- `guard` is the one place that decides if an order may be sent. An order that only reduces what is
  open is never blocked: closing must always work. A limit of 0 is off. `RiskPrefs::normalized`
  repairs numbers that are not numbers.
- Everything an order needs to be found again after a restart is in its label (`plan::Label`, with
  `AtrFollow::encode`). The label is a format: old orders carry old labels, so `decode` must keep
  reading what `encode` ever wrote.
- `Outcome`, `OrderLine`, `TicketProblem`, `guard::Refusal`, `guard::Warning` and `guard::Lock` carry
  codes and the numbers behind them, not sentences: the words are in `wyck-app`
  (`account::notice`). Keep it so; a sentence here would have to be kept in step in two places.
- The ATR limits (`atr_stop::MAX_LENGTH`, `MAX_MULTIPLIER`) are shared by the setting and by the
  label that stores it.

### drawings

- A point is a time and a price, never a screen position, so a drawing stays put when the chart is
  scrolled, zoomed or shown on another timeframe. Screen positions exist only inside `geometry` and
  `book`, and only through a `Projection`.
- The saved form is a promise. `Tool` names are written in files (`Tool::code`) and never change;
  an unknown name loads as `Tool::Unknown` and the drawing is dropped by `is_valid`, so a file from
  a newer version still opens. Every field has `#[serde(default)]`; a look piece at its default is
  left out of the file. Raising `SCHEMA_VERSION` needs a step in the app's migrations (a test there
  fails otherwise).
- `DrawingsDoc::normalized` and `Drawing::normalized` are what makes a file safe: finite prices,
  the right number of points for the tool, unique ids, `next_id` past every id, the limits below.
  Anything read from disk or pasted goes through them.
- Limits against a runaway file: `MAX_DRAWINGS_PER_SYMBOL`, `MAX_BRUSH_POINTS`, `MAX_LEVELS`, 64 points
  in an arrow path, 200 steps of undo.
- `Book` has no idea of a window: a press, a move and a release come in with a `Projection`, and
  drawings come out. Keep it that way, it is what makes the fiddly rules testable. The rule for placing
  points is written at the top of `book.rs`; read it before touching `create`.
- A grip is on what is drawn. `geometry::handles` gives a tool whose last point only sets a size or an offset (`grips_may_leave_shape`: channels, rotated rectangle, wedge, cycles, regression) a grip placed on a drawn line where a press changes nothing, and `book/edit.rs` keeps the third point of a channel inside the span of its lines while it is placed or dragged. A new tool of that kind goes in both, and in the test `a_grip_stays_on_what_is_drawn_even_when_its_point_is_far_from_it`.
- Every drawing under the pointer offers its grips (`Book::hit_resolved`), not only the selected one: the chart shows them on hover. `Book::pick_resolved` (the selected one's grips first, then the topmost drawing) is what both a press and the hover ask, so the cursor never disagrees with what a press does.
- A replay sets a volatile `Book::horizon` (never saved, never a change worth saving): `Book::shows` is the one test of whether a drawing shows on a timeframe now, and it hides the drawings whose first anchor is after the horizon, from the screen, the pick and the box select alike. Use it, not `Drawing::shows_on`, wherever a drawing is shown or picked.
- `Book` is shared by every chart. `hold` / `release_hold` / `held_elsewhere` say which chart holds the button, so that a second chart of the same symbol does not move a drawing with its own projection. Escape during a drag puts the drawing back (`Book::cancel`).
- The tags of a position show while the pointer is over it (or always, with `always_stats`), so they are part of it for the hit tests. When the zones are shorter than a tag, or the box narrower than its words, the three tags become one, beside the box (`position_prims`), which is not part of it.
- A position takes a press in its zones only if they are painted and `grab_zones` is on; otherwise its lines, edges and grips do. A new position starts small (a few percent of the price span, 2R) or like the last one edited (`position_shape`, kept for the session, not saved).
- The money on a position drawing is a plan, not a quote: no live rate. The quantity comes from
  `trading::contract::quantity_for_risk`, the same formula as the order ticket. Do not copy it.
- Raw prices (what the chart holds) and real prices (what a trader reads) differ; only
  `Projection::real_price` converts, and only the tools that work out money should call it.

### indicators

- The saved form is a promise. `StudyKind` names, input keys and plot keys are written in files and
  never change; an unknown kind loads as `StudyKind::Unknown` and is dropped, a missing input takes
  its default, an unknown one is ignored. `StudyConfig::normalized` repairs a file.
- A built-in output stays aligned with the input bars: one value per bar, `NaN` where there is no
  value yet. The arithmetic itself lives in `base::ta`; this crate wires it.
- A script is bounded. `Limits` caps the operations (20 million) and the wall time (4 s) of a run,
  and a declaration (a run on 0 bars) gets far less. The engine has no modules and no time, and
  nothing a script can call reaches the file system, the network or the clock. Keep it that way.
- A script runs twice: once on no bars to declare what it is (`indicator`, `input_*`, `plot`, `hline`,
  `fill`), once on the bars to compute. So `plot` and `input_*` must run unconditionally, only the
  values may depend on an input, and nothing may draw when `n == 0`.
- A Rhai `fn` cannot see the script's globals (`close`, `n`, `na`, top-level `const`s): they are
  arguments. A starter that forgets this fails with "Variable not found".
- The list of functions described for the editor (`custom/docs.rs`) and what the engine registers
  (`custom/api`) are checked against each other by a test: a function cannot be added without being
  described, nor described without existing.
- Visible overlay plots count in the price autoscale of the chart, so a plot far from the price
  squashes the chart; drawings do not. Script drawings get id 0 and cannot be an alert source.
- A script id is its path from the scripts folder without the extension (`trend/my_average`), with
  `/` between folders. A chart saves that id, so a rename breaks the charts that hold it (they show
  a "missing" spec, not a crash).

### chart

- The saved form is a promise. Every field of `ChartSettings` has a default and `normalized`
  repairs what was read, so a file from another version loads. `ChartKind::code` names are written
  in files and never change. The look is saved once for all charts, the content once per chart.
- Prices are the server's raw integers (`market::PRICE_SCALE`). They stay integers until the
  last step: `PriceMap` is for the screen, export writes them by integer arithmetic so a price
  never picks up the error of a float.
- The work of a frame is bounded by the screen, never by the data: only the points that reach the
  plot are visited, and points narrower than a pixel are folded into one column (low, high, first,
  last). Keep that when adding a chart type, a hundred thousand bars must cost a screenful.
- Drawings never hide the candles: their fills, lines and tags are painted under the prices. Only the grips (of the selected drawings and of the hovered one) go over them (`scene/drawings.rs` returns that layer to `bands::prices`), since a candle over a grip makes it hard to use. The tags of a position show only on the drawing the pointer is over or dragging in this chart (`DrawingView::hovered`), so a position seen on another chart, or ten of them, add nothing to read.
- `Display` is computed on a change of prices or settings, a frame only reads it. Do not compute
  indicators or transforms inside `scene`.
- `scene` makes commands (`Cmd`) in window coordinates, it names shapes, not a graphics library. It
  must stay testable without a window.
- `Flow` is an estimate: the Open API has no trade tape, so a unit of volume is a quote that moved
  the middle of the spread, a buy if up and a sell if down, put on the bid price. The legend says
  so. Do not present it as traded size.
- Replay knows the past only: a cursor `c` shows what happened strictly before `c` (a bar is seen
  when `time < c`, ended when `c >= its end`). A tick at time `t` is seen when `t < c` too, so a step in ticks lands on `t + 1` and ticks of the same millisecond show together. With real ticks the bar of the minute in progress is never read, not even its open: it holds the price of ticks that have not happened yet. The synthetic path inside a minute is built from the
  minute's final range, so it is plausible, not real; the default one-minute step never shows it.
  `ReplayFeed::reveal_into` after any sequence of moves must equal a rebuild from scratch (a test
  checks it); keep it so when changing the grouping.
- A series is bounded (`MAX_BARS`, `MAX_TICKS`) and sorted by time: every join goes through the
  functions of `data`, which keep both true.

## Global state

### base

- `text_scale`: one `AtomicU32`, set by the interface when the user changes the scale, read by the
  scene of every chart. A picture of a chart is made at the scale in force when it is taken.
  Tests must not set it: other tests read it.
- `intern`: a `Mutex` around a table of leaked strings, capped at 4 MiB. The same text is stored
  once; past the cap `name()` returns `(too many names)`.

### drawings

- `geometry::label_size` and `prims_with` read `base::text_scale`, set by the interface. A test
  that depends on the size of text must not set it: other tests read it.
- `Style::face` interns a font name with `base::intern` (leaked, capped).

### indicators

- `custom::library::registry`: an `RwLock` around the compiled scripts, which every chart looks
  indicators up in. The app publishes the whole list with `install`. Tests use the test-only
  `insert` so they do not wipe each other's scripts.
- A spec holds `'static` slices, so what a script declares (inputs, plots, choices) is leaked once per
  distinct content in tables capped at 20 000 slices each (past it an empty slice is handed out),
  and names go through `base::intern`.

### chart

- `base::text_scale`: the size of the text and of the axes in `scene` (`font`, `axis_w`,
  `axis_h`, `zone_w`...) is scaled by a global the interface sets. A picture is made at the scale in
  force. Tests must not set it: other tests read it.
- `zone`: a `thread_local` cache of the last UTC offset asked for (zone, quarter of an hour,
  offset). It only avoids recomputing the offset for every bar of a frame.

## Tests

### base

Unit tests sit next to the code (`#[cfg(test)]`). `ta` is checked against values worked out by
hand. Run: `cargo test -p wyck-domain base::`.

### market

Unit tests next to the code. The decoders have tests shaped like real answers. Run:
`cargo test -p wyck-domain market::`.

### trading

Unit tests next to the code, plus doc-tests on `Contract`, `MarginCall::new` and `money`. The guard and
the risk sizing are the tests that must never be weakened. Run: `cargo test -p wyck-domain trading::`.

### drawings

Unit tests next to the code. Geometry and tool shapes are tested with the linear `Projection` of
`geometry::tests` (`Linear`, `drawing`), so nothing needs a chart. The save/load round trip of a
look and the rules of `Book` (which click places which point, what undo restores) are the tests
that must not be weakened. Run: `cargo test -p wyck-domain drawings::`.

### indicators

Unit tests next to the code, `src/tests.rs` for the ones that cross modules (every indicator on one
set of bars), `custom/behavior.rs` for scripts end to end (each function against its arithmetic),
and the tests of `custom/templates.rs`, which compile and compute every starter (on 0, 1 and 600
bars, every choice and every switch of each). Math is checked against worked examples:
those and the `docs` cross-check are the tests that must not be weakened. Run:
`cargo test -p wyck-domain indicators::`. If a script test fails by timeout on a loaded machine, rerun it
alone before blaming a change.

### chart

Unit tests next to the code; the scene is tested on commands, the data rules on plain vectors, the
export on typed tables. The data joins (`merge_bars`, `prepend_bars`, `splice_ticks`), the
saved-form round trips of `settings` and the integer price writing of `export` are the tests that
must not be weakened. Run: `cargo test -p wyck-domain chart::`.

## Where to add X

### base

- A number format: `format`. A new serde reader for the wire: `flex`.
- A new indicator formula: `indicators` unless `trading` needs it too.
- A color or an interface constant shared by the app and the design system: `appearance`.

### market

- A new field the server sends: add it to the payload type with a `flex` reader.
- A new request or response: in `wyck-ctrader`.
- A rule about a timeframe or a period: `timeframe` / `bars`.

### trading

- A field the server sends on an order, position or deal: `types`, with a `flex` reader.
- A new safety limit: `guard::RiskPrefs` (default off or safe), `normalized`, `check`, a test.
- A new kind of exit or label field: `plan`, keeping `Label` decoding backward compatible.

### drawings

- A new tool: a variant of `Tool` (a new name, never a rename), its group, label and `anchors` in
  `model/tool.rs`, its capabilities and default look in `model/`, its shapes in `figures` or
  `extras`, its handles in `geometry/hit.rs` (on the shape, see above), and tests for geometry and for the saved round trip.
- A new property of a look: a field with `#[serde(default)]` in `model/style.rs` or `look.rs`,
  skipped when at its default, with `normalized` keeping it in range.
- A new rule for the pointer: `book/` (`create`, `edit`, `pick`), tested through `Book`.

### indicators

- A built-in indicator: a `StudyKind` variant (a new name), its spec in `kind/specs.rs` or
  `kind/specs/extended.rs`, its arithmetic in `base::ta` when it is a general one, its branch in
  `compute`, its entry in `catalog`, a test on the shared set of bars.
- A function a script can call: `custom/api/` (thin: it hands the numbers to `base::ta`),
  its description in `custom/docs.rs`, a test in `custom/behavior.rs`.
- A starter: a `.rhai` file in `assets/starters/`, an entry in `custom/templates.rs`. It must be
  ASCII, compile, and compute. Its name becomes a file name (letters, digits, spaces, `- _ ( )`).
  The ten starters are the user's tour of the language: when a function or an option
  is added, show it in one of them.

### chart

- A chart type: a `ChartKind` variant with a new code, its settings in `settings` (default, saved
  form), its sequence in `transform` or its own module, its drawing in `scene`, its export in
  `export`, and a test for the saved form.
- A setting of the look or of one chart: `look`/`settings` (or `options`), a default,
  `normalized`, then `scene` reads it.
- An export format or column: `export/render.rs`, `export/columns.rs`, with `choices` for the menu.

## Do not

### base

- Do not add a dependency on another crate of the workspace, on `gpui`, `tokio` or any I/O crate:
  `cargo xtask deps` fails.
- Do not put a thing here because two modules of one crate use it: it belongs in that crate.

### market

- Do not add a request or response type here.
- Do not depend on `trading` or anything above: `cargo xtask deps` fails.
- Do not put pixels or colors here (the default width of a bar is a screen concern).

### trading

- Do not add a request or response type: those belong to `wyck-ctrader`.
- Do not depend on anything above `market` or on a UI or I/O crate: `cargo xtask deps` fails.
- Do not weaken or bypass `guard::check` for a new way to send an order. The connection asks for a
  `Cleared` (`TradingClient::new_order`), and only `clear` and `clear_batch` make one: do not add a
  constructor for it, not even for a test or an example.

### drawings

- Do not use `gpui`, `tokio`, files or the network: the crate makes shapes (`Prim`), the chart paints
  them. `cargo xtask deps` fails on such a dependency.
- Do not store a screen position in a drawing.
- Do not rename a `Tool` variant or a serde field without an alias and a migration.
- Do not depend on `indicators` or `chart`: the arrow goes the other way.

### indicators

- Do not read the scripts folder or any file here: that is the app's job, this crate gets sources.
- Do not give a script a way out: no file, network, clock or module in the engine.
- Do not rename a `StudyKind` variant, an input key or a plot key without an alias and a migration.
- Do not use `rhai` anywhere but this crate (`cargo xtask deps` fails), nor `gpui`, `tokio` or files.
- Do not depend on `chart` or anything above: the arrow goes the other way.

### chart

- Do not use `gpui`, `tokio`, files or the network: `cargo xtask deps` fails on them. A painter
  belongs in `wyck`.
- Do not make a module public or give a new public item a name already taken: re-export it flat in
  `lib.rs`, and pick another name if the name clashes.
- Do not store a screen position in a saved type.
- Do not rename a `ChartKind` code or a serde field without an alias and a migration.
- Do not depend on `trading`, `wyck-storage` or anything above: the arrow goes the other way.
