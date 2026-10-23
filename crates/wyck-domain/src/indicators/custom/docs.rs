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
    /// The chart drawing tools.
    Drawings,
}

impl Group {
    pub const ALL: [Self; 7] = [
        Self::Declare,
        Self::Series,
        Self::Windows,
        Self::Indicators,
        Self::Conditions,
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

use Group::{Colors, Conditions, Declare, Drawings, Indicators, Series, Windows};

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
        "round(series) -> series",
        "Each value rounded to the nearest whole number.",
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
        "min(a, b) -> series",
        "The smaller of two, bar by bar. Either can be a number.",
        "min(close, open)",
    ),
    doc(
        Series,
        "max",
        "max(a, b) -> series",
        "The larger of two, bar by bar. Either can be a number.",
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
        "stdev(series, length) -> series",
        "Standard deviation over a window.",
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
        "sum(series, length) -> series",
        "The sum of the last bars.",
        "sum(volume, 10)",
    ),
    doc(
        Windows,
        "linreg",
        "linreg(series, length) -> series",
        "The value of the least squares line through the last bars, at the newest.",
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
        "True range of each bar.",
        "tr()",
    ),
    doc(
        Indicators,
        "atr",
        "atr(length) -> series",
        "Average true range.",
        "atr(14)",
    ),
    doc(
        Indicators,
        "cci",
        "cci(length) -> series",
        "Commodity channel index.",
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
        "mfi(length) -> series",
        "Money flow index, from 0 to 100.",
        "mfi(14)",
    ),
    doc(
        Indicators,
        "vwap",
        "vwap(series?) -> series",
        "Volume weighted average price, starting over every day. Without a series it reads the typical price.",
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
        "Slow stochastic oscillator. Gives a map with k and d.",
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
        "median(series, length) -> series",
        "The median over the last bars: the middle value, or the mean of the two in the middle.",
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
