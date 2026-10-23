// Channels and levels: a channel around the price, the pivot points of the previous period and
// the turning points, all on the prices.
//
// A channel gives a map: .basis, .upper and .lower. Pivot levels give .p, .r1, .s1 and so on.

indicator(#{
    name: "Channels and levels",
    short: "Chan",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "Bollinger, Keltner and Donchian channels, pivot point levels of the previous day, week or month, fractals and pivots.",
});

let length = input_int("length", 20, #{ min: 2, max: 200 });
let mult = input_float("mult", 2.0, #{ min: 0.1, max: 10.0, step: 0.1, label: "Width" });
let kind = input_choice("channel", "Bollinger", ["Bollinger", "Keltner (Pine)", "Keltner (Wilder)", "Donchian"]);
let pivots = input_choice("pivots", "off", ["off", "traditional", "fibonacci", "woodie", "classic", "camarilla", "dm"]);
let anchor = input_choice("anchor", "day", ["day", "week", "month", "year"]);
let marks = input_bool("marks", false, #{ label: "Fractals and pivots" });

let band = switch kind {
    "Bollinger" => bb(close, length, mult),
    "Keltner (Pine)" => kc(close, length, mult),
    "Keltner (Wilder)" => keltner(length, mult, 10),
    _ => donchian(length),
};
plot("basis", band.basis, #{ color: "orange" });
plot("upper", band.upper, #{ color: "blue" });
plot("lower", band.lower, #{ color: "blue" });
fill("upper", "lower", #{ alpha: 0.06 });

// The levels only show when asked for: a line of no value draws nothing, so the plots are always
// there and only their values change.
let levels = pivot_point_levels(if pivots == "off" { "traditional" } else { pivots }, anchor);
let shown = if pivots == "off" { na } else { 1.0 };
plot("pivot", levels.p * shown, #{ color: "gray", dash: "dashed" });
plot("resistance", levels.r1 * shown, #{ color: "red", dash: "dashed" });
plot("support", levels.s1 * shown, #{ color: "green", dash: "dashed" });

let fractal = fractals(2);
let top = pivothigh(5, 5);
let bottom = pivotlow(5, 5);
let on = if marks { 1.0 } else { na };
plot("fractal_high", fractal.high * on, #{ style: "dots", color: "red", width: 3 });
plot("fractal_low", fractal.low * on, #{ style: "dots", color: "green", width: 3 });
plot("pivot_high", top * on, #{ style: "dots", color: "orange", width: 3 });
plot("pivot_low", bottom * on, #{ style: "dots", color: "teal", width: 3 });
