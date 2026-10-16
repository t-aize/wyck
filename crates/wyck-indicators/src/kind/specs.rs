//! The spec of each of the first indicators on offer: the moving averages, the bands, the
//! oscillators and the volume studies. The later ones are in [`crate::extended::specs`].

use super::StudyKind;
use crate::compute::MA_METHODS;
use crate::extended;
use crate::spec::{
    ATR_SMOOTHING, BLUE, DOWN, GRAY, InputKind, InputSpec, OFFSET, ORANGE, PINK, PURPLE, Placement,
    PlotKind, PlotSpec, SOURCE, Spec, TEAL, UP, ValueFormat, choice, float, hidden_line, int, line,
};
use wyck_drawings::model::style::Dash;

/// The spec of a kind that is not known, or that a script stands for: no input, no line.
pub(crate) const UNKNOWN: Spec = Spec {
    label: "Unknown",
    short: "?",
    placement: Placement::Overlay,
    format: ValueFormat::Plain(2),
    inputs: &[],
    plots: &[],
    range: None,
};

pub(super) fn spec_of(kind: StudyKind) -> Spec {
    use Placement::{Overlay, Pane};
    match kind {
        StudyKind::Sma => {
            const {
                Spec {
                    label: "Moving average (simple)",
                    short: "SMA",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[int("length", "Length", 20.0, 1.0, 1000.0), SOURCE, OFFSET],
                    plots: &[line("ma", "Average", BLUE, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::Ema => {
            const {
                Spec {
                    label: "Moving average (exponential)",
                    short: "EMA",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[int("length", "Length", 50.0, 1.0, 1000.0), SOURCE, OFFSET],
                    plots: &[line("ma", "Average", ORANGE, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::Wma => {
            const {
                Spec {
                    label: "Moving average (weighted)",
                    short: "WMA",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[int("length", "Length", 20.0, 1.0, 1000.0), SOURCE, OFFSET],
                    plots: &[line("ma", "Average", PURPLE, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::Hma => {
            const {
                Spec {
                    label: "Hull moving average",
                    short: "HMA",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[int("length", "Length", 21.0, 2.0, 1000.0), SOURCE, OFFSET],
                    plots: &[line("ma", "Average", TEAL, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::MtfMa => {
            const {
                Spec {
                    label: "Moving average on a timeframe",
                    short: "MA",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[
                        InputSpec {
                            kind: InputKind::Timeframe,
                            key: "timeframe",
                            label: "Timeframe",
                            default: 0.0,
                            min: 0.0,
                            max: 0.0,
                            step: 1.0,
                            group: "",
                            tooltip: "The average is computed on bars of this timeframe, built from the chart's bars. Empty uses the chart's own.",
                            text: "4h",
                        },
                        int("length", "Length", 20.0, 1.0, 1000.0),
                        choice("method", "Method", &MA_METHODS, 0.0),
                        SOURCE,
                        InputSpec {
                            kind: InputKind::Toggle,
                            key: "confirmed",
                            label: "Wait for the bar to close",
                            default: 1.0,
                            min: 0.0,
                            max: 1.0,
                            step: 1.0,
                            group: "",
                            tooltip: "On: the line only shows what was known when each bar closed. Off: it follows the higher bar still forming and can change.",
                            text: "",
                        },
                    ],
                    plots: &[line("ma", "Average", ORANGE, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::Vwap => {
            const {
                Spec {
                    label: "VWAP",
                    short: "VWAP",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[
                        InputSpec {
                            default: 5.0,
                            ..SOURCE
                        },
                        choice("anchor", "Anchor", &["Day", "Week", "Month", "Year"], 0.0),
                        float("band_1", "Band 1 deviations", 1.0, 0.1, 10.0, 0.1),
                        float("band_2", "Band 2 deviations", 2.0, 0.1, 10.0, 0.1),
                    ],
                    plots: &[
                        line("vwap", "VWAP", PINK, 1.5),
                        hidden_line("upper_1", "Upper band 1", BLUE),
                        hidden_line("lower_1", "Lower band 1", BLUE),
                        hidden_line("upper_2", "Upper band 2", PURPLE),
                        hidden_line("lower_2", "Lower band 2", PURPLE),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Bollinger => {
            const {
                Spec {
                    label: "Bollinger bands",
                    short: "BB",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[
                        int("length", "Length", 20.0, 1.0, 1000.0),
                        float("mult", "Deviations", 2.0, 0.1, 10.0, 0.1),
                        SOURCE,
                        choice(
                            "method",
                            "Basis average",
                            &["SMA", "EMA", "WMA", "RMA"],
                            0.0,
                        ),
                    ],
                    plots: &[
                        line("basis", "Basis", ORANGE, 1.0),
                        line("upper", "Upper", BLUE, 1.0),
                        line("lower", "Lower", BLUE, 1.0),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Keltner => {
            const {
                Spec {
                    label: "Keltner channels",
                    short: "KC",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[
                        int("length", "Length", 20.0, 1.0, 1000.0),
                        float("mult", "Multiplier", 2.0, 0.1, 10.0, 0.1),
                        int("atr", "ATR length", 10.0, 1.0, 1000.0),
                        SOURCE,
                        choice(
                            "method",
                            "Basis average",
                            &["SMA", "EMA", "WMA", "RMA"],
                            1.0,
                        ),
                    ],
                    plots: &[
                        line("basis", "Basis", BLUE, 1.0),
                        line("upper", "Upper", BLUE, 1.0),
                        line("lower", "Lower", BLUE, 1.0),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Donchian => {
            const {
                Spec {
                    label: "Donchian channels",
                    short: "DC",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[int("length", "Length", 20.0, 1.0, 1000.0)],
                    plots: &[
                        line("basis", "Basis", ORANGE, 1.0),
                        line("upper", "Upper", BLUE, 1.0),
                        line("lower", "Lower", BLUE, 1.0),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Ichimoku => {
            const {
                Spec {
                    label: "Ichimoku cloud",
                    short: "Ichimoku",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[
                        int("conversion", "Conversion line", 9.0, 1.0, 500.0),
                        int("base", "Base line", 26.0, 1.0, 500.0),
                        int("span_b", "Leading span B", 52.0, 1.0, 500.0),
                        int("displacement", "Displacement", 26.0, 1.0, 500.0),
                    ],
                    plots: &[
                        line("conversion", "Conversion line", BLUE, 1.0),
                        line("base", "Base line", 0xb7_1c1c, 1.0),
                        line("lagging", "Lagging span", 0x43_a047, 1.0),
                        line("span_a", "Leading span A", 0xa5_d6a7, 1.0),
                        line("span_b", "Leading span B", 0xef_9a9a, 1.0),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::ParabolicSar => {
            const {
                Spec {
                    label: "Parabolic SAR",
                    short: "SAR",
                    placement: Overlay,
                    format: ValueFormat::Price,
                    inputs: &[
                        float("start", "Start", 0.02, 0.001, 1.0, 0.001),
                        float("step", "Increment", 0.02, 0.001, 1.0, 0.001),
                        float("max", "Maximum", 0.2, 0.01, 1.0, 0.01),
                    ],
                    plots: &[PlotSpec {
                        key: "sar",
                        label: "SAR",
                        kind: PlotKind::Dots,
                        color: BLUE,
                        width: 2.0,
                        dash: Dash::Solid,
                        visible: true,
                    }],
                    range: None,
                }
            }
        }
        StudyKind::VolumeProfile => {
            const {
                Spec {
                    label: "Volume profile (visible range)",
                    short: "VPVR",
                    placement: Overlay,
                    format: ValueFormat::Count,
                    inputs: &[
                        int("rows", "Rows", 24.0, 4.0, 200.0),
                        int("value_area", "Value area (%)", 70.0, 1.0, 100.0),
                        int("width", "Width (% of the chart)", 30.0, 5.0, 100.0),
                        InputSpec {
                            key: "side",
                            label: "Placement",
                            kind: InputKind::Choice(&["Right", "Left"]),
                            default: 0.0,
                            min: 0.0,
                            max: 1.0,
                            step: 1.0,
                            group: "",
                            tooltip: "",
                            text: "",
                        },
                        InputSpec {
                            key: "highlight",
                            label: "Highlight the value area",
                            kind: InputKind::Toggle,
                            default: 1.0,
                            min: 0.0,
                            max: 1.0,
                            step: 1.0,
                            group: "",
                            tooltip: "",
                            text: "",
                        },
                        float(
                            "area_opacity",
                            "Value area opacity (%)",
                            45.0,
                            0.0,
                            100.0,
                            1.0,
                        ),
                        float(
                            "other_opacity",
                            "Other rows opacity (%)",
                            20.0,
                            0.0,
                            100.0,
                            1.0,
                        ),
                        float(
                            "normal_opacity",
                            "Opacity without highlight (%)",
                            35.0,
                            0.0,
                            100.0,
                            1.0,
                        ),
                    ],
                    plots: &[
                        line("up", "Up volume", UP, 1.0),
                        line("down", "Down volume", DOWN, 1.0),
                        line("poc", "Point of control", 0xff_eb3b, 1.0),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Volume => {
            const {
                Spec {
                    label: "Volume (ticks)",
                    short: "Vol",
                    placement: Pane,
                    format: ValueFormat::Count,
                    inputs: &[int("ma", "Average length", 20.0, 1.0, 1000.0)],
                    plots: &[
                        PlotSpec {
                            key: "volume",
                            label: "Volume",
                            kind: PlotKind::Histogram,
                            color: GRAY,
                            width: 1.0,
                            dash: Dash::Solid,
                            visible: true,
                        },
                        line("ma", "Average", BLUE, 1.0),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Rsi => {
            const {
                Spec {
                    label: "Relative strength index",
                    short: "RSI",
                    placement: Pane,
                    format: ValueFormat::Plain(2),
                    inputs: &[
                        int("length", "Length", 14.0, 1.0, 1000.0),
                        SOURCE,
                        float("upper", "Overbought", 70.0, 0.0, 100.0, 1.0),
                        float("lower", "Oversold", 30.0, 0.0, 100.0, 1.0),
                    ],
                    plots: &[line("rsi", "RSI", PURPLE, 1.5)],
                    range: Some((0.0, 100.0)),
                }
            }
        }
        StudyKind::Macd => {
            const {
                Spec {
                    label: "MACD",
                    short: "MACD",
                    placement: Pane,
                    format: ValueFormat::Price,
                    inputs: &[
                        int("fast", "Fast length", 12.0, 1.0, 500.0),
                        int("slow", "Slow length", 26.0, 1.0, 500.0),
                        int("signal", "Signal length", 9.0, 1.0, 500.0),
                        SOURCE,
                    ],
                    plots: &[
                        PlotSpec {
                            key: "histogram",
                            label: "Histogram",
                            kind: PlotKind::Histogram,
                            color: GRAY,
                            width: 1.0,
                            dash: Dash::Solid,
                            visible: true,
                        },
                        line("macd", "MACD", BLUE, 1.5),
                        line("signal", "Signal", ORANGE, 1.5),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Stochastic => {
            const {
                Spec {
                    label: "Stochastic",
                    short: "Stoch",
                    placement: Pane,
                    format: ValueFormat::Plain(2),
                    inputs: &[
                        int("length", "%K length", 14.0, 1.0, 500.0),
                        int("smooth", "%K smoothing", 3.0, 1.0, 100.0),
                        int("d", "%D smoothing", 3.0, 1.0, 100.0),
                        float("upper", "Upper band", 80.0, 0.0, 100.0, 1.0),
                        float("lower", "Lower band", 20.0, 0.0, 100.0, 1.0),
                    ],
                    plots: &[line("k", "%K", BLUE, 1.5), line("d", "%D", ORANGE, 1.5)],
                    range: Some((0.0, 100.0)),
                }
            }
        }
        StudyKind::Atr => {
            const {
                Spec {
                    label: "Average true range",
                    short: "ATR",
                    placement: Pane,
                    format: ValueFormat::Price,
                    inputs: &[
                        int("length", "ATR length", 14.0, 1.0, 1000.0),
                        InputSpec {
                            key: "smoothing",
                            label: "ATR smoothing",
                            kind: InputKind::Choice(ATR_SMOOTHING),
                            default: 0.0,
                            min: 0.0,
                            max: 3.0,
                            step: 1.0,
                            group: "",
                            tooltip: "",
                            text: "",
                        },
                        InputSpec {
                            key: "unit",
                            label: "Display unit",
                            kind: InputKind::Choice(&["Price", "% of close"]),
                            default: 0.0,
                            min: 0.0,
                            max: 1.0,
                            step: 1.0,
                            group: "",
                            tooltip: "",
                            text: "",
                        },
                        int("percent_decimals", "Percent decimals", 2.0, 0.0, 6.0),
                        int("signal_length", "Signal length", 14.0, 1.0, 1000.0),
                        InputSpec {
                            key: "signal_smoothing",
                            label: "Signal smoothing",
                            kind: InputKind::Choice(ATR_SMOOTHING),
                            default: 1.0,
                            min: 0.0,
                            max: 3.0,
                            step: 1.0,
                            group: "",
                            tooltip: "",
                            text: "",
                        },
                    ],
                    plots: &[
                        line("atr", "ATR", 0xb7_1c1c, 1.5),
                        hidden_line("signal", "Signal average", ORANGE),
                        hidden_line("tr", "True range", GRAY),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Cci => {
            const {
                Spec {
                    label: "Commodity channel index",
                    short: "CCI",
                    placement: Pane,
                    format: ValueFormat::Plain(2),
                    inputs: &[
                        int("length", "Length", 20.0, 1.0, 1000.0),
                        float("upper", "Upper band", 100.0, -1000.0, 1000.0, 1.0),
                        float("lower", "Lower band", -100.0, -1000.0, 1000.0, 1.0),
                    ],
                    plots: &[line("cci", "CCI", TEAL, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::WilliamsR => {
            const {
                Spec {
                    label: "Williams %R",
                    short: "%R",
                    placement: Pane,
                    format: ValueFormat::Plain(2),
                    inputs: &[
                        int("length", "Length", 14.0, 1.0, 1000.0),
                        float("upper", "Upper band", -20.0, -100.0, 0.0, 1.0),
                        float("lower", "Lower band", -80.0, -100.0, 0.0, 1.0),
                    ],
                    plots: &[line("r", "%R", PURPLE, 1.5)],
                    range: Some((-100.0, 0.0)),
                }
            }
        }
        StudyKind::Momentum => {
            const {
                Spec {
                    label: "Momentum",
                    short: "Mom",
                    placement: Pane,
                    format: ValueFormat::Price,
                    inputs: &[int("length", "Length", 10.0, 1.0, 1000.0), SOURCE],
                    plots: &[line("mom", "Momentum", BLUE, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::Obv => {
            const {
                Spec {
                    label: "On balance volume (ticks)",
                    short: "OBV",
                    placement: Pane,
                    format: ValueFormat::Count,
                    inputs: &[],
                    plots: &[line("obv", "OBV", BLUE, 1.5)],
                    range: None,
                }
            }
        }
        StudyKind::Dmi => {
            const {
                Spec {
                    label: "Directional movement (ADX)",
                    short: "DMI",
                    placement: Pane,
                    format: ValueFormat::Plain(2),
                    inputs: &[
                        int("length", "DI length", 14.0, 1.0, 500.0),
                        int("smoothing", "ADX smoothing", 14.0, 1.0, 500.0),
                    ],
                    plots: &[
                        line("adx", "ADX", 0xff_5252, 1.5),
                        line("plus", "+DI", BLUE, 1.0),
                        line("minus", "-DI", ORANGE, 1.0),
                    ],
                    range: None,
                }
            }
        }
        StudyKind::Unknown | StudyKind::Custom => UNKNOWN,
        other => extended::specs::spec_for(other),
    }
}
