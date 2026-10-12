//! The input_* calls of a script.

use super::describe::{Options, check_key, declare_input, title_of};
use super::indicators::{color_text, parse_color};

use crate::domain::indicators::custom::run::decl::{
    InputDecl, InputSection, InputType, MAX_INPUTS,
};
use crate::domain::indicators::custom::run::{Mode, source_index, with_run};
use crate::domain::indicators::custom::series::{Fallible, Series};
use rhai::{Array, Map};

pub(super) fn input_section(options: &Options<'_>) -> Fallible<Option<InputSection>> {
    match options.text("section")?.as_deref() {
        None => Ok(None),
        Some("inputs") => Ok(Some(InputSection::Inputs)),
        Some("style") => Ok(Some(InputSection::Style)),
        Some(_) => Err("input section must be \"inputs\" or \"style\"".into()),
    }
}

pub(super) fn input_number(kind: InputType, key: &str, default: f64, opts: Map) -> Fallible<f64> {
    let options = Options::new(
        "input",
        opts,
        &[
            "label", "min", "max", "step", "section", "group", "tooltip", "inline",
        ],
    )?;
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
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind,
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
        options: Vec::new(),
        section: input_section(&options)?,
        text: String::new(),
        group: options.text("group")?,
        tooltip: options.text("tooltip")?,
    })
}

pub(super) fn input_source(key: &str, default: &str, opts: Map) -> Fallible<Series> {
    let options = Options::new(
        "input_source",
        opts,
        &["label", "section", "group", "tooltip", "inline"],
    )?;
    let index = source_index(default).ok_or_else(|| {
        format!(
            "input_source: \"{default}\" is not a price (the prices are {})",
            crate::domain::indicators::spec::SOURCES.join(", ")
        )
    })?;
    let chosen = declare_input(InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind: InputType::Source,
        default: index as f64,
        min: 0.0,
        max: (crate::domain::indicators::spec::SOURCES.len() - 1) as f64,
        step: 1.0,
        options: Vec::new(),
        section: input_section(&options)?,
        text: String::new(),
        group: options.text("group")?,
        tooltip: options.text("tooltip")?,
    })?;
    with_run(|run| run.columns.source(chosen as usize))
}

pub(super) fn input_choice(
    key: &str,
    default: &str,
    choices: Array,
    opts: Map,
) -> Fallible<String> {
    let options = Options::new(
        "input_choice",
        opts,
        &["label", "section", "group", "tooltip", "inline"],
    )?;
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
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind: InputType::Choice,
        default: index as f64,
        min: 0.0,
        max: (names.len() - 1) as f64,
        step: 1.0,
        options: names.clone(),
        section: input_section(&options)?,
        text: String::new(),
        group: options.text("group")?,
        tooltip: options.text("tooltip")?,
    })?;
    Ok(names[chosen as usize].clone())
}

/// The longest text an input may hold.
pub(in crate::domain::indicators::custom) const MAX_TEXT: usize = 200;

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
            && part[..2].parse::<u32>().is_ok_and(|h| h < 24)
            && part[2..].parse::<u32>().is_ok_and(|m| m < 60)
    };
    clock(from) && clock(to)
}

/// An input whose value is text: a symbol, a timeframe, a session or free text.
pub(super) fn input_text(kind: InputType, key: &str, default: &str, opts: Map) -> Fallible<String> {
    let name = match kind {
        InputType::Symbol => "input_symbol",
        InputType::Timeframe => "input_timeframe",
        InputType::Session => "input_session",
        _ => "input_text",
    };
    let options = Options::new(
        name,
        opts,
        &["label", "section", "group", "tooltip", "inline"],
    )?;
    let default = default.trim();
    if default.chars().count() > MAX_TEXT {
        return Err(format!("{name}: the default is over {MAX_TEXT} characters").into());
    }
    match kind {
        InputType::Timeframe if !default.is_empty() => {
            if crate::domain::chart::timeframe::Timeframe::from_code(default).is_none() {
                return Err(format!(
                    "{name}: \"{default}\" is not a timeframe (try 15m, 4h or 1D)"
                )
                .into());
            }
        }
        InputType::Session if !is_session(default) => {
            return Err(format!("{name}: \"{default}\" is not a session (write 0930-1600)").into());
        }
        _ => {}
    }
    let decl = InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind,
        default: 0.0,
        min: 0.0,
        max: 0.0,
        step: 1.0,
        options: Vec::new(),
        section: input_section(&options)?,
        text: default.to_owned(),
        group: options.text("group")?,
        tooltip: options.text("tooltip")?,
    };
    check_key("input", &decl.key)?;
    with_run(|run| {
        if run.declaration.inputs.iter().any(|i| i.key == decl.key) {
            return Err(format!("the input \"{}\" is declared twice", decl.key).into());
        }
        if run.declaration.inputs.len() >= MAX_INPUTS {
            return Err(format!("an indicator has at most {MAX_INPUTS} inputs").into());
        }
        let chosen = match run.mode {
            Mode::Compute => run.texts.get(&decl.key).cloned(),
            Mode::Declare => None,
        }
        .map(|t| t.trim().to_owned())
        .filter(|t| t.chars().count() <= MAX_TEXT)
        .filter(|t| match kind {
            InputType::Timeframe => {
                t.is_empty() || crate::domain::chart::timeframe::Timeframe::from_code(t).is_some()
            }
            InputType::Session => is_session(t),
            _ => true,
        })
        .unwrap_or_else(|| decl.text.clone());
        run.declaration.inputs.push(decl);
        Ok(chosen)
    })?
}

pub(super) fn input_color(key: &str, default: &str, opts: Map) -> Fallible<String> {
    let options = Options::new(
        "input_color",
        opts,
        &["label", "section", "group", "tooltip", "inline"],
    )?;
    let color =
        parse_color(default).ok_or_else(|| format!("input_color: \"{default}\" is not a color"))?;
    let chosen = declare_input(InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind: InputType::Color,
        default: f64::from(color),
        min: 0.0,
        max: f64::from(0x00ff_ffff_u32),
        step: 1.0,
        options: Vec::new(),
        section: input_section(&options)?,
        text: String::new(),
        group: options.text("group")?,
        tooltip: options.text("tooltip")?,
    })?;
    Ok(color_text(chosen as u32))
}
