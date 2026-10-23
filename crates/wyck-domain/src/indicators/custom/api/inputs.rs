//! The input_* calls of a script.

use super::declare::{Options, base_input, declare_input, register};
use crate::base::math::{count_f64, f64_to_u32, f64_to_usize};
use crate::indicators::custom::color::{color_text, parse_color};

use crate::indicators::custom::context::Mode;
use crate::indicators::custom::decl::{InputDecl, InputType};
use crate::indicators::custom::series::{Fallible, Series};
use crate::indicators::spec::source_index;
use rhai::{Array, Map};

/// The options every input takes.
const COMMON: [&str; 5] = ["label", "section", "group", "tooltip", "inline"];

/// The options of an input of a number.
const NUMBER: [&str; 8] = [
    "label", "min", "max", "step", "section", "group", "tooltip", "inline",
];

pub(super) fn input_number(kind: InputType, key: &str, default: f64, opts: Map) -> Fallible<f64> {
    let options = Options::new("input", opts, &NUMBER)?;
    let (low, high) = match kind {
        InputType::Bool => (0.0, 1.0),
        _ => (
            options.number("min")?.unwrap_or(-1_000_000.0),
            options.number("max")?.unwrap_or(1_000_000.0),
        ),
    };
    if low > high {
        return Err(format!("input \"{key}\": min is over max").into());
    }
    declare_input(InputDecl {
        default,
        min: low,
        max: high,
        step: options
            .number("step")?
            .filter(|s| *s > 0.0)
            .unwrap_or(match kind {
                InputType::Float => 0.1,
                _ => 1.0,
            }),
        ..base_input(&options, key, kind)?
    })
}

pub(super) fn input_source(key: &str, default: &str, opts: Map) -> Fallible<Series> {
    let options = Options::new("input_source", opts, &COMMON)?;
    let index = source_index(default).ok_or_else(|| {
        format!(
            "input_source: \"{default}\" is not a price (the prices are {})",
            crate::indicators::spec::SOURCES.join(", ")
        )
    })?;
    let chosen = declare_input(InputDecl {
        default: count_f64(index),
        max: count_f64(crate::indicators::spec::SOURCES.len() - 1),
        ..base_input(&options, key, InputType::Source)?
    })?;
    crate::indicators::custom::context::with_run(|run| run.columns.source(f64_to_usize(chosen)))
}

pub(super) fn input_choice(
    key: &str,
    default: &str,
    choices: Array,
    opts: Map,
) -> Fallible<String> {
    let options = Options::new("input_choice", opts, &COMMON)?;
    let names: Vec<String> = choices
        .into_iter()
        .map(|c| {
            c.into_string()
                .map_err(|_| "input_choice: the choices are text")
        })
        .collect::<Result<_, _>>()?;
    if names.is_empty() || names.len() > 32 {
        return Err("input_choice: give between 1 and 32 choices".into());
    }
    let index = names.iter().position(|n| n == default).ok_or_else(|| {
        format!("input_choice: the default \"{default}\" is not one of the choices")
    })?;
    let chosen = declare_input(InputDecl {
        default: count_f64(index),
        max: count_f64(names.len() - 1),
        options: names.clone(),
        ..base_input(&options, key, InputType::Choice)?
    })?;
    names
        .get(f64_to_usize(chosen))
        .cloned()
        .ok_or_else(|| "input: the chosen option does not exist".into())
}

/// The longest text an input may hold.
pub(in crate::indicators::custom) const MAX_TEXT: usize = 200;

/// Whether `text` is a session such as `0930-1600` (or empty, for none).
pub(super) fn is_session(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    let Some((from, to)) = text.split_once('-') else {
        return false;
    };
    let clock = |part: &str| {
        part.len() == 4
            && part.bytes().all(|b| b.is_ascii_digit())
            && part
                .get(..2)
                .and_then(|h| h.parse::<u32>().ok())
                .is_some_and(|h| h < 24)
            && part
                .get(2..)
                .and_then(|m| m.parse::<u32>().ok())
                .is_some_and(|m| m < 60)
    };
    clock(from) && clock(to)
}

/// Whether `text` is acceptable for an input of `kind`.
fn text_fits(kind: InputType, text: &str) -> bool {
    match kind {
        InputType::Timeframe => {
            text.is_empty() || crate::market::Timeframe::from_code(text).is_some()
        }
        InputType::Session => is_session(text),
        _ => true,
    }
}

/// An input whose value is text: a symbol, a timeframe, a session or free text.
pub(super) fn input_text(kind: InputType, key: &str, default: &str, opts: Map) -> Fallible<String> {
    let name = match kind {
        InputType::Symbol => "input_symbol",
        InputType::Timeframe => "input_timeframe",
        InputType::Session => "input_session",
        _ => "input_text",
    };
    let options = Options::new(name, opts, &COMMON)?;
    let default = default.trim();
    if default.chars().count() > MAX_TEXT {
        return Err(format!("{name}: the default is over {MAX_TEXT} characters").into());
    }
    if !default.is_empty() && !text_fits(kind, default) {
        return Err(match kind {
            InputType::Timeframe => {
                format!("{name}: \"{default}\" is not a timeframe (try 15m, 4h or 1D)")
            }
            _ => format!("{name}: \"{default}\" is not a session (write 0930-1600)"),
        }
        .into());
    }
    register(
        InputDecl {
            text: default.to_owned(),
            ..base_input(&options, key, kind)?
        },
        |run, input| {
            match run.mode {
                Mode::Compute => run.texts.get(&input.key).cloned(),
                Mode::Declare => None,
            }
            .map(|t| t.trim().to_owned())
            .filter(|t| t.chars().count() <= MAX_TEXT && text_fits(kind, t))
            .unwrap_or_else(|| input.text.clone())
        },
    )
}

pub(super) fn input_color(key: &str, default: &str, opts: Map) -> Fallible<String> {
    let options = Options::new("input_color", opts, &COMMON)?;
    let color =
        parse_color(default).ok_or_else(|| format!("input_color: \"{default}\" is not a color"))?;
    let chosen = declare_input(InputDecl {
        default: f64::from(color),
        max: f64::from(0x00ff_ffff_u32),
        ..base_input(&options, key, InputType::Color)?
    })?;
    Ok(color_text(f64_to_u32(chosen)))
}
