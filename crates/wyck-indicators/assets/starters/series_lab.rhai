// THE FUNCTIONS ON SERIES
//
// A lab: pick a function in the settings of the indicator and see what it does to the bars.
// Each line of the `switch` below is one function (or one operator) in use, so this file is also
// a cheat sheet. Everything works on the whole series at once, bar by bar.
//
// Rules to remember:
//  - a comparison gives a series of 1 (true) and 0 (false); a bar with no value is NaN;
//  - use & (and), | (or) and ! (not) between series, not && || which only work on single values;
//  - a whole number and a decimal are not the same for a function: write 30.0, not 30, where a
//    function asks for a decimal (clamp, nz...).

indicator(#{
    name: "Series lab",
    short: "SLAB",
    overlay: false,
    format: "plain",
    decimals: 4,
    category: "Examples",
    description: "Pick a function on series and see its result.",
});

let which = input_choice("function", "abs", [
    "abs", "sqrt", "ln", "log10", "exp", "sin", "cos", "floor", "ceil", "round", "sign",
    "min", "max", "pow", "clamp", "nz", "is_na", "shift", "change", "cum", "barssince",
    "iff", "cross_over", "cross_under", "cross", "logic", "compare", "arithmetic", "modulo",
    "power", "index", "scalars",
], #{ label: "Function" });
let length = input_int("length", 10, #{ label: "Length", min: 1, max: 200 });

let result = switch which {
    // One value in, one value out, on every bar.
    "abs" => abs(close - open),
    "sqrt" => sqrt(volume),
    "ln" => ln(close),
    "log10" => log10(close),
    "exp" => exp((close - sma(close, length)) / max(stdev(close, length), 0.000001)),
    "sin" => sin(bar_index / 10.0),
    "cos" => cos(bar_index / 10.0),
    "floor" => floor(close * 100.0) / 100.0,
    "ceil" => ceil(close * 100.0) / 100.0,
    "round" => round(close * 100.0) / 100.0,
    "sign" => sign(close - open),

    // Two series (or a series and a number) in, one out.
    "min" => min(close, open),
    "max" => max(close, sma(close, length)),
    "pow" => pow(close - open, 2),
    "clamp" => clamp(rsi(close, length), 30.0, 70.0),

    // Bars with no value (the first bars of an average, say).
    "nz" => nz(close[length], 0.0),
    "is_na" => is_na(close[length]),

    // Going back in time.
    "shift" => shift(close, length),
    "change" => change(close, length),
    "cum" => cum(close - open),
    "barssince" => barssince(close > open),

    // Conditions.
    "iff" => iff(close > open, 1, -1),
    "cross_over" => cross_over(close, sma(close, length)),
    "cross_under" => cross_under(close, sma(close, length)),
    "cross" => cross(close, sma(close, length)),
    "logic" => ((close > open) & (volume > sma(volume, length)))
        - (!(close > open) & (volume > sma(volume, length))),
    "compare" => (close > open) - (close < open),

    // Operators.
    "arithmetic" => (close - open) / (high - low) * 100.0,
    "modulo" => floor(time / 60000.0) % 60.0,
    "power" => (close - open) ** 2,

    // close[3] is the close of 3 bars before, the same as shift(close, 3).
    "index" => close - close[length],

    // at(i), last() and len() read one number out of a series; series(n, x) makes a flat series.
    "scalars" => series(n, close.last() - close.at(0)),
    _ => close,
};

hline(0);
plot("result", result, #{ title: "Result", color: "#42a5f5", width: 1.5 });

// print and debug write to the console of the editor. to_string and to_debug describe a series.
print(`${which}: ${result.to_string()}`);
debug(result.to_debug());
