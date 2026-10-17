# wyck-chart

The chart as a model: the prices it holds, what the user set for it, what it draws worked out from
both, the flat list of drawing commands for one frame, and the export of its data. Pure code: no
window, no file, no socket. The painters that turn commands into pixels live in `wyck`.

Depends on: `wyck-core`, `wyck-market`, `wyck-drawings`, `wyck-indicators`, `chrono`, `chrono-tz`,
`serde`, `serde_json` (`toml` in tests only).

## Public API

Flat at the root: `wyck_chart::Cmd`, `wyck_chart::Series`, `wyck_chart::ExportOptions`. Every module
is private and `lib.rs` re-exports what the app uses, group by group. No two public names are the
same, which is what makes this possible; keep it so. Entry points: `Series` and the joins of
`data` (`merge_bars`...), `ChartSettings` (`ChartContent` + `ChartLook`), `Display`, `View`,
`build(&Frame)` for a frame of commands, `table` then `render` for an export, `Zone`.

## Module map

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
| `scene` | `build(&Frame) -> Vec<Cmd>`: the data, the view and the settings as drawing commands. `cmd` is the command set, `price`/`scale` the price maps and bands, `palette` the colors; `bands` draws the prices band, the indicator panes and the crosshair, `labels` the axes over them; `series`, `studies`, `drawings`, `axes`, `tpo`, `footprint` draw one thing each (private). |
| `projection` | `ChartProjection`: the chart seen as a `wyck_drawings::geometry::Projection`, so drawings follow it. |
| `axis`, `zone`, `lines` | Round tick values and time labels; `Zone` (UTC, local or named time zone); `LineId` (what a horizontal line stands for). |
| `export` | `table` picks rows and columns into a typed `Table`; `render` writes it (delimited text, JSON, JSON lines, Markdown, SQL); `options`, `choices`, `columns`, `source`, `presets` describe what the user can ask for. |

## Invariants and traps

- The saved form is a promise. Every field of `ChartSettings` has a default and `normalized`
  repairs what was read, so a file from another version loads. `ChartKind::code` names are written
  in files and never change. The look is saved once for all charts, the content once per chart.
- Prices are the server's raw integers (`wyck_market::PRICE_SCALE`). They stay integers until the
  last step: `PriceMap` is for the screen, export writes them by integer arithmetic so a price
  never picks up the error of a float.
- The work of a frame is bounded by the screen, never by the data: only the points that reach the
  plot are visited, and points narrower than a pixel are folded into one column (low, high, first,
  last). Keep that when adding a chart type, a hundred thousand bars must cost a screenful.
- `Display` is computed on a change of prices or settings, a frame only reads it. Do not compute
  indicators or transforms inside `scene`.
- `scene` makes commands (`Cmd`) in window coordinates, it names shapes, not a graphics library. It
  must stay testable without a window.
- `Flow` is an estimate: the Open API has no trade tape, so a unit of volume is a quote that moved
  the middle of the spread, a buy if up and a sell if down, put on the bid price. The legend says
  so. Do not present it as traded size.
- A series is bounded (`MAX_BARS`, `MAX_TICKS`) and sorted by time: every join goes through the
  functions of `data`, which keep both true.

## Global state (on purpose)

- `wyck_core::text_scale`: the size of the text and of the axes in `scene` (`font`, `axis_w`,
  `axis_h`, `zone_w`...) is scaled by a global the interface sets. A picture is made at the scale in
  force. Tests must not set it: other tests read it.
- `zone`: a `thread_local` cache of the last UTC offset asked for (zone, quarter of an hour,
  offset). It only avoids recomputing the offset for every bar of a frame.

## Tests

Unit tests next to the code; the scene is tested on commands, the data rules on plain vectors, the
export on typed tables. The data joins (`merge_bars`, `prepend_bars`, `splice_ticks`), the
saved-form round trips of `settings` and the integer price writing of `export` are the tests that
must not be weakened. Run: `cargo test -p wyck-chart`.

## Where to add X

- A chart type: a `ChartKind` variant with a new code, its settings in `settings` (default, saved
  form), its sequence in `transform` or its own module, its drawing in `scene`, its export in
  `export`, and a test for the saved form.
- A setting of the look or of one chart: `look`/`settings` (or `options`), a default,
  `normalized`, then `scene` reads it.
- An export format or column: `export/render.rs`, `export/columns.rs`, with `choices` for the menu.

## Do not

- Do not use `gpui`, `tokio`, files or the network: `cargo xtask deps` fails on them. A painter
  belongs in `wyck`.
- Do not make a module public or give a new public item a name already taken: re-export it flat in
  `lib.rs`, and pick another name if the name clashes.
- Do not store a screen position in a saved type.
- Do not rename a `ChartKind` code or a serde field without an alias and a migration.
- Do not depend on `wyck-trading`, `wyck-storage` or anything above: the arrow goes the other way.
