// MACD in a pane of its own: two lines and a histogram of their gap.

indicator(#{
    name: "Momentum",
    short: "MOM",
    overlay: false,
    format: "plain",
    decimals: 4,
    category: "Custom",
    description: "The gap between a fast and a slow average, with its signal.",
});

let fast = input_int("fast", 12, #{ min: 1, max: 200, label: "Fast length" });
let slow = input_int("slow", 26, #{ min: 2, max: 500, label: "Slow length" });
let signal = input_int("signal", 9, #{ min: 1, max: 200, label: "Signal length" });

let m = macd(close, fast, slow, signal);

hline(0);

plot("hist", m.hist, #{ style: "histogram", color: "#78909c", title: "Histogram" });
plot("macd", m.macd, #{ color: "#42a5f5", width: 1.5, title: "MACD" });
plot("signal", m.signal, #{ color: "#ffa726", width: 1.5, title: "Signal" });
