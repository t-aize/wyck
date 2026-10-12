//! Reading the prices of another symbol.

use crate::domain::indicators::custom::other::{Aligned, key_of};
use crate::domain::indicators::custom::run::{Mode, with_run};
use crate::domain::indicators::custom::series::Fallible;
use rhai::{Dynamic, Engine, Map};

pub fn symbols(engine: &mut Engine) {
    engine.register_fn("bars_of", bars_of);
}

/// The open, high, low, close and volume of a symbol on the bars of the chart. An empty name is
/// the symbol of the chart. Any other name must be the one an input of type symbol holds, since
/// the app loads the prices of those.
fn bars_of(name: &str) -> Fallible<Map> {
    let key = key_of(name);
    let found = with_run(|run| {
        if key.is_empty() {
            Some(Aligned {
                open: run.columns.open.clone(),
                high: run.columns.high.clone(),
                low: run.columns.low.clone(),
                close: run.columns.close.clone(),
                volume: run.columns.volume.clone(),
            })
        } else if run.mode == Mode::Declare {
            // The declaration run is on no bars: there is nothing to read.
            Some(Aligned::empty())
        } else {
            run.others.get(&key).cloned()
        }
    })?;
    let Some(found) = found else {
        return Err(format!(
            "bars_of: there are no prices for \"{}\"; read the name of the symbol with \
             input_symbol so that Wyck loads it",
            name.trim()
        )
        .into());
    };
    let mut map = Map::new();
    for (field, series) in [
        ("open", found.open),
        ("high", found.high),
        ("low", found.low),
        ("close", found.close),
        ("volume", found.volume),
    ] {
        map.insert(field.into(), Dynamic::from(series));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use crate::domain::indicators::compute::StudyInput;
    use crate::domain::indicators::custom::other::{OtherBars, OtherSymbols, key_of};
    use crate::domain::indicators::custom::run::Script;
    use crate::domain::indicators::custom::run::decl::Limits;
    use std::collections::BTreeMap;

    fn chart() -> StudyInput {
        StudyInput {
            time: vec![60, 120, 180],
            open: vec![1.0; 3],
            high: vec![1.0; 3],
            low: vec![1.0; 3],
            close: vec![10.0, 20.0, 30.0],
            volume: vec![1.0; 3],
            day: vec![0; 3],
        }
    }

    fn other() -> OtherSymbols {
        let mut others = OtherSymbols::new();
        others.insert(
            key_of("gbpusd"),
            OtherBars {
                time: vec![60, 120, 180],
                open: vec![1.0; 3],
                high: vec![1.0; 3],
                low: vec![1.0; 3],
                close: vec![1.0, 2.0, 4.0],
                volume: vec![1.0; 3],
            },
        );
        others
    }

    const SOURCE: &str = "let sym = input_symbol(\"other\", \"GBPUSD\");\n\
        let b = bars_of(sym);\n\
        plot(\"ratio\", close / b.close);";

    #[test]
    fn a_script_reads_the_prices_of_the_symbol_an_input_names() {
        let script = Script::compile(SOURCE).unwrap();
        assert_eq!(script.symbols_wanted(&BTreeMap::new()), ["GBPUSD"]);
        let done = script
            .compute_with_others(
                &chart(),
                &BTreeMap::new(),
                &BTreeMap::new(),
                &other(),
                Limits::default(),
                None,
            )
            .unwrap();
        assert_eq!(done.output.plots[0].values, [10.0, 10.0, 7.5]);
    }

    #[test]
    fn the_name_the_user_typed_replaces_the_default_in_the_list_of_wanted_symbols() {
        let script = Script::compile(SOURCE).unwrap();
        let mut texts = BTreeMap::new();
        texts.insert("other".to_owned(), " usdjpy ".to_owned());
        assert_eq!(script.symbols_wanted(&texts), ["USDJPY"]);
        texts.insert("other".to_owned(), String::new());
        assert!(script.symbols_wanted(&texts).is_empty());
    }

    #[test]
    fn a_symbol_with_no_prices_stops_the_script_with_a_message() {
        let script = Script::compile(SOURCE).unwrap();
        let problems = script
            .compute_with_others(
                &chart(),
                &BTreeMap::new(),
                &BTreeMap::new(),
                &OtherSymbols::new(),
                Limits::default(),
                None,
            )
            .unwrap_err();
        assert!(problems[0].message.contains("no prices for \"GBPUSD\""));
    }

    #[test]
    fn an_empty_name_is_the_symbol_of_the_chart() {
        let script = Script::compile("plot(\"same\", bars_of(\"\").close);").unwrap();
        let done = script
            .compute_with_others(
                &chart(),
                &BTreeMap::new(),
                &BTreeMap::new(),
                &OtherSymbols::new(),
                Limits::default(),
                None,
            )
            .unwrap();
        assert_eq!(done.output.plots[0].values, [10.0, 20.0, 30.0]);
    }
}
