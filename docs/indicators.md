# Your own indicators

wyck runs indicators written as small scripts. A script is a text file with the extension `.rhai`
(the language is [Rhai](https://rhai.rs)). The app reads every script in a folder and adds it to
the list of indicators, next to the ones it ships.

## Where the scripts are

- The default folder is `indicators` inside the settings folder. The button with the `</>` icon in
  the header opens it (first entry of its menu, "Open the indicators folder").
- Settings (Ctrl+comma), page Indicators, lets you choose another folder. Every `.rhai` file in it, and in
  the folders inside it (three levels deep), is an indicator. A folder is the category of the
  scripts in it, unless a script says its own.
- The folder is read again every moment. A file saved by another program shows up on its own, and
  the charts that hold it draw the new version. You can turn that off in the settings.
- The first time, the folder gets a few examples in a folder named `Examples`.
- A script you delete in the editor goes to the `.trash` folder inside the indicators folder.

## The editor

Open it from the header button (menu entry "Indicator editor") or with Ctrl+Shift+E. It docks under
the charts. Drag its top edge to resize it, or use the button that makes it as tall as the window.

- **Scripts**: the folder as a tree, with a search. Right click a script for Open, Add to chart,
  Rename, Duplicate, Show in folder and Delete.
- **Tabs**: several scripts can be open. A dot on a tab means it has changes that are not saved.
- **Code**: colors, line numbers, folding, search (Ctrl+F), auto closing brackets, indentation,
  the names of the language offered while you type, and help under the pointer.
- **Problems**: mistakes are underlined as you type, with the message, and listed in the console.
  Click one to go to it.
- **Output**: what the script printed with `print(x)`, and how long it took on the active chart.
- **Reference**: every function, with its signature and an example. Click a line to write the
  example at the cursor.
- **Save** (Ctrl+S) writes the file. **Add to chart** (Ctrl+Enter or F5) saves and puts the script
  on the active chart. A chart that holds a script draws the new version as soon as it is saved.
- **Import** takes `.rhai` files or folders of them. **Export** writes one script, or all of them,
  where you choose. A script is a plain text file: share it like any file.

## Adding an indicator to a chart

The Indicators button of the bar over each chart opens the list: the indicators the app ships
and yours, by category, with a search. Star the ones you use most and they go to Favorites, and
to the quick menu next to the button. A script with a mistake is listed with its problems and
cannot be added until it works.

## The language in one page

A script is a program. It says what the indicator is, what the user can change, and what it draws.

```rhai
indicator(#{
    name: "My average",
    short: "MA",
    overlay: true,
    format: "price",
    category: "Custom",
    description: "An average of the closes.",
});

let length = input_int("length", 20, #{ min: 1, max: 500, label: "Length" });
let source = input_source("source", "close");

plot("average", sma(source, length), #{ color: "orange", width: 2 });
```

The bars are provided as **series**: one number for every bar of the chart. `open`, `high`, `low`,
`close`, `volume`, `time`, `hl2`, `hlc3`, `ohlc4` and `bar_index` are series, and `n` is how many
bars there are. Arithmetic works on whole series: `close - open`, `2 * close`, `close > open`
(1 where true, 0 where false). `close[1]` is the close of the bar before. A bar with no value (the
first bars of an average) holds `na`.

Functions work on whole series too, so a script does a handful of operations, not one for every
bar. The reference in the editor lists them all: averages (`sma`, `ema`, `wma`, `hma`, `rma`,
`vwma`), windows (`highest`, `lowest`, `sum`, `stdev`, `linreg`), oscillators (`rsi`, `macd`,
`stoch`, `cci`, `mfi`), volatility (`atr`, `bollinger`, `keltner`, `donchian`), trend
(`supertrend`, `dmi`, `psar`, `vwap`), conditions (`cross_over`, `cross_under`, `iff`) and more.
The ones that give several series give a map: `let m = macd(close, 12, 26, 9); plot("hist", m.hist);`.

### Describing the indicator

| Call | What it does |
| ---- | ------------ |
| `indicator(#{ ... })` | name, short, overlay, format, range, category, description, author, version |
| `input_int`, `input_float`, `input_bool` | a number or a switch the user can change |
| `input_source`, `input_choice`, `input_color` | a price, one of a few choices, a color |
| `plot(key, series, #{ ... })` | draws a line, a histogram or dots |
| `hline(value)`, `band(low, high)` | levels and a shaded band in the pane |
| `fill(key_a, key_b)` | shades the space between two plots |

What a script declares must not depend on the data: the app runs the script once on no bars to
learn its inputs and plots, then again on the bars of a chart.

### Loops

A loop over the bars is possible (`for i in 0..n { ... }`, with `series(n, 0.0)` and `out.set(i, x)`)
and is what the example "Recursive smoothing" shows, but it is slow next to the functions on whole
series. A script that does too many operations, or runs too long, is stopped, and the chart says so.
Settings, page Indicators, sets how much a script may do (Light, Normal, Heavy).

## What a script cannot do

It cannot read a file, use the network or the clock, or load another script. It only reads the bars
it is given and draws. A script runs off the interface thread, so a slow one never freezes a window.
Scripts still are code: read a script you did not write before you run it.

## Backup

The backup of everything (Settings, page Data and backup) includes the indicator scripts.
Restoring puts them in the default indicators folder, after copying aside the ones it replaces.
