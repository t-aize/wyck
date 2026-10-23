// THE SETTINGS OF AN INDICATOR
//
// Every call that describes an indicator and the settings a user can change, with every option.
// Read it from the top, change a line, save (Ctrl+S) and open the settings of the indicator on a
// chart. Every function is described in the reference on the right of the editor.
//
// A script runs twice: first on no bars at all, to learn what it is (indicator, input_*, plot),
// then on the bars of the chart. So those calls always run, whatever the settings are.
// How a line looks (color, width, dash) is fixed by the script; the user changes it in the Style
// tab of the settings. Fills and drawings, on the other hand, can follow a setting.

// 1. What the indicator is. Every option can be left out.
indicator(#{
    name: "Settings",            // the name in the list of indicators
    short: "SET",                // the short name in the legend
    overlay: true,               // true: on the prices. false: in a pane of its own
    format: "price",             // how values are written: "price", "plain" or "count"
    decimals: 2,                 // the decimals of "plain" (0 to 8)
    // range: [0, 100],          // a pane with a fixed scale (with overlay: false)
    category: "Examples",        // the heading it is listed under
    description: "Every kind of setting an indicator can have.",
    author: "Wyck",
    version: "1.0",
});

// 2. Numbers. Options: label, min, max, step, group (a heading), tooltip, section.
// The value is always kept between min and max.
let length = input_int("length", 20, #{
    label: "Length",
    min: 1,
    max: 500,
    step: 1,
    group: "Average",
    tooltip: "How many bars the average looks at.",
});
let envelope = input_float("envelope", 1.0, #{
    label: "Envelope (percent)",
    min: 0.1,
    max: 20.0,
    step: 0.1,
    group: "Envelope",
});

// 3. A switch.
let show_envelope = input_bool("show_envelope", true, #{
    label: "Show the envelope",
    group: "Envelope",
});

// 4. A price: Open, High, Low, Close, HL2, HLC3, OHLC4 or HLCC4. It gives a series.
let source = input_source("source", "close", #{ label: "Price", group: "Average" });

// 5. One choice among a few. It gives the text of the choice.
let kind = input_choice("kind", "EMA", ["SMA", "EMA", "WMA", "HMA", "RMA", "VWMA"], #{
    label: "Type of average",
    group: "Average",
});

// 6. A color, as text (#rrggbb), and a width. `section` puts a setting in the "inputs" tab or in
// the "style" tab; without it a color goes to Style, and so does a name with "width" in it.
let tint = input_color("tint", "#42a5f5", #{
    label: "Envelope color",
    section: "style",
    group: "Envelope",
});
let line_width = input_float("line_width", 1.0, #{
    label: "Line width",
    min: 0.5,
    max: 6.0,
    step: 0.5,
    section: "style",
});

// 7. Text. Each gives the text the user typed (empty if nothing).
// A timeframe is written 15m, 4h, 1D...; a session is written HHMM-HHMM. Wyck checks them, and
// what to do with them is up to the script: here they only end up in a label.
// (A symbol is the other text setting: see the example Two symbols.)
let note = input_text("note", "Last price", #{
    label: "Note",
    tooltip: "Free text, up to 200 characters.",
    group: "Text",
});
let timeframe = input_timeframe("timeframe", "", #{ label: "Timeframe", group: "Text" });
let hours = input_session("hours", "", #{ label: "Session", group: "Text" });

// The values. The whole series is computed at once: there is no loop over the bars.
let average = switch kind {
    "SMA" => sma(source, length),
    "WMA" => wma(source, length),
    "HMA" => hma(source, length),
    "RMA" => rma(source, length),
    "VWMA" => vwma(source, length),
    _ => ema(source, length),
};

// A series with no value: nothing is drawn where there is none. A plot must always be there, so
// a hidden line is a plot of nothing, never a plot that is skipped.
let none = series(n, na);
let upper = if show_envelope { average * (1.0 + envelope / 100.0) } else { none };
let lower = if show_envelope { average * (1.0 - envelope / 100.0) } else { none };

plot("average", average, #{ title: "Average", color: "orange", width: 2 });
plot("upper", upper, #{ title: "Upper", color: "#42a5f5", width: 1, dash: "dashed" });
plot("lower", lower, #{ title: "Lower", color: "#42a5f5", width: 1, dash: "dashed" });
fill("upper", "lower", #{ color: tint, alpha: 0.08 });

// 8. A setting can drive a drawing. Never draw when n is 0: that run has no bars.
if n > 0 && !close.last().is_nan() {
    let label = note;
    if timeframe != "" { label += ` ${timeframe}`; }
    if hours != "" { label += ` ${hours}`; }
    draw("last", "horizontal_line", [bar_point(n - 1, close.last())], #{
        color: tint,
        width: line_width,
        dash: "dotted",
        text: label,
    });
}
