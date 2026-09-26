// Supertrend: a line that follows the price and flips side when the close goes through it.
// It is drawn twice, in two colors, so a change of trend shows.

indicator(#{
    name: "Supertrend",
    short: "ST",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "A trailing line that flips with the trend.",
    author: "Wyck",
    version: "1.0",
});

let length = input_int("length", 10, #{ min: 1, max: 200, label: "ATR length" });
let mult = input_float("mult", 3.0, #{ min: 0.5, max: 20.0, step: 0.5, label: "Multiplier" });

let st = supertrend(length, mult);
let rising = st.direction > 0;

plot("up", iff(rising, st.line, na), #{ color: "#26a69a", width: 2, title: "Up trend" });
plot("down", iff(!rising, st.line, na), #{ color: "#ef5350", width: 2, title: "Down trend" });
