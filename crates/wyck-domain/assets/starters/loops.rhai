// LOOPS, FUNCTIONS AND STATE
//
// Most indicators need no loop: use the functions on whole series (see the examples Series lab and Window lab). A loop is
// for a rule that remembers what it did on the bar before, like this trailing stop: it only goes
// one way until the price crosses it, then it flips to the other side.
//
// The language is Rhai (rhai.rs). What this file shows:
//   let, const, if / else, for ... in, while, loop / break, continue, switch, fn / return,
//   series(n, value) to make a series, and set(i, value) / at(i) to write and read one bar.
// A loop over the bars is slow: the script is stopped after 20 million operations or 4 seconds.

indicator(#{
    name: "Loops and state",
    short: "LOOP",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "An ATR trailing stop that flips, written with a loop.",
});

let length = input_int("length", 14, #{ label: "ATR length", min: 1, max: 200 });
let mult = input_float("mult", 3.0, #{ label: "ATR multiple", min: 0.5, max: 10.0, step: 0.5 });

// A function sees only its own arguments: not close, n, na, nor the constants of the script.
fn higher(a, b) {
    if a > b { a } else { b }
}

fn lower(a, b) {
    if a < b { a } else { b }
}

fn abs_of(x) {
    if x < 0.0 { -x } else { x }
}

// return leaves the function at once; without it the last value is the result.
fn direction_of(x) {
    if x > 0.0 { return 1; }
    if x < 0.0 { return -1; }
    0
}

const FIRST = 1;                      // a constant: readable in the loops, not in the functions

let volatility = atr(length);
let trail = series(n, na);            // the stop, bar by bar (no value until it exists)
let side = series(n, 0.0);            // 1 while long, -1 while short
let streak = series(n, 0.0);          // consecutive closes in the same direction (+3, -2...)
let stop = na;
let current = 1;
let flips = 0;
let longest = 0.0;

// for: once for each bar. `continue` goes to the next bar, `break` leaves the loop.
for i in FIRST..n {
    if volatility.at(i).is_nan() { continue; }       // the first bars of the ATR have no value
    let price = close.at(i);
    let width = mult * volatility.at(i);
    if current == 1 {
        stop = if stop.is_nan() { price - width } else { higher(stop, price - width) };
        if price < stop {
            current = -1;
            stop = price + width;
            flips += 1;
        }
    } else {
        stop = lower(stop, price + width);
        if price > stop {
            current = 1;
            stop = price - width;
            flips += 1;
        }
    }
    trail.set(i, stop);
    side.set(i, current);
}

// while: as long as the condition holds.
let i = FIRST;
while i < n {
    let step = direction_of(close.at(i) - close.at(i - 1));
    let before = streak.at(i - 1);
    if step == 0 {
        streak.set(i, 0);
    } else if before * step > 0.0 {
        streak.set(i, before + step);     // the run goes on
    } else {
        streak.set(i, step);              // a new run starts
    }
    if abs_of(streak.at(i)) > longest { longest = abs_of(streak.at(i)); }
    i += 1;
}

// loop: until break. Here, back from the last bar to the bar where the side last changed.
let back = n - 1;
loop {
    if back <= FIRST || side.at(back) != side.at(back - 1) { break; }
    back -= 1;
}

// switch: one branch for the value; _ is "anything else".
let word = switch current {
    1 => "long",
    -1 => "short",
    _ => "flat",
};
print(`Trailing stop: ${word} for ${n - 1 - back} bars, ${flips} flips, longest streak ${longest}`);

// The stop is drawn in two colors; na makes the line stop where the side changes.
plot("stop_long", iff(side > 0, trail, na), #{ title: "Stop (long)", color: "#26a69a", width: 2 });
plot("stop_short", iff(side < 0, trail, na), #{ title: "Stop (short)", color: "#ef5350", width: 2 });
