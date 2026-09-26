// Two moving averages, with the space between them shaded.
// The shading is green while the fast one is over the slow one, and red while it is under.

indicator(#{
    name: "Two averages",
    short: "2MA",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "A fast and a slow average, shaded between.",
    author: "Wyck",
    version: "1.0",
});

let fast_length = input_int("fast", 9, #{ min: 1, max: 500, label: "Fast length" });
let slow_length = input_int("slow", 21, #{ min: 2, max: 1000, label: "Slow length" });
let source = input_source("source", "close");
let kind = input_choice("kind", "EMA", ["SMA", "EMA", "WMA", "HMA"], #{ label: "Average" });

fn average(kind, source, length) {
    switch kind {
        "SMA" => sma(source, length),
        "WMA" => wma(source, length),
        "HMA" => hma(source, length),
        _ => ema(source, length),
    }
}

plot("fast", average(kind, source, fast_length), #{ color: "#2962ff", width: 1.5, title: "Fast" });
plot("slow", average(kind, source, slow_length), #{ color: "#ff9800", width: 1.5, title: "Slow" });
fill("fast", "slow", #{ color: "#26a69a", down_color: "#ef5350", alpha: 0.12 });
