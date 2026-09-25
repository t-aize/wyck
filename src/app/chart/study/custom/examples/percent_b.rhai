// Where the price is inside its Bollinger bands: 0 at the lower band, 1 at the upper band.

indicator(#{
    name: "Bollinger %B",
    short: "%B",
    overlay: false,
    format: "plain",
    decimals: 2,
    category: "Examples",
    description: "The position of the price between its Bollinger bands.",
    author: "Wyck",
    version: "1.0",
});

let length = input_int("length", 20, #{ min: 2, max: 500, label: "Length" });
let mult = input_float("mult", 2.0, #{ min: 0.1, max: 10.0, step: 0.1, label: "Multiplier" });
let source = input_source("source", "close");

let bands = bollinger(source, length, mult);
let percent = (source - bands.lower) / (bands.upper - bands.lower);

hline(0);
hline(0.5);
hline(1);

plot("percent", percent, #{ color: "#2962ff", width: 1.5, title: "%B" });
