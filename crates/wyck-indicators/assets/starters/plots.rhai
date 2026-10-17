// PLOTS, LEVELS AND FILLS
//
// Everything that can be drawn in the pane of an indicator, on a z-score (how many standard
// deviations the close is from its average). Change an option, save, and look.

indicator(#{
    name: "Plots and levels",
    short: "PLOT",
    overlay: false,
    format: "plain",
    decimals: 2,
    range: [-4, 4],              // a pane with a fixed scale: the lines can leave it, the scale stays
    category: "Examples",
    description: "Lines, histogram, dots, levels, a band and fills in a pane.",
});

let length = input_int("length", 30, #{ label: "Length", min: 5, max: 300 });
let smooth = input_int("smooth", 9, #{ label: "Signal length", min: 1, max: 100 });

// max(series, number) avoids dividing by zero on a flat market.
let z = (close - sma(close, length)) / max(stdev(close, length), 0.000001);
let signal = ema(z, smooth);
let gap = z - signal;

// Levels: a horizontal line at a value, and one shaded band between two values.
hline(0);
hline(2);
hline(-2);
band(-1, 1);

// plot(key, series, options). The key names the line in the settings (lowercase, digits, _).
// Options: title, color (a name or #rrggbb), width, style, dash, offset, up.

// A line is the default style; "dashed" and "dotted" are the other dashes.
plot("z", z, #{ title: "Z-score", color: "#42a5f5", width: 2 });
plot("signal", signal, #{ title: "Signal", color: "orange", width: 1.5, dash: "dashed" });

// A histogram. `up` is a condition for every bar: where it holds the column takes the "up" color,
// elsewhere the "down" color.
plot("gap", gap, #{
    title: "Gap",
    style: "histogram",
    color: "#78909c",
    up: gap > gap[1],
});

// Dots, only where the close is far from its average: na (no value) draws nothing.
plot("extreme", iff(abs(z) > 2.0, z, na), #{
    title: "Extreme",
    style: "dots",
    color: "red",
    width: 3,
});

// A copy of the line moved by some bars, and a number, which is a flat line.
plot("later", z, #{ title: "Z shifted", color: "gray", dash: "dotted", width: 1, offset: 5 });
plot("limit", 2.5, #{ title: "Limit", color: "gray", dash: "dotted", width: 1 });

// Shade the space between two plots drawn before. `color` is where the first is over the second,
// `down_color` where it is under; alpha goes from 0.02 (faint) to 1.
fill("z", "signal", #{ color: "#42a5f5", down_color: "#ef5350", alpha: 0.15 });
