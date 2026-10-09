# Indicators

Built-in indicators and script indicators share one model: `domain::indicators`.

## Inputs

An indicator describes its inputs with `InputSpec` (key, label, kind, default, bounds, step, and
optionally a `group` heading, a `tooltip` sentence and a text default). Numbers, toggles, choices
and colors are stored as numbers in `StudyConfig::inputs`. Inputs that are text are stored in
`StudyConfig::texts`.

| Kind | Stored as | Settings control |
|---|---|---|
| `Int`, `Float` | number | number field |
| `Source` | position in `SOURCES` (Open, High, Low, Close, HL2, HLC3, OHLC4, HLCC4) | segmented control |
| `Choice` | position in the list | segmented control |
| `Toggle` | 1 or 0 | switch |
| `Color` | `0xRRGGBB` | color swatch |
| `Symbol` | text; empty means the chart's symbol | text field |
| `Timeframe` | text such as `15m`; empty means the chart's own | quick picks and a text field |
| `Session` | text such as `0930-1600` | text field |
| `Text` | text, up to 200 characters | text field |

Old saves keep working: new kinds only add fields with defaults, and the new source goes last.

## Script API

Scripts declare inputs with `input_int`, `input_float`, `input_bool`, `input_source`,
`input_choice`, `input_color` and the text ones `input_symbol`, `input_timeframe`,
`input_session`, `input_text`. Every input accepts the options `label`, `section`, `group`,
`tooltip` and `inline`. A text input gives the script its text; a timeframe or session whose
saved text is not valid falls back to the default. `docs.rs` in `domain/indicators/custom/` is
the reference shown in the editor, and a test keeps it in step with the engine.

## Higher timeframes

`higher::higher` groups the chart's bars into the bars of a higher timeframe and keeps, for each
chart bar, the grouped bar that holds it and that bar as it was at that moment. Nothing is
fetched, so it reaches back as far as the chart. The built-in "Moving average on a timeframe"
uses it:

- With "Wait for the bar to close" on (the default), a chart bar shows the value of the last
  closed higher bar, so a finished chart never shows what was not yet known.
- With it off, a chart bar shows the average with the higher bar as it was forming.

## What is not done

- A symbol input is stored and given to scripts as text, but no indicator reads the prices of a
  second symbol yet. That needs the chart to load a second series (symbol lookup by name and
  the load functions of `app::market_data`) and pass it to the indicator next to `StudyInput`.
- A script cannot ask for a higher timeframe's prices yet; the built-in does it with `higher`.
- Plot styles beyond line, histogram and dots, per timeframe visibility of an indicator, and
  alerts on script indicators are not done.
