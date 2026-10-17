// Bands around a moving average, from the spread of the prices.

indicator(#{
    name: "Price bands",
    short: "PB",
    overlay: true,
    format: "price",
    category: "Custom",
    description: "A basis line with an upper and a lower band.",
});

let length = input_int("length", 20, #{ min: 2, max: 500, label: "Length" });
let mult = input_float("mult", 2.0, #{ min: 0.1, max: 10.0, step: 0.1, label: "Width in deviations" });

let bb = bollinger(close, length, mult);

plot("upper", bb.upper, #{ color: "#42a5f5", width: 1, title: "Upper" });
plot("basis", bb.basis, #{ color: "orange", width: 1, dash: "dashed", title: "Basis" });
plot("lower", bb.lower, #{ color: "#42a5f5", width: 1, title: "Lower" });
fill("upper", "lower", #{ alpha: 0.06 });
