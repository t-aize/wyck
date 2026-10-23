//! The calls that put something on the chart.

use super::declare::{Options, check_key, title_of};
use crate::base::math::{f32_of, f64_to_i64};

use crate::drawings::model::style::Dash;
use crate::indicators::custom::context::{FillResult, Mode, PlotResult, with_run};
use crate::indicators::custom::decl::{MAX_PLOTS, PlotDecl};
use crate::indicators::custom::series::{Fallible, Series, is_true};
use crate::indicators::spec::PlotKind;
use rhai::Map;

pub(super) fn plot_number(key: &str, value: f64, opts: Map) -> Fallible<()> {
    let len = with_run(|run| run.columns.len)?;
    plot(key, Series::constant(len, value), opts)
}

pub(super) fn plot(key: &str, series: Series, opts: Map) -> Fallible<()> {
    check_key("plot", key)?;
    let options = Options::new(
        "plot",
        opts,
        &["title", "color", "width", "style", "dash", "offset", "up"],
    )?;
    let kind = match options.text("style")?.as_deref() {
        None | Some("line") => PlotKind::Line,
        Some("histogram") => PlotKind::Histogram,
        Some("dots") => PlotKind::Dots,
        Some(other) => {
            return Err(format!(
                "plot: the style \"{other}\" is not one of line, histogram and dots"
            )
            .into());
        }
    };
    let dash = match options.text("dash")?.as_deref() {
        None | Some("solid") => Dash::Solid,
        Some("dashed") => Dash::Dashed,
        Some("dotted") => Dash::Dotted,
        Some(other) => {
            return Err(format!(
                "plot: the dash \"{other}\" is not one of solid, dashed and dotted"
            )
            .into());
        }
    };
    let width = f32_of(options.number("width")?.unwrap_or(1.5).clamp(0.5, 20.0));
    let color = options.color("color")?;
    let label = options.text("title")?.unwrap_or_else(|| title_of(key));
    let offset = f64_to_i64(
        options
            .number("offset")?
            .unwrap_or(0.0)
            .clamp(-500.0, 500.0),
    );
    let up = options.series("up")?;
    with_run(|run| {
        if run.declaration.plots.iter().any(|p| p.key == key) {
            return Err(format!("the plot \"{key}\" is drawn twice").into());
        }
        if run.declaration.plots.len() >= MAX_PLOTS {
            return Err(format!("an indicator draws at most {MAX_PLOTS} plots").into());
        }
        let number = run.declaration.plots.len();
        let Some(color) = color.or_else(|| PALETTE.get(number % PALETTE.len()).copied()) else {
            return Err("the palette has no color left".into());
        };
        let values = if run.mode == Mode::Compute {
            if series.len() != run.columns.len {
                return Err(format!(
                    "plot \"{key}\" has {} values and the chart has {} bars",
                    series.len(),
                    run.columns.len
                )
                .into());
            }
            series.into_vec()
        } else {
            Vec::new()
        };
        let up = match (&up, run.mode) {
            (Some(up), Mode::Compute) if up.len() == run.columns.len => {
                Some(up.values().iter().map(|v| is_true(*v)).collect())
            }
            (Some(_), Mode::Compute) => {
                return Err("plot: \"up\" must have a value for every bar".into());
            }
            _ => None,
        };
        run.declaration.plots.push(PlotDecl {
            key: key.to_owned(),
            label,
            kind,
            color,
            width,
            dash,
        });
        run.plots.push(PlotResult {
            key: key.to_owned(),
            values,
            offset,
            up,
        });
        Ok(())
    })?
}

/// The colors a plot takes when the script does not choose one, in the order plots are drawn.
pub(super) const PALETTE: [u32; 8] = [
    0x29_62ff, 0xff_9800, 0x26_a69a, 0xef_5350, 0x7e_57c2, 0x00_bcd4, 0xe9_1e63, 0x9c_cc65,
];

/// The most horizontal levels an indicator may draw.
pub(crate) const MAX_LEVELS: usize = 16;

pub(super) fn hline(value: f64) -> Fallible<()> {
    if !value.is_finite() {
        return Err("hline: the level must be a number, not na".into());
    }
    with_run(|run| {
        if run.levels.len() >= MAX_LEVELS {
            return Err(format!("an indicator draws at most {MAX_LEVELS} levels").into());
        }
        run.levels.push(value);
        Ok(())
    })?
}

pub(super) fn band(low: f64, high: f64) -> Fallible<()> {
    if !(low.is_finite() && high.is_finite() && low < high) {
        return Err("band: give the lower level first, under the higher".into());
    }
    with_run(|run| {
        if run.band.is_some() {
            run.warn("band: called twice, only the last one is drawn");
        }
        run.band = Some((low, high));
    })
}

pub(super) fn fill(a: &str, b: &str, opts: Map) -> Fallible<()> {
    let options = Options::new("fill", opts, &["color", "alpha", "down_color"])?;
    let color = options.color("color")?;
    let other = options.color("down_color")?;
    let alpha = f32_of(options.number("alpha")?.unwrap_or(0.1).clamp(0.02, 1.0));
    with_run(|run| {
        let find = |key: &str| -> Fallible<usize> {
            run.plots.iter().position(|p| p.key == key).ok_or_else(|| {
                format!("fill: the plot \"{key}\" is not drawn yet: draw it before filling").into()
            })
        };
        let (index_a, index_b) = (find(a)?, find(b)?);
        let color = color.unwrap_or_else(|| {
            run.declaration
                .plots
                .iter()
                .find(|p| p.key == a)
                .map_or(0x29_62ff, |p| p.color)
        });
        run.fills.push(FillResult {
            a: index_a,
            b: index_b,
            color,
            alpha,
            other,
        });
        Ok(())
    })?
}
