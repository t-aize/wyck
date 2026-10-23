// Statistics lab: pick a statistic of the last bars and see it as a line.
//
// Each function takes a series and a length and gives a series. The first bars of a window have
// no value, and a window with a gap in it has none either.

indicator(#{
    name: "Statistics lab",
    short: "Stat",
    overlay: false,
    format: "plain",
    decimals: 3,
    category: "Examples",
    description: "Variance, deviation, median, mode, percentiles, correlation, ALMA, CMO, TSI and the other statistics of Pine, one at a time.",
});

let src = input_source("source", "close");
let length = input_int("length", 14, #{ min: 2, max: 200, tooltip: "Bars in each window." });
let kind = input_choice("statistic", "variance", [
    "variance",
    "deviation",
    "median",
    "mode",
    "range",
    "percent rank",
    "nearest rank",
    "interpolated rank",
    "alma",
    "swma",
    "cmo",
    "center of gravity",
    "tsi",
    "bars to the highest",
    "bars to the lowest",
    "rising",
    "falling",
    "correlation with volume",
    "regression deviation",
    "value when",
    "fixed gaps",
    "named average",
], #{ tooltip: "Which statistic the line shows." });

let stat = switch kind {
    "variance" => variance(src, length),
    "deviation" => dev(src, length),
    "median" => median(src, length),
    "mode" => mode(src, length),
    "range" => range(src, length),
    "percent rank" => percentrank(src, length),
    "nearest rank" => percentile_nearest_rank(src, length, 80),
    "interpolated rank" => percentile_linear_interpolation(src, length, 80),
    "alma" => alma(src, length),
    "swma" => swma(src),
    "cmo" => cmo(src, length),
    "center of gravity" => cog(src, length),
    "tsi" => tsi(src, length, length * 2),
    "bars to the highest" => highestbars(src, length),
    "bars to the lowest" => lowestbars(src, length),
    "rising" => rising(src, 2),
    "falling" => falling(src, 2),
    "correlation with volume" => correlation(src, volume, length),
    "regression deviation" => linreg_deviation(src, length),
    "value when" => valuewhen(cross_over(src, sma(src, length)), src, 0),
    "fixed gaps" => fixnan(pivothigh(src, 3, 3)),
    _ => ma(src, length, "ema"),
};

plot("stat", stat);
