// A recursive filter written as a loop, to show how a series is built bar by bar.
//
// Prefer the functions that work on the whole series (sma, ema, ...): they are much faster. A loop
// is for what depends on its own previous value in a way no function covers. The script is stopped
// when it does too many operations.

indicator(#{
    name: "Recursive smoothing",
    short: "RS",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "y = y[1] + alpha * (price - y[1]), written as a loop.",
    author: "Wyck",
    version: "1.0",
});

let alpha = input_float("alpha", 0.1, #{ min: 0.01, max: 1.0, step: 0.01, label: "Alpha" });

let out = series(n, 0.0);
for i in 0..n {
    let price = close.at(i);
    if i == 0 {
        out.set(i, price);
    } else {
        let before = out.at(i - 1);
        out.set(i, before + alpha * (price - before));
    }
}

plot("smooth", out, #{ color: "#00bcd4", width: 2, title: "Smoothed price" });
