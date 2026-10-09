//! Additional built-in indicators. Each output stays aligned with the input bars.

use chrono::{Datelike, NaiveDate};

use super::{
    FillOut, InputKind, InputSpec, Placement, PlotKind, PlotOut, PlotSpec, SOURCE, Spec,
    StudyConfig, StudyInput, StudyKind, StudyOutput, ValueFormat, float, int, line, math, plot,
};

const BLUE: u32 = 0x2962ff;
const ORANGE: u32 = 0xff9800;
const GREEN: u32 = 0x26a69a;
const RED: u32 = 0xef5350;
const PURPLE: u32 = 0x7e57c2;

const fn styled_plot(
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
        dash: crate::domain::drawings::model::Dash::Solid,
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

const fn choice(
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

const fn spec(
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

pub(super) const fn spec_for(kind: StudyKind) -> Spec {
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

pub(super) fn average(values: &[f64], len: usize, method: usize) -> Vec<f64> {
    match method {
        1 => math::ema(values, len),
        2 => math::wma(values, len),
        3 => math::rma(values, len),
        _ => math::sma(values, len),
    }
}

fn values(key: &'static str, v: Vec<f64>) -> StudyOutput {
    StudyOutput {
        plots: vec![plot(key, v)],
        ..StudyOutput::default()
    }
}

fn ratio(top: f64, bottom: f64) -> f64 {
    if bottom.is_finite() && bottom != 0.0 && top.is_finite() {
        top / bottom
    } else {
        f64::NAN
    }
}

pub(super) fn pivot_period(day: i64, kind: usize) -> i64 {
    match kind {
        1 => (day + 3).div_euclid(7),
        2 => NaiveDate::from_num_days_from_ce_opt((day + 719163) as i32)
            .map_or(day, |d| i64::from(d.year()) * 12 + i64::from(d.month())),
        3 => NaiveDate::from_num_days_from_ce_opt((day + 719163) as i32)
            .map_or(day, |d| i64::from(d.year())),
        _ => day,
    }
}

pub(super) fn compute(kind: StudyKind, config: &StudyConfig, input: &StudyInput) -> StudyOutput {
    let n = input.len();
    let source = || input.source(config.input("source"));
    let typical = || input.source(5.0);
    let mut output = match kind {
        StudyKind::Rma => values("rma", math::rma(&source(), config.length("length"))),
        StudyKind::Supertrend => {
            let (line, dir) = math::supertrend(
                &input.high,
                &input.low,
                &input.close,
                config.length("length"),
                config.input("mult"),
            );
            let up = line
                .iter()
                .zip(&dir)
                .map(|(v, d)| if *d > 0.0 { *v } else { f64::NAN })
                .collect();
            let down = line
                .iter()
                .zip(&dir)
                .map(|(v, d)| if *d < 0.0 { *v } else { f64::NAN })
                .collect();
            StudyOutput {
                plots: vec![plot("up", up), plot("down", down)],
                ..StudyOutput::default()
            }
        }
        StudyKind::Aroon => {
            let len = config.length("length");
            let (mut up, mut down) = (vec![f64::NAN; n], vec![f64::NAN; n]);
            for i in len..n {
                let start = i - len;
                let h = (start..=i)
                    .max_by(|&a, &b| input.high[a].total_cmp(&input.high[b]))
                    .unwrap_or(i);
                let l = (start..=i)
                    .min_by(|&a, &b| input.low[a].total_cmp(&input.low[b]))
                    .unwrap_or(i);
                up[i] = 100.0 * (len - (i - h)) as f64 / len as f64;
                down[i] = 100.0 * (len - (i - l)) as f64 / len as f64;
            }
            StudyOutput {
                plots: vec![plot("up", up), plot("down", down)],
                levels: vec![70.0, 30.0],
                ..StudyOutput::default()
            }
        }
        StudyKind::Alligator => {
            let median = input.source(4.0);
            let lines = [
                ("jaw", "jaw_shift"),
                ("teeth", "teeth_shift"),
                ("lips", "lips_shift"),
            ]
            .into_iter()
            .map(|(key, shift)| PlotOut {
                offset: config.input(shift) as i64,
                ..plot(key, math::rma(&median, config.length(key)))
            })
            .collect();
            StudyOutput {
                plots: lines,
                ..StudyOutput::default()
            }
        }
        StudyKind::Envelopes => {
            let basis = average(
                &source(),
                config.length("length"),
                config.input("method") as usize,
            );
            let k = config.input("percent") / 100.0;
            let upper = basis.iter().map(|v| v * (1.0 + k)).collect();
            let lower = basis.iter().map(|v| v * (1.0 - k)).collect();
            StudyOutput {
                plots: vec![
                    plot("basis", basis),
                    plot("upper", upper),
                    plot("lower", lower),
                ],
                ..StudyOutput::default()
            }
        }
        StudyKind::LinearRegression => {
            let src = source();
            let len = config.length("length");
            let (basis, slope) = math::linear_regression(&src, len);
            let dev: Vec<f64> = (0..n)
                .map(|i| {
                    if i + 1 < len || !basis[i].is_finite() {
                        return f64::NAN;
                    }
                    let start = basis[i] - slope[i] * (len - 1) as f64;
                    ((0..len)
                        .map(|j| (src[i + 1 - len + j] - start - slope[i] * j as f64).powi(2))
                        .sum::<f64>()
                        / len as f64)
                        .sqrt()
                })
                .collect();
            let k = config.input("deviations");
            let upper = basis.iter().zip(&dev).map(|(v, d)| v + k * d).collect();
            let lower = basis.iter().zip(&dev).map(|(v, d)| v - k * d).collect();
            StudyOutput {
                plots: vec![
                    plot("basis", basis),
                    plot("upper", upper),
                    plot("lower", lower),
                ],
                ..StudyOutput::default()
            }
        }
        StudyKind::PivotPoints => {
            let mut columns = vec![vec![f64::NAN; n]; 7];
            let mut current = None;
            let (mut hi, mut lo, mut close) = (f64::NEG_INFINITY, f64::INFINITY, f64::NAN);
            let mut previous = None;
            for (i, day) in input.day.iter().enumerate().take(n) {
                let period = pivot_period(*day, config.input("period") as usize);
                if current != Some(period) {
                    if current.is_some() && hi.is_finite() && lo.is_finite() && close.is_finite() {
                        previous = Some((hi, lo, close));
                    }
                    current = Some(period);
                    hi = f64::NEG_INFINITY;
                    lo = f64::INFINITY;
                }
                if let Some((h, l, c)) = previous {
                    let p = (h + l + c) / 3.0;
                    columns[0][i] = p;
                    columns[1][i] = 2.0 * p - l;
                    columns[2][i] = 2.0 * p - h;
                    columns[3][i] = p + h - l;
                    columns[4][i] = p - h + l;
                    columns[5][i] = h + 2.0 * (p - l);
                    columns[6][i] = l - 2.0 * (h - p);
                }
                hi = hi.max(input.high[i]);
                lo = lo.min(input.low[i]);
                close = input.close[i];
            }
            let keys = ["p", "r1", "s1", "r2", "s2", "r3", "s3"];
            StudyOutput {
                plots: keys
                    .into_iter()
                    .zip(columns)
                    .map(|(key, v)| plot(key, v))
                    .collect(),
                ..StudyOutput::default()
            }
        }
        StudyKind::Fractals => {
            let wing = config.length("wing");
            let (mut high, mut low) = (vec![f64::NAN; n], vec![f64::NAN; n]);
            for i in wing..n.saturating_sub(wing) {
                if (i - wing..i + wing + 1).all(|j| j == i || input.high[i] > input.high[j]) {
                    high[i] = input.high[i];
                }
                if (i - wing..i + wing + 1).all(|j| j == i || input.low[i] < input.low[j]) {
                    low[i] = input.low[i];
                }
            }
            let mut a = plot("high", high);
            a.kind = PlotKind::Dots;
            let mut b = plot("low", low);
            b.kind = PlotKind::Dots;
            StudyOutput {
                plots: vec![a, b],
                ..StudyOutput::default()
            }
        }
        StudyKind::StochasticRsi => {
            let rsi = math::rsi(&source(), config.length("rsi"));
            let hi = math::highest(&rsi, config.length("stoch"));
            let lo = math::lowest(&rsi, config.length("stoch"));
            let raw: Vec<f64> = (0..n)
                .map(|i| 100.0 * ratio(rsi[i] - lo[i], hi[i] - lo[i]))
                .collect();
            let k = math::sma(&raw, config.length("k"));
            let d = math::sma(&k, config.length("d"));
            StudyOutput {
                plots: vec![plot("k", k), plot("d", d)],
                levels: vec![config.input("upper"), config.input("lower")],
                band: Some((config.input("lower"), config.input("upper"))),
                ..StudyOutput::default()
            }
        }
        StudyKind::Roc => StudyOutput {
            levels: vec![0.0],
            ..values("roc", math::roc(&source(), config.length("length")))
        },
        StudyKind::AwesomeOscillator | StudyKind::AcceleratorOscillator => {
            let median = input.source(4.0);
            let fast = math::sma(&median, config.length("fast"));
            let slow = math::sma(&median, config.length("slow"));
            let ao: Vec<f64> = fast.iter().zip(&slow).map(|(a, b)| a - b).collect();
            let (key, data) = if kind == StudyKind::AcceleratorOscillator {
                let signal = math::sma(&ao, config.length("signal"));
                ("ac", ao.iter().zip(&signal).map(|(a, b)| a - b).collect())
            } else {
                ("ao", ao)
            };
            let up = data
                .iter()
                .enumerate()
                .map(|(i, value)| i == 0 || *value >= data[i - 1])
                .collect();
            let mut p = plot(key, data);
            p.kind = PlotKind::Histogram;
            p.up = Some(up);
            StudyOutput {
                plots: vec![p],
                levels: vec![0.0],
                ..StudyOutput::default()
            }
        }
        StudyKind::Dpo => {
            let src = source();
            let len = config.length("length");
            let ma = math::sma(&src, len);
            let shift = len / 2 + 1;
            values(
                "dpo",
                (0..n)
                    .map(|i| {
                        if i >= shift {
                            src[i] - ma[i - shift]
                        } else {
                            f64::NAN
                        }
                    })
                    .collect(),
            )
        }
        StudyKind::Trix => {
            let a = math::ema(&source(), config.length("length"));
            let b = math::ema(&a, config.length("length"));
            let c = math::ema(&b, config.length("length"));
            let trix: Vec<f64> = (0..n)
                .map(|i| {
                    if i > 0 {
                        100.0 * ratio(c[i] - c[i - 1], c[i - 1])
                    } else {
                        f64::NAN
                    }
                })
                .collect();
            let signal = math::ema(&trix, config.length("signal"));
            StudyOutput {
                plots: vec![plot("trix", trix), plot("signal", signal)],
                levels: vec![0.0],
                ..StudyOutput::default()
            }
        }
        StudyKind::UltimateOscillator => {
            let tr = math::true_range(&input.high, &input.low, &input.close);
            let bp: Vec<f64> = (0..n)
                .map(|i| {
                    input.close[i]
                        - input.low[i].min(if i > 0 {
                            input.close[i - 1]
                        } else {
                            input.close[i]
                        })
                })
                .collect();
            let periods = [
                config.length("short"),
                config.length("middle"),
                config.length("long"),
            ];
            let sums: Vec<_> = periods
                .into_iter()
                .map(|len| (math::sum(&bp, len), math::sum(&tr, len)))
                .collect();
            let out = (0..n)
                .map(|i| {
                    100.0
                        * (4.0 * ratio(sums[0].0[i], sums[0].1[i])
                            + 2.0 * ratio(sums[1].0[i], sums[1].1[i])
                            + ratio(sums[2].0[i], sums[2].1[i]))
                        / 7.0
                })
                .collect();
            StudyOutput {
                levels: vec![70.0, 30.0],
                ..values("uo", out)
            }
        }
        StudyKind::RelativeVigorIndex => {
            let body: Vec<f64> = (0..n).map(|i| input.close[i] - input.open[i]).collect();
            let range: Vec<f64> = (0..n).map(|i| input.high[i] - input.low[i]).collect();
            let smooth = |v: &[f64]| {
                (0..n)
                    .map(|i| {
                        if i >= 3 {
                            (v[i] + 2.0 * v[i - 1] + 2.0 * v[i - 2] + v[i - 3]) / 6.0
                        } else {
                            f64::NAN
                        }
                    })
                    .collect::<Vec<_>>()
            };
            let numerator = math::sma(&smooth(&body), config.length("length"));
            let denominator = math::sma(&smooth(&range), config.length("length"));
            let rvi: Vec<f64> = numerator
                .iter()
                .zip(&denominator)
                .map(|(a, b)| ratio(*a, *b))
                .collect();
            let signal = math::sma(&smooth(&rvi), config.length("signal"));
            StudyOutput {
                plots: vec![plot("rvi", rvi), plot("signal", signal)],
                levels: vec![0.0],
                ..StudyOutput::default()
            }
        }
        StudyKind::DeMarker => {
            let mut up = vec![f64::NAN; n];
            let mut down = up.clone();
            for i in 1..n {
                up[i] = (input.high[i] - input.high[i - 1]).max(0.0);
                down[i] = (input.low[i - 1] - input.low[i]).max(0.0);
            }
            let a = math::sma(&up, config.length("length"));
            let b = math::sma(&down, config.length("length"));
            StudyOutput {
                levels: vec![0.7, 0.3],
                ..values(
                    "dem",
                    a.iter().zip(&b).map(|(u, d)| ratio(*u, *u + *d)).collect(),
                )
            }
        }
        StudyKind::StandardDeviation => {
            values("dev", math::stdev(&source(), config.length("length")))
        }
        StudyKind::TrueRange => values(
            "tr",
            math::true_range(&input.high, &input.low, &input.close),
        ),
        StudyKind::HistoricalVolatility => {
            let returns: Vec<f64> = (0..n)
                .map(|i| {
                    if i > 0 && input.close[i] > 0.0 && input.close[i - 1] > 0.0 {
                        (input.close[i] / input.close[i - 1]).ln()
                    } else {
                        f64::NAN
                    }
                })
                .collect();
            let len = config.length("length");
            let dev = math::stdev(&returns, len);
            values(
                "hv",
                dev.into_iter()
                    .map(|v| v * config.input("annual").sqrt() * 100.0)
                    .collect(),
            )
        }
        StudyKind::ChaikinVolatility => {
            let spread: Vec<f64> = (0..n).map(|i| input.high[i] - input.low[i]).collect();
            let smoothed = math::ema(&spread, config.length("ema"));
            values("cv", math::roc(&smoothed, config.length("roc")))
        }
        StudyKind::ChoppinessIndex => {
            let len = config.length("length");
            let tr = math::true_range(&input.high, &input.low, &input.close);
            let total = math::sum(&tr, len);
            let hi = math::highest(&input.high, len);
            let lo = math::lowest(&input.low, len);
            values(
                "chop",
                (0..n)
                    .map(|i| {
                        let width = hi[i] - lo[i];
                        if width > 0.0 && total[i] > 0.0 {
                            100.0 * (total[i] / width).log10() / (len as f64).log10()
                        } else {
                            f64::NAN
                        }
                    })
                    .collect(),
            )
        }
        StudyKind::MoneyFlowIndex => {
            let out = math::mfi(&typical(), &input.volume, config.length("length"));
            StudyOutput {
                levels: vec![80.0, 20.0],
                ..values("mfi", out)
            }
        }
        StudyKind::ChaikinMoneyFlow | StudyKind::AccumulationDistribution => {
            let flow: Vec<f64> = (0..n)
                .map(|i| {
                    let span = input.high[i] - input.low[i];
                    if span > 0.0 {
                        ((2.0 * input.close[i] - input.high[i] - input.low[i]) / span)
                            * input.volume[i]
                    } else {
                        0.0
                    }
                })
                .collect();
            if kind == StudyKind::AccumulationDistribution {
                values("ad", math::cumulative(&flow))
            } else {
                let a = math::sum(&flow, config.length("length"));
                let b = math::sum(&input.volume, config.length("length"));
                StudyOutput {
                    levels: vec![0.0],
                    ..values(
                        "cmf",
                        a.iter().zip(&b).map(|(x, y)| ratio(*x, *y)).collect(),
                    )
                }
            }
        }
        StudyKind::PriceVolumeTrend => {
            let flow: Vec<f64> = (0..n)
                .map(|i| {
                    if i > 0 {
                        ratio(input.close[i] - input.close[i - 1], input.close[i - 1])
                            * input.volume[i]
                    } else {
                        0.0
                    }
                })
                .collect();
            values("pvt", math::cumulative(&flow))
        }
        StudyKind::ForceIndex => {
            let raw: Vec<f64> = (0..n)
                .map(|i| {
                    if i > 0 {
                        (input.close[i] - input.close[i - 1]) * input.volume[i]
                    } else {
                        f64::NAN
                    }
                })
                .collect();
            StudyOutput {
                levels: vec![0.0],
                ..values("force", math::ema(&raw, config.length("length")))
            }
        }
        StudyKind::VolumeOscillator => {
            let fast = math::ema(&input.volume, config.length("fast"));
            let slow = math::ema(&input.volume, config.length("slow"));
            StudyOutput {
                levels: vec![0.0],
                ..values(
                    "osc",
                    fast.iter()
                        .zip(&slow)
                        .map(|(a, b)| 100.0 * ratio(a - b, *b))
                        .collect(),
                )
            }
        }
        StudyKind::EaseOfMovement => {
            let mut raw = vec![f64::NAN; n];
            for (i, value) in raw.iter_mut().enumerate().skip(1) {
                let distance =
                    (input.high[i] + input.low[i] - input.high[i - 1] - input.low[i - 1]) / 2.0;
                let span = input.high[i] - input.low[i];
                *value = distance * span * config.input("scale") / input.volume[i].max(1.0);
            }
            StudyOutput {
                levels: vec![0.0],
                ..values("emv", math::sma(&raw, config.length("length")))
            }
        }
        _ => unreachable!("not an extended indicator"),
    };
    if matches!(kind, StudyKind::Envelopes | StudyKind::LinearRegression) {
        output.fills.push(FillOut {
            a: 1,
            b: 2,
            color: config.plot_style("upper").color,
            alpha: 0.06,
            other: None,
        });
    }
    output
}
