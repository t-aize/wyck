//! The calls that describe an indicator.

use super::indicators::parse_color;
use super::inputs::{input_choice, input_color, input_number, input_source, input_text};
use super::plots::{band, fill, hline, plot, plot_number};

use crate::domain::indicators::custom::run::decl::{InputDecl, InputType, MAX_INPUTS, Meta};
use crate::domain::indicators::custom::run::{Mode, with_run};
use crate::domain::indicators::custom::series::{Fallible, Series, truth};
use crate::domain::indicators::spec::ValueFormat;
use rhai::{Array, Dynamic, Engine, Map};

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
    call: &'a str,
    map: Map,
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
        Ok(Self { call, map })
    }

    pub(super) fn number(&self, key: &str) -> Fallible<Option<f64>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v
                .as_float()
                .ok()
                .or_else(|| v.as_int().ok().map(|i| i as f64))
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

pub(super) fn describing(engine: &mut Engine) {
    engine.register_fn("indicator", |opts: Map| -> Fallible<()> { indicator(opts) });

    // Inputs.
    engine.register_fn("input_int", |key: &str, default: i64| {
        input_number(InputType::Int, key, default as f64, Map::new()).map(|value| value as i64)
    });
    engine.register_fn("input_int", |key: &str, default: i64, opts: Map| {
        input_number(InputType::Int, key, default as f64, opts).map(|value| value as i64)
    });
    engine.register_fn("input_float", |key: &str, default: f64| {
        input_number(InputType::Float, key, default, Map::new())
    });
    engine.register_fn("input_float", |key: &str, default: f64, opts: Map| {
        input_number(InputType::Float, key, default, opts)
    });
    engine.register_fn("input_float", |key: &str, default: i64| {
        input_number(InputType::Float, key, default as f64, Map::new())
    });
    engine.register_fn("input_float", |key: &str, default: i64, opts: Map| {
        input_number(InputType::Float, key, default as f64, opts)
    });
    engine.register_fn("input_bool", |key: &str, default: bool| {
        input_number(InputType::Bool, key, truth(default), Map::new()).map(|v| v != 0.0)
    });
    engine.register_fn("input_bool", |key: &str, default: bool, opts: Map| {
        input_number(InputType::Bool, key, truth(default), opts).map(|v| v != 0.0)
    });
    engine.register_fn("input_source", |key: &str, default: &str| {
        input_source(key, default, Map::new())
    });
    engine.register_fn("input_source", |key: &str, default: &str, opts: Map| {
        input_source(key, default, opts)
    });
    engine.register_fn(
        "input_choice",
        |key: &str, default: &str, options: Array| input_choice(key, default, options, Map::new()),
    );
    engine.register_fn(
        "input_choice",
        |key: &str, default: &str, options: Array, opts: Map| {
            input_choice(key, default, options, opts)
        },
    );
    engine.register_fn("input_symbol", |key: &str, default: &str| {
        input_text(InputType::Symbol, key, default, Map::new())
    });
    engine.register_fn("input_symbol", |key: &str, default: &str, opts: Map| {
        input_text(InputType::Symbol, key, default, opts)
    });
    engine.register_fn("input_timeframe", |key: &str, default: &str| {
        input_text(InputType::Timeframe, key, default, Map::new())
    });
    engine.register_fn("input_timeframe", |key: &str, default: &str, opts: Map| {
        input_text(InputType::Timeframe, key, default, opts)
    });
    engine.register_fn("input_session", |key: &str, default: &str| {
        input_text(InputType::Session, key, default, Map::new())
    });
    engine.register_fn("input_session", |key: &str, default: &str, opts: Map| {
        input_text(InputType::Session, key, default, opts)
    });
    engine.register_fn("input_text", |key: &str, default: &str| {
        input_text(InputType::Text, key, default, Map::new())
    });
    engine.register_fn("input_text", |key: &str, default: &str, opts: Map| {
        input_text(InputType::Text, key, default, opts)
    });
    engine.register_fn("input_color", |key: &str, default: &str| {
        input_color(key, default, Map::new())
    });
    engine.register_fn("input_color", |key: &str, default: &str, opts: Map| {
        input_color(key, default, opts)
    });

    // What it draws.
    engine.register_fn("plot", |key: &str, s: Series| plot(key, s, Map::new()));
    engine.register_fn("plot", |key: &str, s: Series, opts: Map| plot(key, s, opts));
    engine.register_fn("plot", |key: &str, value: f64| {
        plot_number(key, value, Map::new())
    });
    engine.register_fn("plot", |key: &str, value: i64| {
        plot_number(key, value as f64, Map::new())
    });
    engine.register_fn("plot", |key: &str, value: f64, opts: Map| {
        plot_number(key, value, opts)
    });
    engine.register_fn("plot", |key: &str, value: i64, opts: Map| {
        plot_number(key, value as f64, opts)
    });
    engine.register_fn("hline", |value: f64| hline(value));
    engine.register_fn("hline", |value: i64| hline(value as f64));
    engine.register_fn("band", |low: f64, high: f64| band(low, high));
    engine.register_fn("band", |low: i64, high: i64| band(low as f64, high as f64));
    engine.register_fn("fill", |a: &str, b: &str| fill(a, b, Map::new()));
    engine.register_fn("fill", |a: &str, b: &str, opts: Map| fill(a, b, opts));
}

pub(super) fn indicator(opts: Map) -> Fallible<()> {
    let options = Options::new(
        "indicator",
        opts,
        &[
            "name",
            "short",
            "overlay",
            "format",
            "decimals",
            "range",
            "description",
            "author",
            "version",
            "category",
        ],
    )?;
    let mut meta = Meta {
        name: options.text("name")?,
        short: options.text("short")?,
        overlay: options.flag("overlay")?.unwrap_or(false),
        description: options.text("description")?.unwrap_or_default(),
        author: options.text("author")?.unwrap_or_default(),
        version: options.text("version")?.unwrap_or_default(),
        category: options.text("category")?.unwrap_or_default(),
        ..Meta::default()
    };
    let decimals = options
        .number("decimals")?
        .map_or(2, |d| d.clamp(0.0, 8.0) as u32);
    meta.format = match options.text("format")?.as_deref() {
        None | Some("plain") => ValueFormat::Plain(decimals),
        Some("price") => ValueFormat::Price,
        Some("count") => ValueFormat::Count,
        Some(other) => {
            return Err(format!(
                "indicator: the format \"{other}\" is not one of price, plain and count"
            )
            .into());
        }
    };
    if let Some(range) = options.map.get("range") {
        let pair = range.clone().into_array().ok().filter(|a| a.len() == 2);
        let number = |d: &Dynamic| {
            d.as_float()
                .ok()
                .or_else(|| d.as_int().ok().map(|i| i as f64))
        };
        match pair.as_deref() {
            Some([low, high]) => match (number(low), number(high)) {
                (Some(low), Some(high)) if low < high => meta.range = Some((low, high)),
                _ => return Err("indicator: range is [low, high] with low under high".into()),
            },
            _ => return Err("indicator: range is [low, high] with low under high".into()),
        }
    }
    with_run(|run| {
        if run.declared {
            return Err("indicator can only be called once".into());
        }
        run.declared = true;
        run.declaration.meta = meta;
        Ok(())
    })?
}

/// Registers an input, and gives the value the user chose (or the default).
pub(super) fn declare_input(mut input: InputDecl) -> Fallible<f64> {
    check_key("input", &input.key)?;
    with_run(|run| {
        if run.declaration.inputs.iter().any(|i| i.key == input.key) {
            return Err(format!("the input \"{}\" is declared twice", input.key).into());
        }
        if run.declaration.inputs.len() >= MAX_INPUTS {
            return Err(format!("an indicator has at most {MAX_INPUTS} inputs").into());
        }
        let chosen = match run.mode {
            Mode::Compute => run.values.get(&input.key).copied(),
            Mode::Declare => None,
        }
        .filter(|v| v.is_finite())
        .unwrap_or(input.default);
        let value = match input.kind {
            InputType::Float => chosen.clamp(input.min, input.max),
            _ => chosen.clamp(input.min, input.max).round(),
        };
        input.default = input.default.clamp(input.min, input.max);
        run.declaration.inputs.push(input);
        Ok(value)
    })?
}

pub(super) fn title_of(key: &str) -> String {
    let mut text = key.replace('_', " ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}
