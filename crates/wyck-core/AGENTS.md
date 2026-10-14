# wyck-core

The smallest vocabulary every other crate shares. No domain knowledge, no I/O, no UI toolkit.

Depends on: `serde` only (no other crate of the workspace). Everything above may use it.

## Module map

| Module | What it holds |
|---|---|
| `flex` | `serde` readers for integers that arrive as a number or as text (the cTrader JSON gateway sends 64 bit values either way). Every wire type uses `flex::int`, `flex::opt` or `flex::list`. |
| `format` | Number to text: `trim` (no trailing zeros), `grouped` (thousands), `signed`. |
| `math` | Arithmetic on series for indicators: moving averages, deviations, oscillators. A series in, a series of the same length out, `NaN` where a value is not defined yet. |
| `defaults` | `yes()`, the `#[serde(default = "...")]` function of a setting that is on when absent. |
| `appearance` | The look as plain data: `Colors` (the one palette), `Look`, the scale bounds `SCALE_MIN` / `SCALE_MAX`, contrast and luminance. |
| `text_scale` | How big the text drawn on a chart is, as a percent. Process wide. |
| `intern` | Names that live for the whole run (`&'static str` for the names of user indicators). Process wide. |

## Invariants and traps

- `math` functions index their inputs without checking: series passed together must have the same
  length. A caller that takes a series from outside (a script) compares the lengths first.
- A `NaN` in a series is a gap: averages restart after it. Keep outputs aligned index for index
  with the bars they were computed from.
- `math` is here, and not in `wyck-indicators`, because `wyck-trading` needs `atr` below the
  indicators. Do not add a function here that only indicators use; add it to `wyck-indicators`.
- `flex` refuses what is not a whole number (`1.5`, `"x"`, a `u64` above `i64::MAX`) instead of
  rounding it.

## Global state (on purpose)

- `text_scale`: one `AtomicU32`, set by the interface when the user changes the scale, read by the
  scene of every chart. A picture of a chart is made at the scale in force when it is taken.
  Tests must not set it: other tests read it.
- `intern`: a `Mutex` around a table of leaked strings, capped at 4 MiB. The same text is stored
  once; past the cap `name()` returns `(too many names)`.

## Tests

Unit tests sit next to the code (`#[cfg(test)]`). `math` is checked against values worked out by
hand. Run: `cargo test -p wyck-core`.

## Where to add X

- A number format: `format`. A new serde reader for the wire: `flex`.
- A new indicator formula: `wyck-indicators` unless `wyck-trading` needs it too.
- A color or an interface constant shared by the app and the design system: `appearance`.

## Do not

- Do not add a dependency on another crate of the workspace, on `gpui`, `tokio` or any I/O crate:
  `cargo xtask deps` fails.
- Do not put a thing here because two modules of one crate use it: it belongs in that crate.
