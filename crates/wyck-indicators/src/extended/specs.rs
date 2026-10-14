//! The fixed description of each additional indicator: its inputs, plots and where it is drawn.

use crate::kind::StudyKind;
use crate::spec::{
    InputKind, InputSpec, Placement, PlotKind, PlotSpec, SOURCE, Spec, ValueFormat, float, int,
    line,
};

pub(super) const BLUE: u32 = 0x2962ff;
pub(super) const ORANGE: u32 = 0xff9800;
pub(super) const GREEN: u32 = 0x26a69a;
pub(super) const RED: u32 = 0xef5350;
pub(super) const PURPLE: u32 = 0x7e57c2;

pub(super) const fn styled_plot(
    key: &'static str,
    label: &'static str,
    color: u32,
    width: f32,
    kind: PlotKind,
) -> PlotSpec {
    PlotSpec {
        key,
        label,
        kind,
        color,
        width,
        dash: wyck_drawings::model::style::Dash::Solid,
        visible: true,
    }
}

macro_rules! static_spec {
    ($label:expr, $short:expr, $placement:expr, $format:expr,
     &[$($input:expr),* $(,)?], &[$($plot:expr),* $(,)?], $range:expr) => {
        const {
            const INPUTS: &[InputSpec] = &[$($input),*];
            const PLOTS: &[PlotSpec] = &[$($plot),*];
            spec($label, $short, $placement, $format, INPUTS, PLOTS, $range)
        }
    };
}

pub(super) const fn choice(
    key: &'static str,
    label: &'static str,
    names: &'static [&'static str],
    default: f64,
) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: InputKind::Choice(names),
        default,
        min: 0.0,
        max: (names.len() - 1) as f64,
        step: 1.0,
        group: "",
        tooltip: "",
        text: "",
    }
}

pub(super) const fn spec(
    label: &'static str,
    short: &'static str,
    placement: Placement,
    format: ValueFormat,
    inputs: &'static [InputSpec],
    plots: &'static [PlotSpec],
    range: Option<(f64, f64)>,
) -> Spec {
    Spec {
        label,
        short,
        placement,
        format,
        inputs,
        plots,
        range,
    }
}

pub(crate) const fn spec_for(kind: StudyKind) -> Spec {
    use Placement::{Overlay, Pane};
    use ValueFormat::{Count, Plain, Price};
    match kind {
        StudyKind::Rma => static_spec!(
            "Wilder moving average",
            "RMA",
            Overlay,
            Price,
            &[int("length", "Length", 14.0, 1.0, 1000.0), SOURCE],
            &[line("rma", "Average", BLUE, 1.5)],
            None
        ),
        StudyKind::Supertrend => static_spec!(
            "Supertrend",
            "Supertrend",
            Overlay,
            Price,
            &[
                int("length", "ATR length", 10.0, 1.0, 500.0),
                float("mult", "Multiplier", 3.0, 0.1, 20.0, 0.1)
            ],
            &[
                line("up", "Up trend", GREEN, 2.0),
                line("down", "Down trend", RED, 2.0)
            ],
            None
        ),
        StudyKind::Aroon => static_spec!(
            "Aroon",
            "Aroon",
            Pane,
            Plain(2),
            &[int("length", "Length", 25.0, 2.0, 500.0)],
            &[
                line("up", "Aroon Up", GREEN, 1.5),
                line("down", "Aroon Down", RED, 1.5)
            ],
            Some((0.0, 100.0))
        ),
        StudyKind::Alligator => static_spec!(
            "Alligator",
            "Alligator",
            Overlay,
            Price,
            &[
                int("jaw", "Jaw length", 13.0, 1.0, 500.0),
                int("jaw_shift", "Jaw shift", 8.0, 0.0, 100.0),
                int("teeth", "Teeth length", 8.0, 1.0, 500.0),
                int("teeth_shift", "Teeth shift", 5.0, 0.0, 100.0),
                int("lips", "Lips length", 5.0, 1.0, 500.0),
                int("lips_shift", "Lips shift", 3.0, 0.0, 100.0)
            ],
            &[
                line("jaw", "Jaw", BLUE, 1.5),
                line("teeth", "Teeth", RED, 1.5),
                line("lips", "Lips", GREEN, 1.5)
            ],
            None
        ),
        StudyKind::Envelopes => static_spec!(
            "Moving average envelopes",
            "Env",
            Overlay,
            Price,
            &[
                int("length", "Length", 20.0, 1.0, 1000.0),
                SOURCE,
                choice("method", "Average", &["SMA", "EMA", "WMA", "RMA"], 0.0),
                float("percent", "Distance (%)", 2.0, 0.01, 50.0, 0.1)
            ],
            &[
                line("basis", "Basis", ORANGE, 1.0),
                line("upper", "Upper", BLUE, 1.0),
                line("lower", "Lower", BLUE, 1.0)
            ],
            None
        ),
        StudyKind::LinearRegression => static_spec!(
            "Linear regression",
            "LinReg",
            Overlay,
            Price,
            &[
                int("length", "Length", 100.0, 2.0, 1000.0),
                SOURCE,
                float("deviations", "Band deviations", 2.0, 0.0, 10.0, 0.1)
            ],
            &[
                line("basis", "Regression", BLUE, 1.5),
                line("upper", "Upper", PURPLE, 1.0),
                line("lower", "Lower", PURPLE, 1.0)
            ],
            None
        ),
        StudyKind::PivotPoints => static_spec!(
            "Pivot points (standard)",
            "Pivots",
            Overlay,
            Price,
            &[choice("period", "Anchor", &["Day", "Week", "Month"], 0.0)],
            &[
                line("p", "Pivot", ORANGE, 1.5),
                line("r1", "R1", RED, 1.0),
                line("s1", "S1", GREEN, 1.0),
                line("r2", "R2", RED, 1.0),
                line("s2", "S2", GREEN, 1.0),
                line("r3", "R3", RED, 1.0),
                line("s3", "S3", GREEN, 1.0)
            ],
            None
        ),
        StudyKind::Fractals => static_spec!(
            "Williams fractals",
            "Fractals",
            Overlay,
            Price,
            &[int("wing", "Bars on each side", 2.0, 2.0, 20.0)],
            &[
                styled_plot("high", "High fractal", RED, 3.0, PlotKind::Dots),
                styled_plot("low", "Low fractal", GREEN, 3.0, PlotKind::Dots)
            ],
            None
        ),
        StudyKind::StochasticRsi => static_spec!(
            "Stochastic RSI",
            "Stoch RSI",
            Pane,
            Plain(2),
            &[
                int("rsi", "RSI length", 14.0, 1.0, 500.0),
                int("stoch", "Stochastic length", 14.0, 1.0, 500.0),
                int("k", "%K smoothing", 3.0, 1.0, 100.0),
                int("d", "%D smoothing", 3.0, 1.0, 100.0),
                SOURCE,
                float("upper", "Upper band", 80.0, 0.0, 100.0, 1.0),
                float("lower", "Lower band", 20.0, 0.0, 100.0, 1.0)
            ],
            &[line("k", "%K", BLUE, 1.5), line("d", "%D", ORANGE, 1.5)],
            Some((0.0, 100.0))
        ),
        StudyKind::Roc => static_spec!(
            "Rate of change",
            "ROC",
            Pane,
            Plain(2),
            &[int("length", "Length", 12.0, 1.0, 1000.0), SOURCE],
            &[line("roc", "ROC", BLUE, 1.5)],
            None
        ),
        StudyKind::AwesomeOscillator => static_spec!(
            "Awesome oscillator",
            "AO",
            Pane,
            Price,
            &[
                int("fast", "Fast length", 5.0, 1.0, 500.0),
                int("slow", "Slow length", 34.0, 2.0, 500.0)
            ],
            &[styled_plot("ao", "AO", GREEN, 1.0, PlotKind::Histogram)],
            None
        ),
        StudyKind::AcceleratorOscillator => static_spec!(
            "Accelerator oscillator",
            "AC",
            Pane,
            Price,
            &[
                int("fast", "Fast length", 5.0, 1.0, 500.0),
                int("slow", "Slow length", 34.0, 2.0, 500.0),
                int("signal", "Signal length", 5.0, 1.0, 500.0)
            ],
            &[styled_plot("ac", "AC", GREEN, 1.0, PlotKind::Histogram)],
            None
        ),
        StudyKind::Dpo => static_spec!(
            "Detrended price oscillator",
            "DPO",
            Pane,
            Price,
            &[int("length", "Length", 20.0, 2.0, 500.0), SOURCE],
            &[line("dpo", "DPO", BLUE, 1.5)],
            None
        ),
        StudyKind::Trix => static_spec!(
            "TRIX",
            "TRIX",
            Pane,
            Plain(2),
            &[
                int("length", "Length", 15.0, 1.0, 500.0),
                int("signal", "Signal length", 9.0, 1.0, 500.0),
                SOURCE
            ],
            &[
                line("trix", "TRIX", BLUE, 1.5),
                line("signal", "Signal", ORANGE, 1.0)
            ],
            None
        ),
        StudyKind::UltimateOscillator => static_spec!(
            "Ultimate oscillator",
            "UO",
            Pane,
            Plain(2),
            &[
                int("short", "Short length", 7.0, 1.0, 100.0),
                int("middle", "Middle length", 14.0, 2.0, 200.0),
                int("long", "Long length", 28.0, 3.0, 500.0)
            ],
            &[line("uo", "Ultimate", PURPLE, 1.5)],
            Some((0.0, 100.0))
        ),
        StudyKind::RelativeVigorIndex => static_spec!(
            "Relative vigor index",
            "RVI",
            Pane,
            Plain(3),
            &[
                int("length", "Length", 10.0, 2.0, 500.0),
                int("signal", "Signal smoothing", 1.0, 1.0, 100.0)
            ],
            &[
                line("rvi", "RVI", BLUE, 1.5),
                line("signal", "Signal", ORANGE, 1.0)
            ],
            None
        ),
        StudyKind::DeMarker => static_spec!(
            "DeMarker",
            "DeM",
            Pane,
            Plain(3),
            &[int("length", "Length", 14.0, 2.0, 500.0)],
            &[line("dem", "DeMarker", PURPLE, 1.5)],
            Some((0.0, 1.0))
        ),
        StudyKind::StandardDeviation => static_spec!(
            "Standard deviation",
            "StDev",
            Pane,
            Price,
            &[int("length", "Length", 20.0, 2.0, 1000.0), SOURCE],
            &[line("dev", "Deviation", BLUE, 1.5)],
            None
        ),
        StudyKind::TrueRange => static_spec!(
            "True range",
            "TR",
            Pane,
            Price,
            &[],
            &[line("tr", "True range", RED, 1.5)],
            None
        ),
        StudyKind::HistoricalVolatility => static_spec!(
            "Historical volatility",
            "HV",
            Pane,
            Plain(2),
            &[
                int("length", "Length", 20.0, 2.0, 1000.0),
                int("annual", "Periods per year", 252.0, 1.0, 100000.0)
            ],
            &[line("hv", "Annualized volatility", PURPLE, 1.5)],
            None
        ),
        StudyKind::ChaikinVolatility => static_spec!(
            "Chaikin volatility",
            "Chaikin Vol",
            Pane,
            Plain(2),
            &[
                int("ema", "EMA length", 10.0, 1.0, 500.0),
                int("roc", "ROC length", 10.0, 1.0, 500.0)
            ],
            &[line("cv", "Volatility", BLUE, 1.5)],
            None
        ),
        StudyKind::ChoppinessIndex => static_spec!(
            "Choppiness index",
            "CHOP",
            Pane,
            Plain(2),
            &[int("length", "Length", 14.0, 2.0, 500.0)],
            &[line("chop", "Choppiness", PURPLE, 1.5)],
            Some((0.0, 100.0))
        ),
        StudyKind::MoneyFlowIndex => static_spec!(
            "Money flow index (ticks)",
            "MFI",
            Pane,
            Plain(2),
            &[int("length", "Length", 14.0, 2.0, 500.0)],
            &[line("mfi", "MFI", PURPLE, 1.5)],
            Some((0.0, 100.0))
        ),
        StudyKind::ChaikinMoneyFlow => static_spec!(
            "Chaikin money flow (ticks)",
            "CMF",
            Pane,
            Plain(3),
            &[int("length", "Length", 20.0, 2.0, 500.0)],
            &[line("cmf", "CMF", GREEN, 1.5)],
            Some((-1.0, 1.0))
        ),
        StudyKind::AccumulationDistribution => static_spec!(
            "Accumulation/distribution (ticks)",
            "A/D",
            Pane,
            Count,
            &[],
            &[line("ad", "A/D", BLUE, 1.5)],
            None
        ),
        StudyKind::PriceVolumeTrend => static_spec!(
            "Price volume trend (ticks)",
            "PVT",
            Pane,
            Count,
            &[],
            &[line("pvt", "PVT", BLUE, 1.5)],
            None
        ),
        StudyKind::ForceIndex => static_spec!(
            "Force index (ticks)",
            "Force",
            Pane,
            Count,
            &[int("length", "EMA length", 13.0, 1.0, 500.0)],
            &[line("force", "Force", GREEN, 1.5)],
            None
        ),
        StudyKind::VolumeOscillator => static_spec!(
            "Volume oscillator (ticks)",
            "Vol Osc",
            Pane,
            Plain(2),
            &[
                int("fast", "Fast length", 5.0, 1.0, 500.0),
                int("slow", "Slow length", 20.0, 2.0, 500.0)
            ],
            &[line("osc", "Volume oscillator", BLUE, 1.5)],
            None
        ),
        StudyKind::EaseOfMovement => static_spec!(
            "Ease of movement (ticks)",
            "EMV",
            Pane,
            Plain(3),
            &[
                int("length", "Average length", 14.0, 1.0, 500.0),
                float("scale", "Volume divisor", 10000.0, 1.0, 1e9, 1000.0)
            ],
            &[line("emv", "Ease of movement", PURPLE, 1.5)],
            None
        ),
        _ => panic!("not an extended indicator"),
    }
}
