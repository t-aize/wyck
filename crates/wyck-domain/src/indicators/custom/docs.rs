//! What the editor tells about the language: every function a script can call, the names it starts
//! with, and its keywords. The completion, the hover help and the reference panel all read this.
//!
//! A test checks that this list and what the engine registers are the same list, so a function
//! cannot be added without being described, nor described without existing.
//!
//! This file is over 600 lines on purpose: it is a table of text, one `Doc` per function, and the
//! test above is what keeps it honest. Nothing in it is logic that a split would make clearer.

/// Where a function is listed in the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Describing the indicator: its name, its inputs, what it draws.
    Declare,
    /// Averages and other measures over a window of bars.
    Windows,
    /// Indicators ready to use.
    Indicators,
    /// Conditions: crossings, choosing a value.
    Conditions,
    /// Arithmetic on every bar, and reading a series.
    Series,
    Colors,
    /// The calendar, the timeframe, the symbol and where a bar is in the chart.
    Chart,
    /// The chart drawing tools.
    Drawings,
}

impl Group {
    pub const ALL: [Self; 8] = [
        Self::Declare,
        Self::Series,
        Self::Windows,
        Self::Indicators,
        Self::Conditions,
        Self::Chart,
        Self::Colors,
        Self::Drawings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Declare => "Describing the indicator",
            Self::Series => "Series and arithmetic",
            Self::Windows => "Averages and windows",
            Self::Indicators => "Ready made indicators",
            Self::Conditions => "Conditions",
            Self::Chart => "Chart and calendar",
            Self::Colors => "Colors",
            Self::Drawings => "Chart drawings",
        }
    }
}

/// One function.
#[derive(Debug, Clone, Copy)]
pub struct Doc {
    pub name: &'static str,
    pub group: Group,
    /// How it is called: `sma(series, length) -> series`.
    pub signature: &'static str,
    pub summary: &'static str,
    /// A line that shows it in use, and what is inserted by the completion.
    pub example: &'static str,
}

const fn doc(
    group: Group,
    name: &'static str,
    signature: &'static str,
    summary: &'static str,
    example: &'static str,
) -> Doc {
    Doc {
        name,
        group,
        signature,
        summary,
        example,
    }
}

use Group::{Chart, Colors, Conditions, Declare, Drawings, Indicators, Series, Windows};

pub const FUNCTIONS: &[Doc] = &[
    doc(
        Drawings,
        "draw",
        "draw(id, tool, points, options?)",
        "Draws one script-owned object on the main chart. Tool names are the drawing tool names in snake_case. Points come from bar_point or time_point. Options accept color, width, text, levels, position, profile and other drawing settings. An id may be used only once per run; an indicator may draw up to 500 objects.",
        "draw(\"swing\", \"trend_line\", [bar_point(0, low.at(0)), bar_point(n - 1, high.at(n - 1))], #{ color: \"orange\" });",
    ),
    doc(
        Drawings,
        "bar_point",
        "bar_point(index, price) -> point",
        "A point at the time of an existing chart bar and a given price.",
        "bar_point(n - 1, close.at(n - 1))",
    ),
    doc(
        Drawings,
        "time_point",
        "time_point(unix_ms, price) -> point",
        "A point at Unix time in milliseconds and a given price. It can be in the future.",
        "time_point(1700000000000, 100.0)",
    ),
    doc(
        Drawings,
        "drawing_tools",
        "drawing_tools() -> array",
        "Lists the names of all chart drawing tools accepted by draw.",
        "drawing_tools()",
    ),
    doc(
        Declare,
        "indicator",
        "indicator(options)",
        "Says what the indicator is. Options: name, short, overlay (drawn on the prices), format (\"price\", \"plain\", \"count\"), decimals, range ([low, high] for a pane with a fixed scale), category, description, author, version. Call it once, at the top.",
        "indicator(#{ name: \"My average\", short: \"MA\", overlay: true });",
    ),
    doc(
        Declare,
        "input_int",
        "input_int(key, default, options?) -> int",
        "A whole number the user can change. Options: label, min, max, step, section (\"inputs\" or \"style\"). Width and text size names go to Style by default.",
        "let length = input_int(\"length\", 20, #{ min: 1, max: 500 });",
    ),
    doc(
        Declare,
        "input_float",
        "input_float(key, default, options?) -> float",
        "A number with decimals the user can change. Options: label, min, max, step, section (\"inputs\" or \"style\"). Opacity and width names go to Style by default.",
        "let mult = input_float(\"mult\", 2.0, #{ min: 0.1, max: 10.0, step: 0.1 });",
    ),
    doc(
        Declare,
        "input_bool",
        "input_bool(key, default, options?) -> bool",
        "A switch the user can turn on and off. Options: label, section (\"inputs\" or \"style\").",
        "let smooth = input_bool(\"smooth\", true);",
    ),
    doc(
        Declare,
        "input_source",
        "input_source(key, default, options?) -> series",
        "Which price the indicator reads: Open, High, Low, Close, HL2, HLC3, OHLC4 or HLCC4. Gives that price as a series. Options: label, section (\"inputs\" or \"style\"), group, tooltip.",
        "let src = input_source(\"source\", \"close\");",
    ),
    doc(
        Declare,
        "input_choice",
        "input_choice(key, default, choices, options?) -> text",
        "One of a few named choices. Gives the text of the one chosen. Options: label, section (\"inputs\" or \"style\").",
        "let kind = input_choice(\"kind\", \"EMA\", [\"SMA\", \"EMA\"]);",
    ),
    doc(
        Declare,
        "input_color",
        "input_color(key, default, options?) -> text",
        "A color the user can change. Gives it as text (#rrggbb), which plot takes as its color. Goes to Style by default. Options: label, section (\"inputs\" or \"style\").",
        "let up = input_color(\"up\", \"#26a69a\");",
    ),
    doc(
        Declare,
        "input_symbol",
        "input_symbol(key, default, options?) -> text",
        "The name of a symbol the user can change; empty means the symbol of the chart. Gives the text, which bars_of turns into that symbol's prices. Options: label, section, group, tooltip.",
        "let other = input_symbol(\"other\", \"\");",
    ),
    doc(
        Declare,
        "input_timeframe",
        "input_timeframe(key, default, options?) -> text",
        "A timeframe the user can change, such as 15m, 4h or 1D; empty means the chart's own. Gives the text. Options: label, section, group, tooltip.",
        "let tf = input_timeframe(\"tf\", \"4h\");",
    ),
    doc(
        Declare,
        "input_session",
        "input_session(key, default, options?) -> text",
        "A trading session written HHMM-HHMM, such as 0930-1600; empty means none. Gives the text. Options: label, section, group, tooltip.",
        "let hours = input_session(\"hours\", \"0930-1600\");",
    ),
    doc(
        Declare,
        "input_text",
        "input_text(key, default, options?) -> text",
        "Free text the user can change, up to 200 characters. Gives the text. Options: label, section, group, tooltip.",
        "let note = input_text(\"note\", \"\");",
    ),
    doc(
        Declare,
        "plot",
        "plot(key, series, options?)",
        "Draws a series. The key names it in the settings. Options: title, color, width, style (\"line\", \"histogram\", \"dots\"), dash (\"solid\", \"dashed\", \"dotted\"), offset (bars to move it), up (a condition: for a histogram, columns where it holds are drawn up).",
        "plot(\"ma\", sma(close, 20), #{ color: \"orange\", width: 2 });",
    ),
    doc(
        Declare,
        "hline",
        "hline(value)",
        "A horizontal line at a value, in the pane of the indicator.",
        "hline(70);",
    ),
    doc(
        Declare,
        "band",
        "band(low, high)",
        "A shaded band between two values, in the pane of the indicator.",
        "band(30, 70);",
    ),
    doc(
        Declare,
        "fill",
        "fill(key_a, key_b, options?)",
        "Shades the space between two plots drawn before. Options: color, alpha (0 to 1), down_color (the color where the second is above).",
        "fill(\"upper\", \"lower\", #{ alpha: 0.08 });",
    ),
    doc(
        Series,
        "bars_of",
        "bars_of(symbol) -> map",
        "The prices of a symbol on the bars of the chart: a map with open, high, low, close and volume. An empty name is the chart's own symbol. Any other name must come from input_symbol, which is how Wyck knows to load it. A bar the other symbol has no price for is flat at its last close. The prices are read when the chart loads.",
        "let b = bars_of(input_symbol(\"other\", \"GBPUSD\")); plot(\"ratio\", close / b.close);",
    ),
    doc(
        Series,
        "abs",
        "abs(series) -> series",
        "The size of each value, without its sign.",
        "abs(close - open)",
    ),
    doc(
        Series,
        "sqrt",
        "sqrt(series) -> series",
        "The square root of each value.",
        "sqrt(volume)",
    ),
    doc(
        Series,
        "ln",
        "ln(series) -> series",
        "The natural logarithm of each value.",
        "ln(close)",
    ),
    doc(
        Series,
        "log10",
        "log10(series) -> series",
        "The base 10 logarithm of each value.",
        "log10(close)",
    ),
    doc(
        Series,
        "exp",
        "exp(series) -> series",
        "e raised to each value.",
        "exp(x)",
    ),
    doc(
        Series,
        "sin",
        "sin(series) -> series",
        "The sine of each value, in radians.",
        "sin(bar_index / 10.0)",
    ),
    doc(
        Series,
        "cos",
        "cos(series) -> series",
        "The cosine of each value, in radians.",
        "cos(bar_index / 10.0)",
    ),
    doc(
        Series,
        "floor",
        "floor(series) -> series",
        "Each value rounded down.",
        "floor(close)",
    ),
    doc(
        Series,
        "ceil",
        "ceil(series) -> series",
        "Each value rounded up.",
        "ceil(close)",
    ),
    doc(
        Series,
        "round",
        "round(series, decimals?) -> series",
        "Each value rounded to the nearest whole number, or to some decimals.",
        "round(close)",
    ),
    doc(
        Series,
        "sign",
        "sign(series) -> series",
        "1 for a positive value, -1 for a negative one, 0 for zero.",
        "sign(close - open)",
    ),
    doc(
        Series,
        "min",
        "min(a, b, c?) -> series, min(array) -> value",
        "The smallest of two or three, bar by bar; any can be a number. Given an array it gives the smallest of its numbers, or bar by bar of its series.",
        "min(close, open)",
    ),
    doc(
        Series,
        "max",
        "max(a, b, c?) -> series, max(array) -> value",
        "The largest of two or three, bar by bar; any can be a number. Given an array it gives the largest of its numbers, or bar by bar of its series.",
        "max(close, open)",
    ),
    doc(
        Series,
        "pow",
        "pow(series, exponent) -> series",
        "Each value raised to a power.",
        "pow(close, 2)",
    ),
    doc(
        Series,
        "clamp",
        "clamp(series, low, high) -> series",
        "Each value kept between two limits.",
        "clamp(rsi(close, 14), 20.0, 80.0)",
    ),
    doc(
        Series,
        "nz",
        "nz(series, fallback?) -> series",
        "Replaces the bars that have no value (the first ones of an average) with a number, 0 unless given.",
        "nz(sma(close, 20), 0.0)",
    ),
    doc(
        Series,
        "is_na",
        "is_na(series) -> series",
        "1 on the bars that have no value, 0 on the others.",
        "is_na(sma(close, 20))",
    ),
    doc(
        Series,
        "shift",
        "shift(series, bars) -> series",
        "The series moved later by some bars: the value of that many bars ago. A negative number looks ahead. close[1] is the same as shift(close, 1).",
        "shift(close, 1)",
    ),
    doc(
        Series,
        "change",
        "change(series, bars?) -> series",
        "The difference from some bars ago (1 unless given).",
        "change(close, 5)",
    ),
    doc(
        Series,
        "cum",
        "cum(series) -> series",
        "The running total.",
        "cum(volume)",
    ),
    doc(
        Series,
        "barssince",
        "barssince(condition) -> series",
        "How many bars ago the condition last held (0 on a bar where it holds).",
        "barssince(close > open)",
    ),
    doc(
        Series,
        "series",
        "series(length, value) -> series",
        "A series of a given length with the same value on every bar. Use n for the length of the chart. For a loop that must remember.",
        "let out = series(n, 0.0);",
    ),
    doc(
        Series,
        "set",
        "set(series, bar, value)",
        "Sets the value of one bar of a series the script made. A loop over the bars is slow: prefer the functions on whole series.",
        "out.set(i, 1.0);",
    ),
    doc(
        Series,
        "at",
        "at(series, bar) -> float",
        "The value of one bar, counted from the first. No value (NaN) past the ends.",
        "close.at(0)",
    ),
    doc(
        Series,
        "last",
        "last(series) -> float",
        "The value of the last bar.",
        "close.last()",
    ),
    doc(
        Series,
        "len",
        "len(series) -> int",
        "How many bars the series has.",
        "close.len()",
    ),
    doc(
        Series,
        "to_string",
        "to_string(series) -> text",
        "A short description of a series, for print.",
        "print(close.to_string());",
    ),
    doc(
        Series,
        "to_debug",
        "to_debug(series) -> text",
        "A short description of a series, for debugging.",
        "debug(close);",
    ),
    doc(
        Windows,
        "sma",
        "sma(series, length) -> series",
        "Simple moving average.",
        "sma(close, 20)",
    ),
    doc(
        Windows,
        "ema",
        "ema(series, length) -> series",
        "Exponential moving average.",
        "ema(close, 20)",
    ),
    doc(
        Windows,
        "wma",
        "wma(series, length) -> series",
        "Linearly weighted moving average: the newest bar weighs most.",
        "wma(close, 20)",
    ),
    doc(
        Windows,
        "hma",
        "hma(series, length) -> series",
        "Hull moving average: fast, with little lag.",
        "hma(close, 20)",
    ),
    doc(
        Windows,
        "rma",
        "rma(series, length) -> series",
        "Wilder's moving average, the one RSI and ATR use.",
        "rma(close, 14)",
    ),
    doc(
        Windows,
        "vwma",
        "vwma(series, length) -> series",
        "Moving average weighted by volume.",
        "vwma(close, 20)",
    ),
    doc(
        Windows,
        "stdev",
        "stdev(series, length) -> series, stdev(array) -> decimal",
        "Standard deviation over a window; given an array of numbers, theirs.",
        "stdev(close, 20)",
    ),
    doc(
        Windows,
        "highest",
        "highest(series, length) -> series",
        "The highest value of the last bars.",
        "highest(high, 20)",
    ),
    doc(
        Windows,
        "lowest",
        "lowest(series, length) -> series",
        "The lowest value of the last bars.",
        "lowest(low, 20)",
    ),
    doc(
        Windows,
        "sum",
        "sum(series, length) -> series, sum(array) -> decimal",
        "The sum of the last bars; given an array of numbers, the sum of them.",
        "sum(volume, 10)",
    ),
    doc(
        Windows,
        "linreg",
        "linreg(series, length, offset?) -> series",
        "The value of the least squares line through the last bars, at the newest. With an offset, the value that many bars back along the line.",
        "linreg(close, 20)",
    ),
    doc(
        Windows,
        "slope",
        "slope(series, length) -> series",
        "The slope of the least squares line through the last bars, per bar.",
        "slope(close, 20)",
    ),
    doc(
        Windows,
        "roc",
        "roc(series, length) -> series",
        "Rate of change in percent over some bars.",
        "roc(close, 10)",
    ),
    doc(
        Windows,
        "momentum",
        "momentum(series, length) -> series",
        "The difference from some bars ago.",
        "momentum(close, 10)",
    ),
    doc(
        Windows,
        "rsi",
        "rsi(series, length) -> series",
        "Relative strength index, from 0 to 100.",
        "rsi(close, 14)",
    ),
    doc(
        Indicators,
        "tr",
        "tr() -> series",
        "True range of each bar. Every indicator that reads prices (tr, atr, cci, williams_r, obv, mfi, psar, stoch, dmi, supertrend, donchian, keltner and the volume ones) also takes a map of bars from bars_of first: tr(bars), atr(bars, 14).",
        "tr()",
    ),
    doc(
        Indicators,
        "atr",
        "atr(length, method?) -> series",
        "Average true range, smoothed the way of Wilder. method (\"sma\", \"ema\", \"wma\", \"rma\" or \"hma\") smooths it another way.",
        "atr(14)",
    ),
    doc(
        Indicators,
        "cci",
        "cci(series?, length) -> series",
        "Commodity channel index of the typical price, or of the series given.",
        "cci(20)",
    ),
    doc(
        Indicators,
        "williams_r",
        "williams_r(length) -> series",
        "Williams %R, from -100 to 0.",
        "williams_r(14)",
    ),
    doc(
        Indicators,
        "obv",
        "obv() -> series",
        "On balance volume.",
        "obv()",
    ),
    doc(
        Indicators,
        "mfi",
        "mfi(series?, length) -> series",
        "Money flow index, from 0 to 100, of the typical price or of the series given, weighted by the volume.",
        "mfi(14)",
    ),
    doc(
        Indicators,
        "vwap",
        "vwap(series?, anchor?, mult?) -> series or map",
        "Volume weighted average price. By default it starts over every day and reads the typical price. With an anchor (\"day\", \"week\", \"month\" or \"year\") it starts over with that period, and with a mult as well it gives a map: .vwap and bands .upper and .lower at mult deviations.",
        "vwap()",
    ),
    doc(
        Indicators,
        "psar",
        "psar(start, step, max) -> series",
        "Parabolic SAR.",
        "psar(0.02, 0.02, 0.2)",
    ),
    doc(
        Indicators,
        "macd",
        "macd(series, fast, slow, signal) -> map",
        "MACD. Gives a map with macd, signal and hist.",
        "let m = macd(close, 12, 26, 9);",
    ),
    doc(
        Indicators,
        "bollinger",
        "bollinger(series, length, mult) -> map",
        "Bollinger bands. Gives a map with basis, upper and lower.",
        "let bb = bollinger(close, 20, 2.0);",
    ),
    doc(
        Indicators,
        "stoch",
        "stoch(length, smooth, d) -> map",
        "Slow stochastic oscillator. Gives a map with k and d. stoch(source, high, low, length) gives the raw %K of Pine, a series with no smoothing.",
        "let s = stoch(14, 3, 3);",
    ),
    doc(
        Indicators,
        "dmi",
        "dmi(length, smoothing) -> map",
        "Directional movement. Gives a map with plus, minus and adx.",
        "let d = dmi(14, 14);",
    ),
    doc(
        Indicators,
        "supertrend",
        "supertrend(length, mult) -> map",
        "Supertrend. Gives a map with line and direction (1 up, -1 down).",
        "let st = supertrend(10, 3.0);",
    ),
    doc(
        Indicators,
        "donchian",
        "donchian(length) -> map",
        "Donchian channel. Gives a map with upper, lower and basis.",
        "let dc = donchian(20);",
    ),
    doc(
        Indicators,
        "keltner",
        "keltner(length, mult, atr_length) -> map",
        "Keltner channel. Gives a map with basis, upper and lower.",
        "let kc = keltner(20, 2.0, 10);",
    ),
    doc(
        Conditions,
        "cross_over",
        "cross_over(a, b) -> series",
        "1 on the bars where a goes from under b to over it. b can be a number.",
        "cross_over(ema(close, 9), ema(close, 21))",
    ),
    doc(
        Conditions,
        "cross_under",
        "cross_under(a, b) -> series",
        "1 on the bars where a goes from over b to under it. b can be a number.",
        "cross_under(rsi(close, 14), 70)",
    ),
    doc(
        Conditions,
        "cross",
        "cross(a, b) -> series",
        "1 on the bars where a crosses b in either direction.",
        "cross(close, sma(close, 20))",
    ),
    doc(
        Conditions,
        "iff",
        "iff(condition, a, b) -> series",
        "a on the bars where the condition holds, b on the others. Each of them is a series or a number.",
        "iff(close > open, 1, -1)",
    ),
    doc(
        Colors,
        "rgb",
        "rgb(red, green, blue) -> text",
        "A color from its parts, each 0 to 255, as text (#rrggbb).",
        "rgb(255, 152, 0)",
    ),
    doc(
        Windows,
        "variance",
        "variance(series, length, biased?) -> series",
        "The variance over the last bars: the mean of the squared gaps to the mean. With biased false it divides by one less than the number of bars (the sample variance).",
        "let v = variance(close, 20);",
    ),
    doc(
        Windows,
        "dev",
        "dev(series, length) -> series",
        "The mean absolute deviation over the last bars: how far the values are from their mean, on average.",
        "let d = dev(close, 20);",
    ),
    doc(
        Windows,
        "median",
        "median(series, length) -> series, median(array) -> decimal",
        "The median over the last bars: the middle value, or the mean of the two in the middle. Given an array of numbers, theirs.",
        "let m = median(close, 20);",
    ),
    doc(
        Windows,
        "mode",
        "mode(series, length) -> series",
        "The value that comes most often over the last bars, the smallest of them when several tie.",
        "let m = mode(close, 20);",
    ),
    doc(
        Windows,
        "range",
        "range(series, length) -> series",
        "The highest value minus the lowest over the last bars.",
        "let r = range(close, 20);",
    ),
    doc(
        Windows,
        "percentrank",
        "percentrank(series, length) -> series",
        "The share, from 0 to 100, of the length values before this bar that are at or below this bar's value.",
        "let r = percentrank(close, 100);",
    ),
    doc(
        Windows,
        "percentile_nearest_rank",
        "percentile_nearest_rank(series, length, percentage) -> series",
        "The value at a percentage of the last bars, by the nearest rank: the smallest value such that at least that share of the values is at or below it.",
        "let p = percentile_nearest_rank(close, 50, 90);",
    ),
    doc(
        Windows,
        "percentile_linear_interpolation",
        "percentile_linear_interpolation(series, length, percentage) -> series",
        "The value at a percentage of the last bars, interpolated between the two nearest ranks.",
        "let p = percentile_linear_interpolation(close, 50, 25);",
    ),
    doc(
        Windows,
        "alma",
        "alma(series, length, offset?, sigma?) -> series",
        "The Arnaud Legoux moving average: the window weighted by a gaussian. offset (0 to 1, default 0.85) puts the peak towards the newest bar and sigma (default 6) sets how narrow it is.",
        "let a = alma(close, 9);",
    ),
    doc(
        Windows,
        "swma",
        "swma(series) -> series",
        "The symmetrically weighted moving average of the last four values: 1, 2, 2 and 1 over 6.",
        "let s = swma(close);",
    ),
    doc(
        Windows,
        "cmo",
        "cmo(series, length) -> series",
        "The Chande momentum oscillator, from -100 to 100: gains minus losses over their sum, for the changes of the last bars.",
        "let m = cmo(close, 9);",
    ),
    doc(
        Windows,
        "cog",
        "cog(series, length) -> series",
        "The center of gravity oscillator: the weighted center of the last values, the newest weighing 1, negated.",
        "let c = cog(close, 10);",
    ),
    doc(
        Windows,
        "tsi",
        "tsi(series, short, long) -> series",
        "The true strength index, from -1 to 1: the momentum smoothed twice over the absolute momentum smoothed twice.",
        "let t = tsi(close, 13, 25);",
    ),
    doc(
        Windows,
        "highestbars",
        "highestbars(series, length) -> series",
        "How many bars back the highest value of the last bars is: 0 when it is this bar, negative otherwise. The most recent wins a tie.",
        "let h = highestbars(high, 20);",
    ),
    doc(
        Windows,
        "lowestbars",
        "lowestbars(series, length) -> series",
        "How many bars back the lowest value of the last bars is: 0 when it is this bar, negative otherwise. The most recent wins a tie.",
        "let l = lowestbars(low, 20);",
    ),
    doc(
        Conditions,
        "rising",
        "rising(series, length) -> series",
        "1 where the series rose on each of the last length bars, 0 where it did not.",
        "let up = rising(close, 3);",
    ),
    doc(
        Conditions,
        "falling",
        "falling(series, length) -> series",
        "1 where the series fell on each of the last length bars, 0 where it did not.",
        "let down = falling(close, 3);",
    ),
    doc(
        Windows,
        "correlation",
        "correlation(a, b, length) -> series",
        "The Pearson correlation of two series over the last bars, from -1 to 1. It has no value where one of them is flat.",
        "let c = correlation(close, volume, 20);",
    ),
    doc(
        Series,
        "valuewhen",
        "valuewhen(condition, series, occurrence) -> series",
        "The value of the series on the bar where the condition held, occurrence times ago: 0 is the latest, this bar included.",
        "let last_high = valuewhen(cross_over(close, sma(close, 20)), high, 0);",
    ),
    doc(
        Series,
        "fixnan",
        "fixnan(series) -> series",
        "The series with each bar that has no value given the last value there was.",
        "let filled = fixnan(pivothigh(high, 5, 5));",
    ),
    doc(
        Windows,
        "linreg_deviation",
        "linreg_deviation(series, length) -> series",
        "How far the values of the last bars are from their regression line: the square root of the mean of the squared gaps. Add and subtract it from linreg for a channel.",
        "let channel = linreg(close, 100) + 2 * linreg_deviation(close, 100);",
    ),
    doc(
        Windows,
        "ma",
        "ma(series, length, method) -> series",
        "A moving average chosen by name: \"sma\", \"ema\", \"wma\", \"rma\" (Wilder) or \"hma\".",
        "let m = ma(close, 20, \"ema\");",
    ),
    doc(
        Windows,
        "pivothigh",
        "pivothigh(series?, left, right) -> series",
        "A high that the left bars before it do not top and the right bars after it stay under (a tie on the left counts, one on the right does not). It is given on the bar where it is known, right bars after it, and has no value elsewhere. Without a series, the highs of the chart.",
        "let top = pivothigh(5, 5);",
    ),
    doc(
        Windows,
        "pivotlow",
        "pivotlow(series?, left, right) -> series",
        "A low that the left bars before it are not under and the right bars after it stay over. It is given on the bar where it is known, right bars after it. Without a series, the lows of the chart.",
        "let bottom = pivotlow(5, 5);",
    ),
    doc(
        Indicators,
        "fractals",
        "fractals(wing) -> map",
        "The Williams fractals: a high above the wing bars on each side and a low under them, each on its own bar (so it is only known wing bars later). Gives .high and .low.",
        "let f = fractals(2); plot(\"high\", f.high, #{ style: \"dots\" });",
    ),
    doc(
        Indicators,
        "aroon",
        "aroon(length) -> map",
        "Aroon, from 0 to 100: how recent the highest high and the lowest low of the last bars are. Gives .up and .down.",
        "let a = aroon(25); plot(\"up\", a.up);",
    ),
    doc(
        Indicators,
        "money_flow",
        "money_flow() -> series",
        "The money flow of each bar: where the close sits in the range, from -1 (at the low) to 1 (at the high), times the volume. It is 0 for a bar with no range. Like every indicator that reads prices, it also takes a map of bars first: money_flow(bars).",
        "let flow = money_flow();",
    ),
    doc(
        Indicators,
        "accdist",
        "accdist() -> series",
        "Accumulation/distribution: the running total of money_flow.",
        "plot(\"ad\", accdist());",
    ),
    doc(
        Indicators,
        "pvt",
        "pvt() -> series",
        "The price volume trend: the running total of the relative change of the close weighted by the volume.",
        "plot(\"pvt\", pvt());",
    ),
    doc(
        Indicators,
        "nvi",
        "nvi() -> series",
        "The negative volume index, starting at 1: it follows the price only on bars where the volume fell.",
        "plot(\"nvi\", nvi());",
    ),
    doc(
        Indicators,
        "pvi",
        "pvi() -> series",
        "The positive volume index, starting at 1: it follows the price only on bars where the volume rose.",
        "plot(\"pvi\", pvi());",
    ),
    doc(
        Indicators,
        "wad",
        "wad() -> series",
        "The accumulation/distribution of Williams: the running total of how far the close is from the true low on a rise and from the true high on a fall.",
        "plot(\"wad\", wad());",
    ),
    doc(
        Indicators,
        "wvad",
        "wvad() -> series",
        "The variable accumulation/distribution of Williams: the body of each bar over its range, times the volume.",
        "plot(\"wvad\", wvad());",
    ),
    doc(
        Indicators,
        "iii",
        "iii() -> series",
        "The intraday intensity index: where the close sits in the range per unit of volume, as a running total.",
        "plot(\"iii\", iii());",
    ),
    doc(
        Indicators,
        "bb",
        "bb(series, length, mult) -> map",
        "The name of Pine for bollinger: the average of the last bars with bands at mult standard deviations. Gives .basis, .upper and .lower.",
        "let b = bb(close, 20, 2); plot(\"upper\", b.upper);",
    ),
    doc(
        Indicators,
        "bbw",
        "bbw(series, length, mult) -> series",
        "The width of the Bollinger bands over their basis.",
        "let w = bbw(close, 20, 2);",
    ),
    doc(
        Indicators,
        "kc",
        "kc(series, length, mult, use_true_range?) -> map",
        "The Keltner channel of Pine: an exponential average with bands at mult times the exponential average of the true range (or of the high minus the low when use_true_range is false). Gives .basis, .upper and .lower. keltner is the other kind, with a Wilder ATR.",
        "let k = kc(close, 20, 1.5); plot(\"upper\", k.upper);",
    ),
    doc(
        Indicators,
        "kcw",
        "kcw(series, length, mult) -> series",
        "The width of the Keltner channel over its basis.",
        "let w = kcw(close, 20, 1.5);",
    ),
    doc(
        Indicators,
        "pivot_point_levels",
        "pivot_point_levels(kind, anchor) -> map",
        "The pivot point levels of the previous day, week, month or year, on every bar of the current one. kind: \"traditional\", \"fibonacci\", \"woodie\", \"classic\", \"camarilla\" or \"dm\". anchor: \"day\", \"week\", \"month\" or \"year\". Gives .p, .r1, .s1, .r2, .s2, .r3 and .s3 (a kind may have fewer levels).",
        "let l = pivot_point_levels(\"traditional\", \"day\"); plot(\"p\", l.p);",
    ),
    doc(
        Chart,
        "year",
        "year() -> series",
        "The year of each bar, in the time zone of the chart.",
        "let y = year();",
    ),
    doc(
        Chart,
        "month",
        "month() -> series",
        "The month of each bar, 1 to 12, in the time zone of the chart.",
        "let m = month();",
    ),
    doc(
        Chart,
        "dayofmonth",
        "dayofmonth() -> series",
        "The day of the month of each bar, 1 to 31.",
        "let d = dayofmonth();",
    ),
    doc(
        Chart,
        "dayofweek",
        "dayofweek() -> series",
        "The day of the week of each bar: Sunday is 1, Monday 2, ... Saturday 7, as in Pine.",
        "let monday = dayofweek() == 2;",
    ),
    doc(
        Chart,
        "hour",
        "hour() -> series",
        "The hour of each bar, 0 to 23, in the time zone of the chart.",
        "let opening = hour() == 9;",
    ),
    doc(
        Chart,
        "minute",
        "minute() -> series",
        "The minute of each bar, 0 to 59.",
        "let m = minute();",
    ),
    doc(
        Chart,
        "second",
        "second() -> series",
        "The second of each bar, 0 to 59.",
        "let s = second();",
    ),
    doc(
        Chart,
        "weekofyear",
        "weekofyear() -> series",
        "The week of the year of each bar, 1 to 53, by the ISO rule (a week begins on Monday).",
        "let w = weekofyear();",
    ),
    doc(
        Chart,
        "utc_offset",
        "utc_offset() -> series",
        "The offset of the time zone of the chart from UTC at each bar, in minutes. It changes with daylight saving time.",
        "let offset = utc_offset();",
    ),
    doc(
        Chart,
        "day",
        "day() -> series",
        "The day of each bar counted from 1970-01-01, in the time zone of the chart.",
        "let d = day();",
    ),
    doc(
        Chart,
        "period_key",
        "period_key(anchor) -> series",
        "A number that is the same for every bar of a day, week, month or year (anchor: \"day\", \"week\", \"month\" or \"year\") and different for the next one. Weeks begin on Monday.",
        "let week = period_key(\"week\");",
    ),
    doc(
        Chart,
        "is_new_period",
        "is_new_period(anchor) -> series",
        "1 on the first bar of each day, week, month or year, 0 elsewhere.",
        "let first_of_day = is_new_period(\"day\");",
    ),
    doc(
        Chart,
        "in_session",
        "in_session(session) -> series",
        "1 on the bars inside a session such as \"0930-1600\" in the time zone of the chart (the end is not inside), 0 elsewhere. A session such as \"2200-0600\" goes over midnight.",
        "let open = in_session(\"0930-1600\");",
    ),
    doc(
        Chart,
        "timeframe_seconds",
        "timeframe_seconds() -> int",
        "The length of a bar of the chart in seconds, 0 when it is not known or for ticks.",
        "let seconds = timeframe_seconds();",
    ),
    doc(
        Chart,
        "timeframe_multiplier",
        "timeframe_multiplier() -> int",
        "How many of its unit a bar is: 15 for 15 minutes, 240 for 4 hours (in minutes), 3 for 3 days, 1 for a week.",
        "let m = timeframe_multiplier();",
    ),
    doc(
        Chart,
        "timeframe_code",
        "timeframe_code() -> text",
        "The code of the timeframe, such as \"M15\", \"H4\", \"D1\", or \"\" when it is not known.",
        "let code = timeframe_code();",
    ),
    doc(
        Chart,
        "timeframe_is_seconds",
        "timeframe_is_seconds() -> bool",
        "Whether a bar is a number of seconds.",
        "if timeframe_is_seconds() { }",
    ),
    doc(
        Chart,
        "timeframe_is_intraday",
        "timeframe_is_intraday() -> bool",
        "Whether a bar is shorter than a day.",
        "if timeframe_is_intraday() { }",
    ),
    doc(
        Chart,
        "timeframe_is_minutes",
        "timeframe_is_minutes() -> bool",
        "Whether a bar is a number of minutes or hours.",
        "if timeframe_is_minutes() { }",
    ),
    doc(
        Chart,
        "timeframe_is_daily",
        "timeframe_is_daily() -> bool",
        "Whether a bar is a number of days.",
        "if timeframe_is_daily() { }",
    ),
    doc(
        Chart,
        "timeframe_is_weekly",
        "timeframe_is_weekly() -> bool",
        "Whether a bar is a number of weeks.",
        "if timeframe_is_weekly() { }",
    ),
    doc(
        Chart,
        "timeframe_is_monthly",
        "timeframe_is_monthly() -> bool",
        "Whether a bar is a number of months.",
        "if timeframe_is_monthly() { }",
    ),
    doc(
        Chart,
        "syminfo_ticker",
        "syminfo_ticker() -> text",
        "The name of the symbol of the chart, \"\" when it is not known.",
        "let name = syminfo_ticker();",
    ),
    doc(
        Chart,
        "syminfo_digits",
        "syminfo_digits() -> int",
        "The digits of the prices of the symbol (5 for EURUSD), 0 when it is not known.",
        "let digits = syminfo_digits();",
    ),
    doc(
        Chart,
        "syminfo_mintick",
        "syminfo_mintick() -> decimal",
        "The smallest step of a price, in the integers price series hold: 1 for 5 digits, 100 for 3 digits.",
        "let tick = syminfo_mintick();",
    ),
    doc(
        Chart,
        "price_scale",
        "price_scale() -> int",
        "What a price is multiplied by in a series: close holds 108501 for the price 1.08501, so this is 100000.",
        "let scale = price_scale();",
    ),
    doc(
        Chart,
        "to_raw",
        "to_raw(price) -> decimal or series",
        "A price as a person writes it, in the integers of a series: to_raw(1.1) is 110000. Use it for hline and for comparing with close.",
        "hline(to_raw(1.1));",
    ),
    doc(
        Chart,
        "from_raw",
        "from_raw(price) -> decimal or series",
        "A price of a series as a person reads it: from_raw(close) holds 1.08501 where close holds 108501.",
        "let readable = from_raw(close);",
    ),
    doc(
        Chart,
        "round_to_mintick",
        "round_to_mintick(series) -> series",
        "A price series rounded to the smallest step of the symbol.",
        "let rounded = round_to_mintick(close);",
    ),
    doc(
        Chart,
        "barstate_isfirst",
        "barstate_isfirst() -> series",
        "1 on the first bar of the chart.",
        "let first = barstate_isfirst();",
    ),
    doc(
        Chart,
        "barstate_islast",
        "barstate_islast() -> series",
        "1 on the last bar of the chart.",
        "let last = barstate_islast();",
    ),
    doc(
        Chart,
        "barstate_isrealtime",
        "barstate_isrealtime() -> series",
        "1 on the last bar while it is still forming, 0 on the others and when the last bar is closed.",
        "let live = barstate_isrealtime();",
    ),
    doc(
        Chart,
        "barstate_isconfirmed",
        "barstate_isconfirmed() -> series",
        "1 on every bar that is closed, so 0 only on a last bar that is still forming.",
        "let closed = barstate_isconfirmed();",
    ),
    doc(
        Chart,
        "last_bar_index",
        "last_bar_index() -> int",
        "The index of the last bar of the chart: n - 1.",
        "let last = last_bar_index();",
    ),
    doc(
        Series,
        "tan",
        "tan(series) -> series",
        "The tangent of each value, in radians.",
        "tan(close)",
    ),
    doc(
        Series,
        "asin",
        "asin(series) -> series",
        "The arc sine of each value, in radians. A value outside -1 to 1 has no result.",
        "asin(x)",
    ),
    doc(
        Series,
        "acos",
        "acos(series) -> series",
        "The arc cosine of each value, in radians. A value outside -1 to 1 has no result.",
        "acos(x)",
    ),
    doc(
        Series,
        "atan",
        "atan(series) -> series",
        "The arc tangent of each value, in radians.",
        "atan(x)",
    ),
    doc(
        Series,
        "todegrees",
        "todegrees(series) -> series",
        "Each angle in radians as degrees.",
        "todegrees(atan(x))",
    ),
    doc(
        Series,
        "toradians",
        "toradians(series) -> series",
        "Each angle in degrees as radians.",
        "toradians(45)",
    ),
    doc(
        Series,
        "log",
        "log(series) -> series",
        "The natural logarithm of each value, as Pine names it (the same as ln).",
        "log(close)",
    ),
    doc(
        Series,
        "pi",
        "pi() -> decimal",
        "The number pi.",
        "let half = pi() / 2.0;",
    ),
    doc(
        Series,
        "e",
        "e() -> decimal",
        "The number e, the base of the natural logarithm.",
        "let x = e();",
    ),
    doc(
        Series,
        "phi",
        "phi() -> decimal",
        "The golden ratio, 1.618...",
        "let g = phi();",
    ),
    doc(
        Series,
        "rphi",
        "rphi() -> decimal",
        "The inverse of the golden ratio, 0.618...",
        "let g = rphi();",
    ),
    doc(
        Series,
        "avg",
        "avg(a, b) -> series, avg(array) -> decimal",
        "The mean of two values bar by bar (either can be a number), or the mean of an array of numbers.",
        "let mid = avg(high, low);",
    ),
    doc(
        Series,
        "percentile",
        "percentile(array, percentage) -> decimal",
        "The value at a percentage (0 to 100) of an array of numbers, interpolated between the two nearest.",
        "let p = percentile([1, 5, 9, 12], 75);",
    ),
    doc(
        Series,
        "tostring",
        "tostring(value) -> text",
        "A number or any other value as text; \"na\" for a number that is not one.",
        "print(`rsi is ${tostring(rsi(close, 14).last())}`);",
    ),
    doc(
        Series,
        "fmt",
        "fmt(number, decimals) -> text",
        "A number as text with a number of decimals (0 to 10); \"na\" for a number that is not one.",
        "let text = fmt(close.last(), 2);",
    ),
    doc(
        Colors,
        "color_r",
        "color_r(color) -> int",
        "The red of a color, 0 to 255.",
        "let red = color_r(\"orange\");",
    ),
    doc(
        Colors,
        "color_g",
        "color_g(color) -> int",
        "The green of a color, 0 to 255.",
        "let green = color_g(\"orange\");",
    ),
    doc(
        Colors,
        "color_b",
        "color_b(color) -> int",
        "The blue of a color, 0 to 255.",
        "let blue = color_b(\"orange\");",
    ),
    doc(
        Colors,
        "color_mix",
        "color_mix(a, b, t) -> color",
        "The color between two colors: a at t = 0, b at t = 1, and the colors in between for the values between.",
        "let c = color_mix(\"red\", \"green\", 0.5);",
    ),
    doc(
        Colors,
        "from_gradient",
        "from_gradient(value, low, high, from_color, to_color) -> color",
        "The color of a value on a gradient: from_color at low, to_color at high, and the colors in between.",
        "let c = from_gradient(rsi(close, 14).last(), 30, 70, \"red\", \"green\");",
    ),
    doc(
        Conditions,
        "crossover",
        "crossover(a, b) -> series",
        "The name of Pine for cross_over: 1 on the bar where a went from under b to over it.",
        "let up = crossover(close, sma(close, 20));",
    ),
    doc(
        Conditions,
        "crossunder",
        "crossunder(a, b) -> series",
        "The name of Pine for cross_under: 1 on the bar where a went from over b to under it.",
        "let down = crossunder(close, sma(close, 20));",
    ),
    doc(
        Windows,
        "mom",
        "mom(series, length) -> series",
        "The name of Pine for momentum: the change over length bars.",
        "let m = mom(close, 10);",
    ),
    doc(
        Indicators,
        "wpr",
        "wpr(length) -> series",
        "The name of Pine for williams_r: Williams %R, from -100 to 0.",
        "let w = wpr(14);",
    ),
    doc(
        Indicators,
        "sar",
        "sar(start, step, max) -> series",
        "The name of Pine for psar: the parabolic SAR.",
        "let s = sar(0.02, 0.02, 0.2);",
    ),
];

/// A name a script starts with.
#[derive(Debug, Clone, Copy)]
pub struct Name {
    pub name: &'static str,
    pub summary: &'static str,
}

pub const GLOBALS: &[Name] = &[
    Name {
        name: "open",
        summary: "The open of every bar (a series).",
    },
    Name {
        name: "high",
        summary: "The high of every bar (a series).",
    },
    Name {
        name: "low",
        summary: "The low of every bar (a series).",
    },
    Name {
        name: "close",
        summary: "The close of every bar (a series).",
    },
    Name {
        name: "volume",
        summary: "The volume of every bar (a series).",
    },
    Name {
        name: "time",
        summary: "The time of every bar, in milliseconds since 1970 (a series).",
    },
    Name {
        name: "hl2",
        summary: "(high + low) / 2 of every bar (a series).",
    },
    Name {
        name: "hlc3",
        summary: "(high + low + close) / 3 of every bar (a series).",
    },
    Name {
        name: "ohlc4",
        summary: "(open + high + low + close) / 4 of every bar (a series).",
    },
    Name {
        name: "hlcc4",
        summary: "(high + low + close + close) / 4 of every bar (a series).",
    },
    Name {
        name: "bar_index",
        summary: "The number of every bar, from 0 (a series).",
    },
    Name {
        name: "n",
        summary: "How many bars the chart has (a whole number).",
    },
    Name {
        name: "na",
        summary: "No value. iff(condition, line, na) draws the line only where the condition holds.",
    },
];

/// The words of the language.
pub(crate) const KEYWORDS: &[&str] = &[
    "let", "const", "if", "else", "switch", "while", "loop", "for", "in", "break", "continue",
    "return", "fn", "true", "false", "print", "debug",
];

/// The description of a function, by name.
pub fn function(name: &str) -> Option<&'static Doc> {
    FUNCTIONS.iter().find(|d| d.name == name)
}

/// The description of a name a script starts with.
pub fn global(name: &str) -> Option<&'static Name> {
    GLOBALS.iter().find(|n| n.name == name)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rhai::Engine;

    use super::*;

    /// The names of the functions the API registers, without the operators.
    fn registered() -> BTreeSet<String> {
        let mut engine = Engine::new_raw();
        crate::indicators::custom::api::register(&mut engine);
        engine
            .gen_fn_signatures(false)
            .into_iter()
            .filter_map(|signature| {
                let name: String = signature
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .trim_start_matches("get ")
                    .trim()
                    .to_owned();
                // The operators and the accessors (`index$get$`) are not functions to describe.
                (name.chars().next().is_some_and(char::is_alphabetic) && !name.contains('$'))
                    .then_some(name)
            })
            .collect()
    }

    #[test]
    fn every_function_the_engine_has_is_described_and_every_description_is_a_function() {
        let registered = registered();
        let described: BTreeSet<String> = FUNCTIONS.iter().map(|d| d.name.to_owned()).collect();
        let undescribed: Vec<_> = registered.difference(&described).collect();
        let missing: Vec<_> = described.difference(&registered).collect();
        assert!(
            undescribed.is_empty(),
            "registered but not described: {undescribed:?}"
        );
        assert!(
            missing.is_empty(),
            "described but not registered: {missing:?}"
        );
    }

    #[test]
    fn a_name_is_described_once_and_every_group_has_functions() {
        let mut seen = BTreeSet::new();
        for d in FUNCTIONS {
            assert!(seen.insert(d.name), "{} is described twice", d.name);
            assert!(!d.summary.is_empty() && !d.example.is_empty());
            assert!(d.signature.starts_with(d.name), "{}", d.signature);
        }
        for group in Group::ALL {
            assert!(FUNCTIONS.iter().any(|d| d.group == group), "{group:?}");
        }
    }

    #[test]
    fn the_names_a_script_starts_with_are_the_ones_the_run_provides() {
        let scope = crate::indicators::custom::run::global_names();
        let described: BTreeSet<String> = GLOBALS.iter().map(|n| n.name.to_owned()).collect();
        assert_eq!(scope.into_iter().collect::<BTreeSet<_>>(), described);
    }
}
