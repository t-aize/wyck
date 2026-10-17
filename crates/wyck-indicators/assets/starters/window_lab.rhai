// AVERAGES AND WINDOWS
//
// A lab for the functions that look at the last `length` bars: pick one in the settings and see
// it. The last few choices are built from the others, to show how the pieces combine.
// Note: a function that takes a series ends on a number of bars; the first bars have no value.

indicator(#{
    name: "Window lab",
    short: "WLAB",
    overlay: false,
    format: "plain",
    decimals: 4,
    category: "Examples",
    description: "Pick an average or a window function and see its result.",
});

let which = input_choice("function", "sma", [
    "sma", "ema", "wma", "hma", "rma", "vwma", "stdev", "highest", "lowest", "sum",
    "linreg", "slope", "roc", "momentum", "rsi",
    "range position", "bandwidth", "z-score", "slope per atr",
], #{ label: "Function" });
let length = input_int("length", 20, #{ label: "Length", min: 2, max: 300 });
let source = input_source("source", "close", #{ label: "Price" });

let result = switch which {
    "sma" => sma(source, length),            // simple average
    "ema" => ema(source, length),            // exponential: recent bars weigh more
    "wma" => wma(source, length),            // linear weights: the newest bar weighs most
    "hma" => hma(source, length),            // Hull: fast, with little lag
    "rma" => rma(source, length),            // Wilder's, the one RSI and ATR use
    "vwma" => vwma(source, length),          // weighted by volume
    "stdev" => stdev(source, length),        // standard deviation
    "highest" => highest(high, length),
    "lowest" => lowest(low, length),
    "sum" => sum(volume, length),
    "linreg" => linreg(source, length),      // the value of the least squares line, now
    "slope" => slope(source, length),        // and its slope per bar
    "roc" => roc(source, length),            // rate of change, in percent
    "momentum" => momentum(source, length),  // difference from `length` bars ago
    "rsi" => rsi(source, length),            // 0 to 100

    // Built from the pieces above.
    "range position" => (close - lowest(low, length)) / max(highest(high, length) - lowest(low, length), 0.000001) * 100.0,
    "bandwidth" => stdev(source, length) / sma(source, length) * 100.0,
    "z-score" => (source - sma(source, length)) / max(stdev(source, length), 0.000001),
    "slope per atr" => slope(source, length) / max(atr(length), 0.000001),
    _ => sma(source, length),
};

plot("result", result, #{ title: "Result", color: "#7e57c2", width: 1.5 });
