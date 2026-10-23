//! The calls that describe an indicator.

use super::declare::Options;
use super::inputs::{input_choice, input_color, input_number, input_source, input_text};
use super::plots::{band, fill, hline, plot, plot_number};
use crate::base::math::{f64_to_i64, f64_to_u32, i64_to_f64};

use crate::indicators::custom::context::with_run;
use crate::indicators::custom::decl::{InputType, Meta};
use crate::indicators::custom::series::{Fallible, Series, number_of, truth};
use crate::indicators::spec::ValueFormat;
use rhai::{Array, Engine, Map};

pub(super) fn describing(engine: &mut Engine) {
    engine.register_fn("indicator", |opts: Map| -> Fallible<()> { indicator(opts) });

    // Inputs.
    engine.register_fn("input_int", |key: &str, default: i64| {
        input_number(InputType::Int, key, i64_to_f64(default), Map::new()).map(f64_to_i64)
    });
    engine.register_fn("input_int", |key: &str, default: i64, opts: Map| {
        input_number(InputType::Int, key, i64_to_f64(default), opts).map(f64_to_i64)
    });
    engine.register_fn("input_float", |key: &str, default: f64| {
        input_number(InputType::Float, key, default, Map::new())
    });
    engine.register_fn("input_float", |key: &str, default: f64, opts: Map| {
        input_number(InputType::Float, key, default, opts)
    });
    engine.register_fn("input_float", |key: &str, default: i64| {
        input_number(InputType::Float, key, i64_to_f64(default), Map::new())
    });
    engine.register_fn("input_float", |key: &str, default: i64, opts: Map| {
        input_number(InputType::Float, key, i64_to_f64(default), opts)
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
        plot_number(key, i64_to_f64(value), Map::new())
    });
    engine.register_fn("plot", |key: &str, value: f64, opts: Map| {
        plot_number(key, value, opts)
    });
    engine.register_fn("plot", |key: &str, value: i64, opts: Map| {
        plot_number(key, i64_to_f64(value), opts)
    });
    engine.register_fn("hline", |value: f64| hline(value));
    engine.register_fn("hline", |value: i64| hline(i64_to_f64(value)));
    engine.register_fn("band", |low: f64, high: f64| band(low, high));
    engine.register_fn("band", |low: i64, high: i64| {
        band(i64_to_f64(low), i64_to_f64(high))
    });
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
        .map_or(2, |d| f64_to_u32(d.clamp(0.0, 8.0)));
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
        let number = number_of;
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
