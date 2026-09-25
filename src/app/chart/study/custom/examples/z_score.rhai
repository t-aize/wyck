// How many standard deviations the price is from its average.

indicator(#{
    name: "Z-score",
    short: "Z",
    overlay: false,
    format: "plain",
    decimals: 2,
    category: "Examples",
    description: "The distance from the average, in standard deviations.",
    author: "Wyck",
    version: "1.0",
});

let length = input_int("length", 50, #{ min: 2, max: 1000, label: "Length" });
let source = input_source("source", "close");

let z = (source - sma(source, length)) / stdev(source, length);

hline(-2);
hline(0);
hline(2);

plot("z", z, #{ color: "#e91e63", width: 1.5, title: "Z-score" });
plot("extreme", iff(abs(z) > 2, z, na), #{ style: "dots", color: "#ffeb3b", title: "Beyond 2" });
