// Flows lab: the indicators that read the volume, and a few measures of width and trend.
//
// Each reads the bars of the chart. Given a map of bars (from bars_of) first, the same call reads
// those bars instead: accdist(bars_of("EURUSD")).

indicator(#{
    name: "Flows lab",
    short: "Flow",
    overlay: false,
    format: "plain",
    decimals: 3,
    category: "Examples",
    description: "Money flow, accumulation/distribution, volume indices, the Williams indices, band widths and Aroon.",
});

let length = input_int("length", 20, #{ min: 2, max: 200 });
let kind = input_choice("indicator", "accumulation/distribution", [
    "money flow",
    "accumulation/distribution",
    "price volume trend",
    "negative volume index",
    "positive volume index",
    "Williams A/D",
    "Williams variable A/D",
    "intraday intensity",
    "Bollinger width",
    "Keltner width",
    "Aroon up",
    "Aroon down",
]);

let line = switch kind {
    "money flow" => money_flow(),
    "accumulation/distribution" => accdist(),
    "price volume trend" => pvt(),
    "negative volume index" => nvi(),
    "positive volume index" => pvi(),
    "Williams A/D" => wad(),
    "Williams variable A/D" => wvad(),
    "intraday intensity" => iii(),
    "Bollinger width" => bbw(close, length, 2),
    "Keltner width" => kcw(close, length, 1.5),
    "Aroon up" => aroon(length).up,
    _ => aroon(length).down,
};
plot("line", line);
