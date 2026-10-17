# wyck-ui-chart

The price chart of the desktop app and the screen that arranges several of them: the chart itself
(`chart`) with its drawing tools, indicators, order lines, settings panels and exports, and the
multichart (`multichart`) that holds the charts of a layout, links them and hosts the drawing rail.
What a frame looks like is decided by `wyck-chart` (plain drawing commands); this crate turns the
commands into pixels, handles the pointer and the keys, and builds the panels around the canvas.

Depends on: `wyck-app`, `wyck-chart`, `wyck-core`, `wyck-drawings`, `wyck-indicators`, `wyck-market`,
`wyck-trading`, `wyck-ui-kit`, `gpui`, `ab_glyph` and `tiny-skia` (the PNG of `chart::raster`),
`thiserror`, `tracing`.
Used by: `wyck-ui-trading`, `wyck-ui-editor`, `wyck-ui-shell`.

## Module map

Two public modules, `chart` and `multichart`; every file below them is private except the few named
in the last column.

| Module | What it holds |
|---|---|
| `chart` (`chart.rs`, `api.rs`) | The `Chart` entity: its state, what it shows and the calls the rest of the app makes on it (`api`). The key actions (`ChartPanBack`, `ToolTrendLine`...) are declared here, in the module `action`. |
| `chart::{history, input, follow, glue, paint, overlay, kinds}` | Loading the history and live prices; the pointer and the keys; following another chart; the drawings (presses and moves go to the drawing book of `wyck-app`); the canvas that executes a frame; what sits over it (legend, toolbar, menus, status). |
| `chart::{flow_sync, custom_runs}` | The order flow of a footprint chart (loaded around the bars on screen); the indicators written as scripts, run off the interface thread. |
| `chart::{chart_settings_ui, footprint_ui, tpo_ui, volume_ui, construction_ui, settings_rows}` | The settings panel of the chart and the part of each chart type. `chart_settings_ui::open` is public. |
| `chart::{study_settings, indicator_picker, header_menus}` | The settings of one indicator, the list to add one, the menus of the header. |
| `chart::{drawing_props, object_tree, tool_icon}` | The settings of one drawing; the list of the drawings of a symbol; the icon of each tool (`tool_icon`, which both and the rail of tools use). |
| `chart::export_ui`, `chart::raster` | The export panel; the picture of a chart (`raster`: the same commands as the screen, painted by `tiny-skia` with Inter rasterized by `ab_glyph`, encoded as PNG). |
| `chart::{events, lines, palette, corner, path_pieces}` | `ChartEvent` (what a chart tells the layout) and the other event types, public; order, position and alert lines (`ChartLine`); the chart colors and their gpui conversions; the corner buttons; cutting a line down to what a GPU path can hold. |
| `multichart` (`multichart.rs`) | `MultiChart`: the charts of a layout (`Slot`s), the active one, the links, the rail, the usage limits (`view` draws it). `SymbolRef`, `MultiChartEvent`. What a chart asks of the order ticket is a `chart::events::TicketRequest`. |
| `multichart::{charts, layout, links, split}` | Adding charts and giving them symbol, timeframe, hours and data; choosing the layout and linking; the rules of the links (plain data, tested without a window); the tree of splits whose lines the user drags. |
| `multichart::{drawing_ui, drawing_cmds, favorites, clipboard, events, icon}` | The rail of tools, the style bar over a selected drawing and the pill of favorites; what the tools do for the active chart; copy and paste of indicators; pictures of the charts; the small drawing of a layout (`icon::layout_icon` is public). |

## Invariants and traps

- **Charts do not know about each other.** A chart reports what the user did as a `ChartEvent`;
  `MultiChart` applies the links (`multichart::links::route`). A chart that is made to follow another
  never reports it, which keeps linked charts free of loops.
- **A frame is plain commands.** `wyck_chart::build` makes the `Cmd` list; `chart::paint` (screen) and
  `chart::raster` (PNG) execute it. A new thing to draw belongs in `wyck-chart`, not in `paint`.
- **Live data goes through the hub.** Charts tell a `LiveHub` (in `wyck-app`) what they want and it
  keeps one subscription per symbol and period.
- **An answer can arrive after its chart is gone.** What a background task hands back goes through
  `wyck_app::lost::lost`, which logs an answer that has no owner any more; an answer carries an
  `epoch` or `serial` so that a stale one is recognised and dropped (`Chart::epoch`, `flow_epoch`,
  the serial of a script run).
- **Scripts run off the interface thread** (`custom_runs`); the last output stays on screen until the
  next run gives something newer.
- **A GPU path holds at most 65 535 vertices**: dashed and long lines go through `path_pieces`.
- **No `[]` on a slice or a vector**: the workspace denies it for data that can be short. Use `get`,
  a slice pattern or an iterator. The one exception is `MultiChart::active_chart`, whose reason is
  written on it (a layout always holds a chart and `active` stays inside it).
- **Positions on a chart are drawings**: a position drawing (`Tool::LongPosition`, `ShortPosition`)
  starts with three points (entry, stop, target); `resolved_position` fills the stop and target from
  the ATR and the risk/reward, and `position_plan` hands the result to the order ticket.

## Global state

None of its own. The saved look of the charts, the drawings and the shortcuts live in `wyck-app`;
the colors and sizes in `wyck-ui-kit`. One `AtomicBool` in `chart::paint` makes the log say once that
the path builder refused a shape.

## Tests

36 unit tests next to the code, and one ignored test that writes a chart preview for visual review
(`chart::raster`). They cover the pure parts: the link rules, the tree of splits, the cutting of
lines, the PNG composition, the settings grouping. The code that needs a window is not unit tested;
check it by running the app. Run: `cargo test -p wyck-ui-chart`. The guards that read the sources
of the screens (`no_inline_colors`, `keymap_guard`) live in `wyck`.

## Where to add X

- A chart type: its scene in `wyck-chart`; here its panel (a `*_ui.rs` file, wired in
  `chart_settings_ui`), its icon in `kinds::kind_icon` and its place in `kinds::KIND_SECTIONS`.
- A drawing tool: in `wyck-drawings`; here its icon in `tool_icon`, its hint in
  `multichart::drawing_ui::tool_hint`, and its fields in `chart::drawing_props`.
- A link between charts: the rule in `multichart::links`, the stored flag in `wyck-app`'s `LinksPref`,
  the row in the layout menu of `wyck-ui-shell`.
- Something a chart tells the group: a `ChartEvent` variant in `chart::events` and its arm in
  `multichart::events::on_chart_event`.
- A key: its action in `chart.rs` (module `action`) and its binding in the keymap of `wyck-ui-shell`.

## Do not

- Do not reach the network, the files or the system from here: go through `wyck-app` (`cargo xtask
  deps` forbids `reqwest`, `keyring`, `tokio` and the rest).
- Do not name `gpui-kit` or spell a color or a size: use `wyck_ui_kit`. The chart canvas has its own
  colors (`chart::palette`), the panels around it do not.
- Do not re-export a type of another crate from here: the user imports it from its owner.
- Do not widen a visibility "just in case": `pub` is for what `wyck-ui-trading`, `wyck-ui-editor`
  and `wyck-ui-shell` call, so that `dead_code` can see what nobody uses.
