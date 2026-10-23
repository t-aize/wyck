//! What the calls that describe an indicator share: the rules for a key, the options of a call
//! and the declaration of an input.

use crate::indicators::custom::color::parse_color;
use crate::indicators::custom::context::{Mode, Run, with_run};
use crate::indicators::custom::decl::{InputDecl, InputSection, InputType, MAX_INPUTS};
use crate::indicators::custom::series::{Fallible, Series, number_of};
use rhai::Map;

pub(super) fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && key.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub(super) fn check_key(what: &str, key: &str) -> Fallible<()> {
    if valid_key(key) {
        Ok(())
    } else {
        Err(format!(
            "{what} \"{key}\": a name is lowercase letters, digits and _, starts with a letter and has at most 32 characters"
        )
        .into())
    }
}

/// The options of a call, read one by one; anything that is not asked for is an error, so a
/// misspelled option does not pass silently.
pub(super) struct Options<'a> {
    pub(super) call: &'a str,
    pub(super) map: Map,
}

impl<'a> Options<'a> {
    pub(super) fn new(call: &'a str, map: Map, allowed: &'a [&'a str]) -> Fallible<Self> {
        for key in map.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(format!(
                    "{call}: unknown option \"{key}\" (the options are: {})",
                    allowed.join(", ")
                )
                .into());
            }
        }
        if map.contains_key("inline") {
            // Accepted so that a script written for another platform runs, and said not to work.
            with_run(|run| {
                run.warn(format!(
                    "{call}: the option \"inline\" has no effect, every input has its own row"
                ));
            })
            .ok();
        }
        Ok(Self { call, map })
    }

    pub(super) fn number(&self, key: &str) -> Fallible<Option<f64>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => number_of(v)
                .filter(|x| x.is_finite())
                .map(Some)
                .ok_or_else(|| {
                    format!("{}: the option \"{key}\" must be a number", self.call).into()
                }),
        }
    }

    pub(super) fn text(&self, key: &str) -> Fallible<Option<String>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v
                .clone()
                .into_string()
                .map(Some)
                .map_err(|_| format!("{}: the option \"{key}\" must be text", self.call).into()),
        }
    }

    pub(super) fn flag(&self, key: &str) -> Fallible<Option<bool>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).map_err(|_| {
                format!("{}: the option \"{key}\" must be true or false", self.call).into()
            }),
        }
    }

    pub(super) fn color(&self, key: &str) -> Fallible<Option<u32>> {
        match self.text(key)? {
            None => Ok(None),
            Some(text) => parse_color(&text).map(Some).ok_or_else(|| {
                format!(
                    "{}: \"{text}\" is not a color (write #rrggbb or a name such as orange)",
                    self.call
                )
                .into()
            }),
        }
    }

    pub(super) fn series(&self, key: &str) -> Fallible<Option<Series>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v.clone().try_cast::<Series>().map(Some).ok_or_else(|| {
                format!("{}: the option \"{key}\" must be a series", self.call).into()
            }),
        }
    }
}

/// Adds `input` to the declaration of the run and gives back what `chosen` reads for it: the
/// value of the run for the input, or its default. Every kind of input goes through here, so the
/// rules (a valid key, declared once, at most [`MAX_INPUTS`]) are in one place.
pub(super) fn register<R>(
    input: InputDecl,
    chosen: impl FnOnce(&Run, &InputDecl) -> R,
) -> Fallible<R> {
    check_key("input", &input.key)?;
    with_run(|run| {
        if run.declaration.inputs.iter().any(|i| i.key == input.key) {
            return Err(format!("the input \"{}\" is declared twice", input.key).into());
        }
        if run.declaration.inputs.len() >= MAX_INPUTS {
            return Err(format!("an indicator has at most {MAX_INPUTS} inputs").into());
        }
        let value = chosen(run, &input);
        run.declaration.inputs.push(input);
        Ok(value)
    })?
}

/// Registers an input of a number, and gives the value the user chose (or the default).
pub(super) fn declare_input(mut input: InputDecl) -> Fallible<f64> {
    input.default = input.default.clamp(input.min, input.max);
    register(input, |run, input| {
        let chosen = match run.mode {
            Mode::Compute => run.values.get(&input.key).copied(),
            Mode::Declare => None,
        }
        .filter(|v| v.is_finite())
        .unwrap_or(input.default);
        match input.kind {
            InputType::Float => chosen.clamp(input.min, input.max),
            _ => chosen.clamp(input.min, input.max).round(),
        }
    })
}

pub(super) fn title_of(key: &str) -> String {
    let mut text = key.replace('_', " ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

/// The tab an input is shown on, when the script chose one.
pub(super) fn input_section(options: &Options<'_>) -> Fallible<Option<InputSection>> {
    match options.text("section")?.as_deref() {
        None => Ok(None),
        Some("inputs") => Ok(Some(InputSection::Inputs)),
        Some("style") => Ok(Some(InputSection::Style)),
        Some(_) => Err("input section must be \"inputs\" or \"style\"".into()),
    }
}

/// An input as every `input_*` call starts it: the label, the tab, the heading and the tooltip
/// come from the options, and the call fills in the rest.
pub(super) fn base_input(options: &Options<'_>, key: &str, kind: InputType) -> Fallible<InputDecl> {
    Ok(InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind,
        default: 0.0,
        min: 0.0,
        max: 0.0,
        step: 1.0,
        options: Vec::new(),
        section: input_section(options)?,
        text: String::new(),
        group: options.text("group")?,
        tooltip: options.text("tooltip")?,
    })
}
