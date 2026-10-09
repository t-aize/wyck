// A new indicator drawn on the prices.
//
// Save it (Ctrl+S) and it appears in the list of indicators. Every function is described in the
// reference on the right of the editor.

indicator(#{
    name: "My overlay",
    short: "MO",
    overlay: true,
    format: "price",
    category: "Custom",
    description: "What it shows, in one line.",
});

// What the user can change, in the settings of the indicator.
let length = input_int("length", 20, #{ min: 1, max: 500, label: "Length" });
let source = input_source("source", "close");

// The whole series is computed at once: there is no loop over the bars.
let average = sma(source, length);

plot("average", average, #{ color: "orange", width: 2, title: "Average" });
