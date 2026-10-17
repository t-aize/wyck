// TWO SYMBOLS
//
// Compare the symbol of the chart with another one. The other symbol is a setting of type symbol:
// that is how Wyck knows which prices to load. bars_of(name) then gives its open, high, low, close
// and volume on the bars of the chart (a bar the other symbol has no price for is flat at its last
// close). An empty name is the symbol of the chart itself.
//
// Both start at 100, so the paths can be compared whatever the prices are.

indicator(#{
    name: "Two symbols",
    short: "CMP",
    overlay: false,
    format: "plain",
    decimals: 2,
    category: "Examples",
    description: "The chart's symbol against another one, both starting at 100.",
});

let other_name = input_symbol("other", "GBPUSD", #{
    label: "Compare with",
    tooltip: "The name of a symbol of your account, such as GBPUSD or US500.",
});
let length = input_int("length", 50, #{ label: "Average length", min: 2, max: 500 });

let other = bars_of(other_name);        // a map: other.open, other.high, other.low, other.close, other.volume

let mine = close / close.at(0) * 100.0;
let theirs = other.close / other.close.at(0) * 100.0;
// How the first does against the second: above 100 when it did better since the first bar.
let relative = mine / theirs * 100.0;

hline(100);

plot("mine", mine, #{ title: "This symbol", color: "#42a5f5", width: 2 });
plot("theirs", theirs, #{ title: "Other symbol", color: "orange", width: 2 });
plot("relative", relative, #{ title: "Relative", color: "gray", width: 1, dash: "dotted" });
plot("trend", sma(relative, length), #{ title: "Relative average", color: "#ab47bc", width: 1.5 });
fill("mine", "theirs", #{ color: "#26a69a", down_color: "#ef5350", alpha: 0.12 });
