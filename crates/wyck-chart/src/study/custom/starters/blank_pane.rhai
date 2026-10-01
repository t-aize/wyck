// A new indicator drawn in a pane of its own, under the prices.

indicator(#{
    name: "My oscillator",
    short: "MOSC",
    overlay: false,
    format: "plain",
    decimals: 2,
    category: "Custom",
    description: "What it shows, in one line.",
});

let length = input_int("length", 14, #{ min: 2, max: 500, label: "Length" });

let value = rsi(close, length);

// Lines that help to read it.
hline(30);
hline(70);

plot("value", value, #{ color: "#7e57c2", width: 1.5, title: "Value" });
