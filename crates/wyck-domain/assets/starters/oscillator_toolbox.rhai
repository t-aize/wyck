// THE READY MADE OSCILLATORS IN A PANE
//
// Pick one in the settings: RSI, stochastic, MACD, CCI, Williams R, MFI, DMI/ADX, ATR, true range,
// OBV, rate of change or momentum. They have different scales, so one is shown at a time, with
// the levels that help to read it. The same four plots serve all of them:
//   Main, Second and Third are lines; Histogram is the columns (MACD only).
//
// What a function gives:
//   stoch(length, smooth, d)  -> .k .d            macd(series, fast, slow, signal) -> .macd .signal .hist
//   dmi(length, smoothing)    -> .plus .minus .adx

indicator(#{
    name: "Oscillator toolbox",
    short: "OSC",
    overlay: false,
    format: "plain",
    decimals: 3,
    category: "Examples",
    description: "RSI, stochastic, MACD, CCI, MFI, DMI, ATR and more, one at a time.",
});

let which = input_choice("oscillator", "RSI", [
    "RSI", "Stochastic", "MACD", "CCI", "Williams R", "MFI", "DMI and ADX",
    "ATR", "True range", "OBV", "Rate of change", "Momentum",
], #{ label: "Oscillator" });
let length = input_int("length", 14, #{ label: "Length", min: 2, max: 200 });

let none = series(n, na);
let main = none;
let second = none;
let third = none;
let columns = none;

// The levels (hline) can depend on the choice: they are read when the script runs on the bars.
if which == "RSI" {
    main = rsi(close, length);
    hline(30);
    hline(70);
} else if which == "Stochastic" {
    let s = stoch(length, 3, 3);
    main = s.k;
    second = s.d;
    hline(20);
    hline(80);
} else if which == "MACD" {
    let m = macd(close, 12, 26, 9);
    main = m.macd;
    second = m.signal;
    columns = m.hist;
    hline(0);
} else if which == "CCI" {
    main = cci(length);
    hline(-100);
    hline(0);
    hline(100);
} else if which == "Williams R" {
    main = williams_r(length);
    hline(-80);
    hline(-20);
} else if which == "MFI" {
    main = mfi(length);
    hline(20);
    hline(80);
} else if which == "DMI and ADX" {
    let d = dmi(length, length);
    main = d.plus;
    second = d.minus;
    third = d.adx;
    hline(25);
} else if which == "ATR" {
    main = atr(length);
} else if which == "True range" {
    main = tr();
} else if which == "OBV" {
    main = obv();
} else if which == "Rate of change" {
    main = roc(close, length);
    hline(0);
} else {
    main = momentum(close, length);
    hline(0);
}

plot("columns", columns, #{ title: "Histogram", style: "histogram", color: "#78909c", up: columns >= 0.0 });
plot("main", main, #{ title: "Main", color: "#42a5f5", width: 1.5 });
plot("second", second, #{ title: "Second", color: "orange", width: 1.5 });
plot("third", third, #{ title: "Third", color: "#ab47bc", width: 1.5, dash: "dashed" });
