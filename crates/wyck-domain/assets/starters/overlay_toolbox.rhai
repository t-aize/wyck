// THE READY MADE INDICATORS DRAWN ON THE PRICES
//
// Bollinger bands, Keltner channel, Donchian channel, Supertrend, parabolic SAR and VWAP, each
// behind a switch. A function that gives several lines gives them as a map: bollinger(...) has
// .basis, .upper and .lower, and so on (the reference lists them).
//
// A plot has to be there on every run, so a line that is switched off is a plot of `none`
// (a series with no value), not a plot that is left out.

indicator(#{
    name: "Overlay toolbox",
    short: "OVL",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "Bollinger, Keltner, Donchian, Supertrend, SAR and VWAP on the prices.",
});

let show_bb = input_bool("show_bb", true, #{ label: "Bollinger bands", group: "Bollinger" });
let bb_length = input_int("bb_length", 20, #{ label: "Length", min: 2, max: 300, group: "Bollinger" });
let bb_mult = input_float("bb_mult", 2.0, #{ label: "Deviations", min: 0.1, max: 10.0, step: 0.1, group: "Bollinger" });

let show_kc = input_bool("show_kc", false, #{ label: "Keltner channel", group: "Keltner" });
let kc_length = input_int("kc_length", 20, #{ label: "Length", min: 2, max: 300, group: "Keltner" });
let kc_mult = input_float("kc_mult", 2.0, #{ label: "ATR multiple", min: 0.1, max: 10.0, step: 0.1, group: "Keltner" });
let kc_atr = input_int("kc_atr", 10, #{ label: "ATR length", min: 1, max: 300, group: "Keltner" });

let show_dc = input_bool("show_dc", false, #{ label: "Donchian channel", group: "Donchian" });
let dc_length = input_int("dc_length", 20, #{ label: "Length", min: 2, max: 300, group: "Donchian" });

let show_st = input_bool("show_st", false, #{ label: "Supertrend", group: "Supertrend" });
let st_length = input_int("st_length", 10, #{ label: "ATR length", min: 1, max: 300, group: "Supertrend" });
let st_mult = input_float("st_mult", 3.0, #{ label: "ATR multiple", min: 0.5, max: 10.0, step: 0.5, group: "Supertrend" });

let show_sar = input_bool("show_sar", false, #{ label: "Parabolic SAR", group: "Parabolic SAR" });
let sar_start = input_float("sar_start", 0.02, #{ label: "Start", min: 0.001, max: 0.5, step: 0.005, group: "Parabolic SAR" });
let sar_step = input_float("sar_step", 0.02, #{ label: "Step", min: 0.001, max: 0.5, step: 0.005, group: "Parabolic SAR" });
let sar_max = input_float("sar_max", 0.2, #{ label: "Maximum", min: 0.01, max: 1.0, step: 0.01, group: "Parabolic SAR" });

let show_vwap = input_bool("show_vwap", false, #{ label: "VWAP (starts over every day)", group: "VWAP" });

let none = series(n, na);

let bb = bollinger(close, bb_length, bb_mult);
let kc = keltner(kc_length, kc_mult, kc_atr);
let dc = donchian(dc_length);
let st = supertrend(st_length, st_mult);
let sar = psar(sar_start, sar_step, sar_max);
let vw = vwap();                 // vwap(series) would use that series instead of the typical price

plot("bb_upper", if show_bb { bb.upper } else { none }, #{ title: "Bollinger upper", color: "#42a5f5", width: 1 });
plot("bb_basis", if show_bb { bb.basis } else { none }, #{ title: "Bollinger basis", color: "orange", width: 1, dash: "dashed" });
plot("bb_lower", if show_bb { bb.lower } else { none }, #{ title: "Bollinger lower", color: "#42a5f5", width: 1 });
fill("bb_upper", "bb_lower", #{ alpha: 0.06 });

plot("kc_upper", if show_kc { kc.upper } else { none }, #{ title: "Keltner upper", color: "#ab47bc", width: 1 });
plot("kc_basis", if show_kc { kc.basis } else { none }, #{ title: "Keltner basis", color: "#ab47bc", width: 1, dash: "dotted" });
plot("kc_lower", if show_kc { kc.lower } else { none }, #{ title: "Keltner lower", color: "#ab47bc", width: 1 });

plot("dc_upper", if show_dc { dc.upper } else { none }, #{ title: "Donchian upper", color: "#26a69a", width: 1 });
plot("dc_basis", if show_dc { dc.basis } else { none }, #{ title: "Donchian basis", color: "#26a69a", width: 1, dash: "dotted" });
plot("dc_lower", if show_dc { dc.lower } else { none }, #{ title: "Donchian lower", color: "#26a69a", width: 1 });

// Supertrend gives one line and a direction (1 up, -1 down): two plots, so each has its color.
plot("st_up", if show_st { iff(st.direction > 0, st.line, na) } else { none }, #{ title: "Supertrend up", color: "#26a69a", width: 2 });
plot("st_down", if show_st { iff(st.direction < 0, st.line, na) } else { none }, #{ title: "Supertrend down", color: "#ef5350", width: 2 });

plot("sar", if show_sar { sar } else { none }, #{ title: "Parabolic SAR", color: "#ffa726", style: "dots", width: 2 });
plot("vwap", if show_vwap { vw } else { none }, #{ title: "VWAP", color: "#ffee58", width: 2 });
