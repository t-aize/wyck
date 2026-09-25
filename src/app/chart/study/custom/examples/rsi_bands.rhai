// The relative strength index with its overbought and oversold levels and a shaded middle band.

indicator(#{
    name: "RSI with bands",
    short: "RSI+",
    overlay: false,
    format: "plain",
    decimals: 1,
    range: [0, 100],
    category: "Examples",
    description: "RSI, with levels and a smoothed line.",
    author: "Wyck",
    version: "1.0",
});

let length = input_int("length", 14, #{ min: 2, max: 500, label: "Length" });
let smooth = input_int("smooth", 5, #{ min: 1, max: 200, label: "Smoothing" });
let source = input_source("source", "close");
let upper = input_float("upper", 70.0, #{ min: 50.0, max: 100.0, label: "Overbought" });
let lower = input_float("lower", 30.0, #{ min: 0.0, max: 50.0, label: "Oversold" });

let value = rsi(source, length);

hline(upper);
hline(lower);
band(lower, upper);

plot("rsi", value, #{ color: "#7e57c2", width: 1.5, title: "RSI" });
plot("smooth", ema(value, smooth), #{ color: "#ff9800", width: 1, title: "Smoothed" });
