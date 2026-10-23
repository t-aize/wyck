# wyck-ui

Every screen of the desktop app, one top-level module each, in this order: `kit` (the widgets and the
look), `chart` (the price chart and the multichart), `trading` (the order ticket, the account panel,
the lines of the account), `editor` (the script editor) and `shell` (the window: sign-in, dashboard,
settings, shortcuts). **A module names the ones before it in that list, never after**;
`cargo xtask deps` checks it (`MODULES` in `xtask/src/sources.rs`). The domain and the account are
not here: they are `wyck-chart`, `wyck-drawings`, `wyck-indicators`, `wyck-trading` and `wyck-app`.

Depends on: `wyck-app`, `wyck-chart`, `wyck-clock`, `wyck-core`, `wyck-drawings`, `wyck-indicators`,
`wyck-market`, `wyck-trading`, `gpui`, `gpui-kit` (only the `kit` module names it), `ab_glyph` and
`tiny-skia` (only `chart::chart::raster` and `paint`), `anyhow` (only `editor`, whose completion
provider returns an `anyhow::Result`), `lsp-types`, `rust-embed`, `unicode-segmentation`, `chrono`,
`serde`, `serde_json`, `thiserror`, `tracing`; `toml` in tests.
Used by: `wyck`, which starts the app with `kit::application`, `kit::init`, `kit::Root`,
`editor::init`, `shell::app_view::AppView`, `shell::keymap::apply` and the start-up toasts of
`shell::settings_hub`.

## Module map

All modules are `pub` because `wyck` and the sibling modules name them; an item a sibling does not
call is `pub(crate)` or private. Files live in `src/<module>/`, the module root is `mod.rs`.

### `kit`: widgets and look

| Module | What it holds |
|---|---|
| `theme` | The colors in force (`fg()`, `bg()`, `accent()`, `chart_*()`...), derived from one palette, `wyck_core::appearance::Colors`. `put_in_force` applies a look chosen by the user (palette, font, scale, animations); `apply` points the component library at it and repaints the windows. |
| `tokens` | The sizes every screen shares, by topic: `text`, `height`, `space`, `bar`, `field`, `swatch`, `measure`, `menu`. They follow the text scale of `wyck_core`. |
| `assets` | `Assets`, the `AssetSource` of the app (Lucide icons from gpui-kit, `marks/...`, the logo), `FONT`, `LOGO_PATH`, `has_mark`. Files are in `assets/kit/`. |
| `button`, `controls`, `field`, `number`, `input`, `icon`, `ids` | Buttons, pickers (segmented choice, switch, swatch, chip), labelled inputs and steppers, text and number fields at the control height, icons. |
| `form`, `layout`, `confirm` | The frame of a settings panel and of a dialog, groups of labelled rows, page blocks (cards, tiles, banners, badges), the "are you sure" dialog with its tone. |
| `modal`, `menu` (with `menu::parts`), `toast` | The one modal window; menus on a right click or under a button; corner notices. |
| `color_picker`, `font_picker`, `text_input` | The color panel; the list of installed fonts; the single-line input with its IME handling (copied from gpui's own example). |
| `application`, `init`, `Root` (the module root) | The entry points of the component library, re-exported so that the program starts the app without naming `gpui-kit`. |
| `anim`, `focus`, `hints`, `window_bar`, `prelude` | Motion helpers; keyboard access for hand-drawn controls (`Keyboard::keyboard`); the keys of the actions for tooltips; the fallback window bar; the gpui-kit traits a screen may use. |

### `chart`: the chart and the multichart

Two public modules, `chart` (`chart::chart`) and `multichart`; every file below them is private except
the few the other modules call. What a frame looks like is decided by `wyck-chart` (plain drawing
commands); this module turns them into pixels, handles the pointer and the keys, and builds the
panels around the canvas.

| Module | What it holds |
|---|---|
| `chart::chart` (`chart.rs`, `api.rs`) | The `Chart` entity: its state, what it shows and the calls the rest of the app makes on it (`api`). The key actions (`ChartPanBack`, `ToolTrendLine`...) are declared here, in the module `action`. |
| `chart::chart::{history, input, follow, glue, paint, overlay, kinds}` | Loading the history and live prices; the pointer and the keys; following another chart; the drawings (presses and moves go to the drawing book of `wyck-app`); the canvas that executes a frame; what sits over it (legend, toolbar, menus, status). |
| `{flow_sync, custom_runs}` | The order flow of a footprint chart (loaded around the bars on screen); the indicators written as scripts, run off the interface thread. |
| `{chart_settings_ui, footprint_ui, tpo_ui, volume_ui, construction_ui, settings_rows}` | The settings panel of the chart and the part of each chart type. `chart_settings_ui::open` is public. |
| `{study_settings, indicator_picker, header_menus}` | The settings of one indicator, the list to add one, the menus of the header. |
| `{drawing_props, object_tree, tool_icon}` | The settings of one drawing; the list of the drawings of a symbol; the icon of each tool (`tool_icon`, which the rail of tools uses too). |
| `{export_ui, raster}` | The export panel; the picture of a chart (`raster`: the same commands as the screen, painted by `tiny-skia` with Inter rasterized by `ab_glyph`, encoded as PNG). |
| `{events, lines, palette, corner, path_pieces}` | `ChartEvent` (what a chart tells the layout) and the other event types, public; order, position and alert lines (`ChartLine`); the chart colors and their gpui conversions; the corner buttons; cutting a line down to what a GPU path can hold. |
| `multichart` | `MultiChart`: the charts of a layout (`Slot`s), the active one, the links, the rail, the usage limits (`view` draws it). `SymbolRef`, `MultiChartEvent`. What a chart asks of the order ticket is a `chart::chart::events::TicketRequest`. |
| `multichart::{charts, layout, links, split}` | Adding charts and giving them symbol, timeframe, hours and data; choosing the layout and linking; the rules of the links (plain data, tested without a window); the tree of splits whose lines the user drags. |
| `chart::chart::replay`, `multichart::replay` | A chart in a replay (tick charts and seconds charts too, from the ticks the replay holds): its series is the part of the history the replay cursor has reached (`ReplayFeed` writes it from the stores of `wyck_app::replay::Replay`), it takes nothing from the live prices and loads no history, and `clock_ms` is its "now". The layout enters every chart in the replay, tells the replay what each needs (`Demand`), shows the cursor, hides the drawings that start after it and handles the choice of the start bar. |
| `multichart::{drawing_ui, drawing_cmds, favorites, clipboard, events, icon}` | The rail of tools, the style bar over a selected drawing and the pill of favorites; what the tools do for the active chart; copy and paste of indicators; pictures of the charts; the small drawing of a layout (`icon::layout_icon` is public). |

### `trading`: order ticket, account panel, lines

The account, the safety limits and the arithmetic of lots, pips and risk are not here: they are
`wyck-app` (`account`, `alerts`) and `wyck-trading` (`ticket`, `risk`, `plan`, `book`).

| Module | What it holds |
|---|---|
| `trading` (the module root) | `lines(book, alerts, profit, currency)`: the `ChartLine`s of every symbol (positions with their profit, protections, working orders, active alerts). The shell hands them to the multichart. |
| `ticket` (`ticket.rs`) | `OrderTicket`, `TicketEvent`; the fields, the plan they add up to (`Plan`), `prefill` (fills it from a `TicketRequest` of the chart) and `same_shape`. |
| `ticket::{build, edit, planning, helpers}` | Building the fields and the subscriptions; what a click changes; the plan, the lines the ticket puts on the chart and the margin asked; the small arithmetic and formats. |
| `ticket::{send, problem}` | Turning the plan into orders, the safety check and the confirmation; the sentence for each `TicketProblem`. |
| `ticket::{link, templates}` | Following a position drawing of the chart (`PositionLink`, `PositionPlan`); the setups saved under a name and the time stop. |
| `ticket::{customize, view}` | The panel that customizes the ticket; how the ticket looks (one file per block under `view/`). |
| `panel` (`panel.rs`) | `AccountPanel`, `PanelEvent`; `open_alert` is public. |
| `panel::{data, view, stats, dialogs}` | Every row of every tab worked out from the account (`data`, one file per tab); how the panel looks; small computations; the dialogs that modify a position or an order and the one that edits an alert. |

### `editor`: the indicator script editor

A code editor (colors, line numbers, folding, search, completion, help under the pointer, the mistakes
underlined as they are made) with the list of the scripts, tabs, a console and a reference of every
function. The language, its checker and its help are `wyck-indicators`; the folder of scripts is
`wyck-app::scripts`. Everything is private except `IndicatorEditor`, `EditorEvent`, `init`,
`CONTEXT` and the eight key actions (`SaveScript`, `AddToChart`...).

| Module | What it holds |
|---|---|
| `editor` (the module root) | `IndicatorEditor`: the open documents, the active tab, the console, the sizes; the drafts of unsaved text; `init` (the language config, once at start). |
| `tabs`, `files`, `prompts`, `actions` | Opening and closing tabs and what the check says about the text; saving, adding the script to the active chart and following the folder on disk (`library_changed`); asking for a name and creating, duplicating, deleting and renaming scripts; what the toolbar and menus do. |
| `providers` | Hands the words of a script to the gpui editor: colors, folds, completion, hover. The thinking is in `wyck_indicators::custom::{lexer, assist}`. |
| `parts` (`toolbar`, `explorer`, `console`, `reference`) | What the editor looks like. |

### `shell`: the window

The root view that shows the sign-in gate until there is a session and the dashboard after it. It is
the only module that knows every other screen module; they do not know it. Three modules are public,
the ones `wyck` calls.

| Module | What it holds |
|---|---|
| `app_view` | `AppView`: owns the sign-in and the open dashboard; `locked_frame` draws a soft copy of the dashboard behind the sign-in modal. |
| `sign_in_gate` | The sign-in modal, one card per `Phase` of `wyck_app::sign_in`. |
| `actions` | The actions of the dashboard (`OpenPicker`, `ToggleTicket`...), declared apart so that `keymap`, which binds keys to them, does not depend on the screen that handles them. |
| `dashboard` | `Dashboard`: the screen the app lands on and what it draws (`view`). |
| `dashboard::{session, catalog, class, picker, details, lists}` | Following the broker session and the active symbol; the list of symbols an account can trade (no UI) and the `Class` it files them under; the symbol picker and its details sheet; favorites and watchlists. |
| `dashboard::marks` | The picture of a symbol (`icon_for`): one module per asset class says which logo, flag or glyph, and `paint` draws it. |
| `dashboard::{header, chart_buttons, layout_menu, editor_dock, notice}` | The bar across the top and the account menu; the chart type and indicator buttons; the layout picker; the drawer of the script editor; the toast for what the account has to say. |
| `dashboard::{replay, replay_bar}` | The replay as the dashboard shows it: the header button and its menu, the bar over the charts (play, step, speed, step size and a custom step typed as `7 min` or `50 ticks`, real ticks on or off, go to a date, back to live), the keys, and what a replay forbids. It takes the lock of the account when a replay starts and gives it back when it ends. |
| `dashboard::trade` | The trading side: account, order ticket, account panel, price alerts and the lines they put on the charts. |
| `keymap` | `DEFS`, the one table of every binding (custom, fixed), `HANDLED` (keys a screen reads by hand), and `apply`, which binds the result in gpui and tells the kit the keys for its tooltips. |
| `settings_hub` | The settings window: one file per page (`look`, `charts`, `behaviour`, `alerts`, `safety`, `scripts`, `shortcuts`, `data`, `folders`, `about`). |

## Invariants and traps

### Everywhere

- **The module order is the layering.** `kit` < `chart` < `trading`, `editor` < `shell`: write
  `crate::kit::...` between modules, never `super::super::` (the check reads `crate::` paths).
- **Guards read the sources** (`tests/guards/`): `keymap.rs` and `colors.rs`, see "Tests".
- **No `[]` on a slice or a vector**: the workspace denies it. Use `get`, a slice pattern or an
  iterator; a control that picks by position uses `Kind::ALL.get(index)` and does nothing out of
  range. The one exception is `MultiChart::active_chart`, whose reason is written on it.

### `kit`

- **No color literal outside `theme`, `color_picker` and the brand colors of the symbol marks**
  (`shell/dashboard/marks*`). The guard `tests/guards/colors.rs` fails on a `0xRRGGBB` anywhere else
  in the screens. A screen reads `theme::` values.
- **A screen never configures a gpui-kit component itself.** Sizes and variants (`Sizable`,
  `ButtonVariants`) stay out of `prelude` on purpose: the kit picks them.
- **An animation plays once per element id.** To replay one, change the id (the `epoch` values the
  screens thread through). With animations off, the helpers still run their animation, but in 1 ms.
- **`text_input` is gpui's example, trimmed.** The grapheme boundaries and the UTF-16 conversions are
  what correct editing takes; do not "simplify" them.
- **A control drawn from a `div` is invisible to the keyboard** until `.keyboard()` makes it a Tab
  stop with a visible focus ring.
- **Marks and fonts keep their licenses**: `assets/kit/fonts/LICENSE.txt`,
  `assets/kit/marks/*/LICENSE.txt`. Marks are trademarks of their owners, shown only to identify an
  instrument.

### `chart`

- **Charts do not know about each other.** A chart reports what the user did as a `ChartEvent`;
  `MultiChart` applies the links (`multichart::links::route`). A chart that is made to follow another
  never reports it, which keeps linked charts free of loops.
- **A frame is plain commands.** `wyck_chart::build` makes the `Cmd` list; `paint` (screen) and
  `raster` (PNG) execute it. A new thing to draw belongs in `wyck-chart`, not in `paint`.
- **A chart in a replay never touches live data**: `Chart::replay` is set, `on_live`, `refill`,
  `on_ready`, older pages and the flow of a footprint return at once, `reload` re-seeds from the
  replay, and every "now" the chart draws or asks for goes through `clock_ms`. A new place that reads
  the clock or loads history must do the same. Orders, alerts and the lines of the account are hidden
  or refused while it runs.
- **Live data goes through the hub.** Charts tell a `LiveHub` (in `wyck-app`) what they want and it
  keeps one subscription per symbol and period.
- **An answer can arrive after its chart is gone.** What a background task hands back goes through
  `wyck_app::lost::lost`; an answer carries an `epoch` or `serial` so that a stale one is dropped.
- **Scripts run off the interface thread** (`custom_runs`); the last output stays on screen until the
  next run gives something newer.
- **A GPU path holds at most 65 535 vertices**: dashed and long lines go through `path_pieces`.
- **One book, several charts.** Every chart draws into the same drawing book, and two charts of one
  symbol both see the pointer. The chart that takes a press calls `Book::hold` (and `release_hold` on
  the release); the others ignore the moves meanwhile (`glue::drawing_moved`). A drag past the edge of
  the plot keeps the point on the edge.
- **Positions on a chart are drawings**: a position drawing (`Tool::LongPosition`, `ShortPosition`)
  starts with three points (entry, stop, target); `resolved_position` fills the stop and target from
  the ATR and the risk/reward, and `position_plan` hands the result to the order ticket.

### `trading`

- **The ticket decides nothing about risk.** The plan comes from `wyck_trading::ticket` and
  `wyck_trading::risk` and from the fields; what stops an order is a `TicketProblem` that
  `problem::text` words.
- **The safety limits come before the confirmation.** `send` asks the account to `assess_batch` the
  orders: a hard limit (`Verdict::Block`) refuses with a toast before anything opens; a warning, or a
  risk above the share the user set (`layout.high_risk`), goes into the confirmation. One-click
  trading sends at once only when there is nothing to warn about. The limits are in `wyck-app`.
- **A new order leaves with `place_batch` on the account, nowhere else.** Closing, reversing,
  trailing and protecting a position also go through methods of the account. The only direct use of
  the broker session is the read of margins in `ticket::planning`.
- **A ticket linked to a position drawing follows it** until the order is sent or the user unlinks:
  the stop of an ATR stop and the take profit of a multiple of the risk are worked out by the ticket
  and are read-only (`same_shape` ignores them when it asks whether the drawing changed).
- **Money comparisons are exact only where numbers are copied**, never computed (`same_shape`, the
  one `#[expect(float_cmp)]`). Sizes and prices are converted with `wyck_core::math`, not `as`.

### `editor`

- **Saving writes the file and nothing else.** The app notices the change the way it does for a file
  edited in another program, so every chart that holds the script redraws. `files::library_changed`
  makes a tab follow the file when the tab has nothing of its own; a tab with unsaved text is marked
  `conflict` instead of being overwritten.
- **Unsaved text survives a restart.** The drafts (`indicator_editor_drafts`, saved through a
  `Saver`) are restored only when the scripts folder is the one they were written for.
- **The check runs `CHECK_DELAY` after the last key**, not on every key.
- **Positions are the protocol's.** A line or a column handed to the editor goes through `lsp_index`
  (a `u32`); a column found in a string is a byte index, so slice with `get`, never with `[..]`.
- **An answer can arrive after its tab is gone**: results of the library and of file dialogs go
  through `wyck_app::lost::lost`.

### `shell`

- **Every binding is declared in `keymap::DEFS`.** `KeyBinding::new` anywhere else, or a file that
  reads keys by hand without being listed in `keymap::HANDLED`, fails `tests/guards/keymap.rs`. The
  settings page shows the table, so a binding outside it is invisible to the user and to the clash
  tests.
- **Preferences are changed through the workspace** (`edit_preferences`, `appearance::update`,
  `indicators::update_prefs`), never by writing a file; saving is `wyck-app`'s business.
- **A backup import or a folder move is staged, not done.** The page records what waits
  (`PendingSummary`, a gpui global) and `wyck` applies it at the next start, before anything reads
  the documents it replaces.
- **Sending and closing orders is `trading`'s path**: the dashboard only routes `TicketEvent` and
  `PanelEvent` and puts the lines on the charts.

## Global state

Each is here because the helpers have no window or context at hand when they are called.

- `kit::theme`: the palette (`RwLock<Colors>`) and the font (`RwLock<Option<String>>`), written by
  `put_in_force`, read on every paint. `kit::anim`: whether things move (`AtomicBool`). The text
  scale is not here: it is `wyck_core::text_scale`, which the charts read too. `kit::hints`: the keys
  of the actions (`RwLock<HashMap>`), filled by `shell::keymap::apply`.
- `kit::color_picker`: the saved colors, the function that persists them and the open panels (gpui
  globals); `kit::modal`: the handle of the host that shows the modal (gpui global).
- `chart`: one `AtomicBool` in `chart::chart::paint` makes the log say once that the path builder
  refused a shape.
- `shell`: `keymap::DEFS` and `HANDLED` are constant tables; `settings_hub` sets one gpui global,
  `PendingSummary`.

## Tests

117 tests. Unit tests sit next to the code: the color maths, the palette, the menu and number fields
(`kit`), the link rules, the tree of splits, the cutting of lines and the PNG composition (`chart`,
plus one ignored test that writes a chart preview for visual review), the arithmetic and saved
setups of the ticket and the rows of the tables (`trading`), the drafts and the styles (`editor`),
the symbol catalog, the marks and the keymap (`shell`). The code that needs a window is not unit
tested; check it by running the app.

Integration tests in `tests/`, each binary in its own process:

- `guards/`: reads the `.rs` files of this crate and of `wyck`. `keymap.rs`: every `KeyBinding::new` is
  in `shell/keymap.rs`, and a file that reads keys by hand is in `READS_KEYS` and its keys in
  `keymap::HANDLED`. `colors.rs`: no `0xRRGGBB` outside the theme, the color picker and the symbol
  marks. A folder that moves must fail them, not make them pass on nothing: `rust_files` panics on a
  folder it cannot read.
- `text_scale.rs`: the only test that sets `wyck_core::text_scale`; it has its own process because
  every other test reads it.

Run: `cargo test -p wyck-ui`.

## Where to add X

- A control: a function or a builder in the `kit` module of its family, with sizes from `tokens` and
  colors from `theme`; make it `pub` only when a screen calls it. A size: `tokens`, scaled with
  `scaled(..)`. A color the user can change: `wyck_core::appearance::Colors`, then a `theme::`
  reader. An icon: `gpui_kit::assets::IconName` through `icon::tinted`; a flag or logo:
  `assets/kit/marks/<set>/<name>.svg` with its license next to it.
- A chart type: its scene in `wyck-chart`; here its panel (a `*_ui.rs` file, wired in
  `chart_settings_ui`), its icon in `kinds::kind_icon` and its place in `kinds::KIND_SECTIONS`.
- A drawing tool: in `wyck-drawings`; here its icon in `tool_icon`, its hint in
  `multichart::drawing_ui::tool_hint`, and its fields in `drawing_props`.
- A link between charts: the rule in `multichart::links`, the stored flag in `wyck-app`'s `LinksPref`,
  the row in the layout menu of `shell`. Something a chart tells the group: a `ChartEvent` variant
  and its arm in `multichart::events::on_chart_event`.
- A block in the ticket: its view in `ticket/view/<block>.rs`, its place in the layout of
  `wyck_app::prefs::ticket::Layout`, its page in `ticket/customize`. A column or tab of the panel:
  `panel::data` and `Tab` in `wyck_app::prefs::panel`.
- A command in the editor toolbar: `parts/toolbar.rs` or `parts/explorer.rs` for the button,
  `actions.rs`, `files.rs` or `prompts.rs` for what it does. A thing the language offers: in
  `wyck-indicators`, not here.
- A shortcut: its action in the module that handles it (key actions of `chart` and `editor` are
  declared in their own `action` module), its row in `shell::keymap::DEFS` (or `HANDLED` if a screen
  reads the key itself, then the file in `READS_KEYS` of `tests/guards/keymap.rs`).
- A settings page: a file in `shell/settings_hub/`, its `Page` and its tab in `settings_hub.rs`.
- A symbol class or a logo: `shell/dashboard/catalog.rs` (the class) and the module of the class in
  `shell/dashboard/marks/` (the logo, with its license in `assets/kit/marks`).

## Do not

- Do not reach the network, the files or the system from here: go through `wyck-app` (`cargo xtask
  deps` forbids `reqwest`, `keyring`, `tokio` and the rest).
- Do not name `gpui-kit` outside `kit`, nor spell a color, a pixel size or an animation duration in
  a screen: they belong in `kit`. The chart canvas has its own colors (`chart::chart::palette`).
- Do not add a way to send an order that skips `assess_batch` and the confirmation.
- Do not work out risk, margin or lots here: `wyck_trading::ticket` and `wyck_trading::risk` do.
- Do not put a rule of the script language here (`wyck-indicators`), and do not write to the scripts
  folder yourself: go through `wyck_app::scripts`.
- Do not put the logic of a screen in `shell`: a chart, a ticket or an editor behaves the same wherever
  it is hosted, so it lives in its module and the dashboard only wires its events.
- Do not re-export a type of another crate: import it from its owner.
- Do not widen a visibility "just in case": `pub` is for what `wyck` or a later module calls, so that
  `dead_code` can see what nobody uses.
