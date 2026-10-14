# wyck-indicators

The indicators ("studies"): the built-in ones, the settings a user gives them (which is also what is
saved), the arithmetic that turns bars into lines, the catalog the menus read, and the Rhai scripts a
user writes. Pure code: no window, no file, no socket. Reading the scripts folder is done by the app,
which hands the sources to `custom::library`.

Depends on: `wyck-core`, `wyck-market`, `wyck-drawings`, `rhai`, `chrono`, `serde`, `serde_json`,
`toml`.

## Module map

| Module | What it holds |
|---|---|
| `kind`, `spec` | `StudyKind` (the indicators on offer; the names are written in saved files) and `Spec` (what one is made of: where it is drawn, its inputs, its plots). `kind/specs.rs` and `extended/specs.rs` hold the fixed spec of each built-in. |
| `config` | `StudyConfig`: one indicator on a chart as the user set it, also its saved form. Inputs and plots are named by stable keys, never by position. |
| `style` | How plots, levels and fills look (`PlotStyle`, `LevelStyle`, `FillStyle`). |
| `compute` | `compute(&StudyConfig, &StudyInput) -> StudyOutput`: bars in, lines out. `StudyInput` is the bars as columns of numbers. |
| `extended` | The built-in indicators past the first batch (private; reached through `compute`). |
| `higher` | Grouping the chart's bars into a higher timeframe and carrying a value back to the chart's bars. |
| `profile` | The volume profile of the bars on screen (not a line per bar, so not in `compute`). |
| `catalog` | `items()`: every indicator on offer, built-in and scripted, in one list. |
| `custom` | The scripts. `run` compiles, declares and computes a script under limits; `api` is every function a script can call (`api/` by topic); `series` is the value a script works on (a number per bar); `library` is the compiled `Entry` of a file and the global `registry`; `other` aligns the bars of a second symbol; `draw_api` lets a script own chart drawings (`wyck-drawings`); `docs`, `assist`, `lexer` serve the editor (reference, completion, colors); `templates` holds the starters (`assets/starters/*.rhai`, compiled in). |

## Invariants and traps

- The saved form is a promise. `StudyKind` names, input keys and plot keys are written in files and
  never change; an unknown kind loads as `StudyKind::Unknown` and is dropped, a missing input takes
  its default, an unknown one is ignored. `StudyConfig::normalized` repairs a file.
- A built-in output stays aligned with the input bars: one value per bar, `NaN` where there is no
  value yet. The arithmetic itself lives in `wyck_core::math`; this crate wires it.
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

## Global state (on purpose)

- `custom::library::registry`: an `RwLock` around the compiled scripts, which every chart looks
  indicators up in. The app publishes the whole list with `install`. Tests use the test-only
  `insert` so they do not wipe each other's scripts.
- A spec holds `'static` slices, so what a script declares (inputs, plots, choices) is leaked once per
  distinct content in tables capped at 20 000 slices each (past it an empty slice is handed out),
  and names go through `wyck_core::intern`.

## Tests

Unit tests next to the code, `src/tests.rs` for the ones that cross modules (every indicator on one
set of bars), `custom/behavior.rs` for scripts end to end (each function against its arithmetic),
and one test that compiles and computes every starter. Math is checked against worked examples:
those and the `docs` cross-check are the tests that must not be weakened. Run:
`cargo test -p wyck-indicators`. If a script test fails by timeout on a loaded machine, rerun it
alone before blaming a change.

## Where to add X

- A built-in indicator: a `StudyKind` variant (a new name), its spec in `kind/specs.rs` or
  `extended/specs.rs`, its arithmetic in `wyck_core::math` when it is a general one, its branch in
  `compute`, its entry in `catalog`, a test on the shared set of bars.
- A function a script can call: `custom/api/` (thin: it hands the numbers to `wyck_core::math`),
  its description in `custom/docs.rs`, a test in `custom/behavior.rs`.
- A starter: a `.rhai` file in `assets/starters/`, an entry in `custom/templates.rs`. It must be
  ASCII, compile, and compute.

## Do not

- Do not read the scripts folder or any file here: that is the app's job, this crate gets sources.
- Do not give a script a way out: no file, network, clock or module in the engine.
- Do not rename a `StudyKind` variant, an input key or a plot key without an alias and a migration.
- Do not use `rhai` anywhere but this crate (`cargo xtask deps` fails), nor `gpui`, `tokio` or files.
- Do not depend on `wyck-chart` or anything above: the arrow goes the other way.
