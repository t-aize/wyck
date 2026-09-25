// The volume of each bar, drawn up when the bar closed over its open and down when it closed under,
// with the average volume over it.

indicator(#{
    name: "Volume pressure",
    short: "VolP",
    overlay: false,
    format: "count",
    category: "Examples",
    description: "Volume in the color of the bar, with its average.",
    author: "Wyck",
    version: "1.0",
});

let length = input_int("length", 20, #{ min: 2, max: 500, label: "Average length" });

plot("volume", volume, #{ style: "histogram", up: close >= open, title: "Volume" });
plot("average", sma(volume, length), #{ color: "orange", width: 1.5, title: "Average" });
