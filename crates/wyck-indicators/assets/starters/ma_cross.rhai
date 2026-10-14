// Two moving averages drawn on the prices. The space between them is shaded.
//
// Change the lengths in the settings of the indicator once it is on a chart.

indicator(#{
    name: "Average cross",
    short: "AX",
    overlay: true,
    format: "price",
    category: "Custom",
    description: "A fast and a slow average, shaded between.",
});

let fast_length = input_int("fast", 9, #{ min: 1, max: 200, label: "Fast length" });
let slow_length = input_int("slow", 21, #{ min: 2, max: 500, label: "Slow length" });
let source = input_source("source", "close");

let fast = ema(source, fast_length);
let slow = ema(source, slow_length);

plot("fast", fast, #{ color: "#26a69a", width: 1.5, title: "Fast" });
plot("slow", slow, #{ color: "#ef5350", width: 1.5, title: "Slow" });
fill("fast", "slow", #{ alpha: 0.08 });
