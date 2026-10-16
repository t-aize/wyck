//! Additional built-in indicators. Each output stays aligned with the input bars.

pub(crate) mod specs;

use super::compute::{FillOut, PlotOut, StudyInput, StudyOutput, plot};
use super::config::StudyConfig;
use super::kind::StudyKind;
use super::spec::PlotKind;
use chrono::{Datelike, NaiveDate};
use wyck_core::math::{self, Series, count_f64, f64_to_i64, f64_to_usize};

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
        2 => i32::try_from(day + 719_163)
            .ok()
            .and_then(NaiveDate::from_num_days_from_ce_opt)
            .map_or(day, |d| i64::from(d.year()) * 12 + i64::from(d.month())),
        3 => i32::try_from(day + 719_163)
            .ok()
            .and_then(NaiveDate::from_num_days_from_ce_opt)
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
                    .max_by(|&a, &b| input.high.at(a).total_cmp(&input.high.at(b)))
                    .unwrap_or(i);
                let l = (start..=i)
                    .min_by(|&a, &b| input.low.at(a).total_cmp(&input.low.at(b)))
                    .unwrap_or(i);
                up.put(i, 100.0 * count_f64(len - (i - h)) / count_f64(len));
                down.put(i, 100.0 * count_f64(len - (i - l)) / count_f64(len));
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
                offset: f64_to_i64(config.input(shift)),
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
                f64_to_usize(config.input("method")),
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
                    if i + 1 < len || !basis.at(i).is_finite() {
                        return f64::NAN;
                    }
                    let start = basis.at(i) - slope.at(i) * count_f64(len - 1);
                    ((0..len)
                        .map(|j| {
                            (src.at(i + 1 - len + j) - start - slope.at(i) * count_f64(j)).powi(2)
                        })
                        .sum::<f64>()
                        / count_f64(len))
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
                let period = pivot_period(*day, f64_to_usize(config.input("period")));
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
                    let levels = [
                        p,
                        2.0 * p - l,
                        2.0 * p - h,
                        p + h - l,
                        p - h + l,
                        h + 2.0 * (p - l),
                        l - 2.0 * (h - p),
                    ];
                    for (column, level) in columns.iter_mut().zip(levels) {
                        column.put(i, level);
                    }
                }
                hi = hi.max(input.high.at(i));
                lo = lo.min(input.low.at(i));
                close = input.close.at(i);
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
                if ((i - wing)..=(i + wing)).all(|j| j == i || input.high.at(i) > input.high.at(j))
                {
                    high.put(i, input.high.at(i));
                }
                if ((i - wing)..=(i + wing)).all(|j| j == i || input.low.at(i) < input.low.at(j)) {
                    low.put(i, input.low.at(i));
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
                .map(|i| 100.0 * ratio(rsi.at(i) - lo.at(i), hi.at(i) - lo.at(i)))
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
                .map(|(i, value)| i == 0 || *value >= data.at(i - 1))
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
                            src.at(i) - ma.at(i - shift)
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
                        100.0 * ratio(c.at(i) - c.at(i - 1), c.at(i - 1))
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
                    input.close.at(i)
                        - input.low.at(i).min(if i > 0 {
                            input.close.at(i - 1)
                        } else {
                            input.close.at(i)
                        })
                })
                .collect();
            let periods = [
                config.length("short"),
                config.length("middle"),
                config.length("long"),
            ];
            let [short, middle, long] =
                periods.map(|len| (math::sum(&bp, len), math::sum(&tr, len)));
            let out = (0..n)
                .map(|i| {
                    100.0
                        * (4.0 * ratio(short.0.at(i), short.1.at(i))
                            + 2.0 * ratio(middle.0.at(i), middle.1.at(i))
                            + ratio(long.0.at(i), long.1.at(i)))
                        / 7.0
                })
                .collect();
            StudyOutput {
                levels: vec![70.0, 30.0],
                ..values("uo", out)
            }
        }
        StudyKind::RelativeVigorIndex => {
            let body: Vec<f64> = (0..n)
                .map(|i| input.close.at(i) - input.open.at(i))
                .collect();
            let range: Vec<f64> = (0..n).map(|i| input.high.at(i) - input.low.at(i)).collect();
            let smooth = |v: &[f64]| {
                (0..n)
                    .map(|i| {
                        if i >= 3 {
                            (v.at(i) + 2.0 * v.at(i - 1) + 2.0 * v.at(i - 2) + v.at(i - 3)) / 6.0
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
                up.put(i, (input.high.at(i) - input.high.at(i - 1)).max(0.0));
                down.put(i, (input.low.at(i - 1) - input.low.at(i)).max(0.0));
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
                    if i > 0 && input.close.at(i) > 0.0 && input.close.at(i - 1) > 0.0 {
                        (input.close.at(i) / input.close.at(i - 1)).ln()
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
            let spread: Vec<f64> = (0..n).map(|i| input.high.at(i) - input.low.at(i)).collect();
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
                        let width = hi.at(i) - lo.at(i);
                        if width > 0.0 && total.at(i) > 0.0 {
                            100.0 * (total.at(i) / width).log10() / count_f64(len).log10()
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
                    let span = input.high.at(i) - input.low.at(i);
                    if span > 0.0 {
                        ((2.0 * input.close.at(i) - input.high.at(i) - input.low.at(i)) / span)
                            * input.volume.at(i)
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
                        ratio(
                            input.close.at(i) - input.close.at(i - 1),
                            input.close.at(i - 1),
                        ) * input.volume.at(i)
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
                        (input.close.at(i) - input.close.at(i - 1)) * input.volume.at(i)
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
                let distance = (input.high.at(i) + input.low.at(i)
                    - input.high.at(i - 1)
                    - input.low.at(i - 1))
                    / 2.0;
                let span = input.high.at(i) - input.low.at(i);
                *value = distance * span * config.input("scale") / input.volume.at(i).max(1.0);
            }
            StudyOutput {
                levels: vec![0.0],
                ..values("emv", math::sma(&raw, config.length("length")))
            }
        }
        // Not an extended indicator: it has no lines, like the kind that is not known.
        _ => StudyOutput::default(),
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
