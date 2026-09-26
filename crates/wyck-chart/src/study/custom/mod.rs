//! Indicators written as scripts.
//!
//! A script is a small program in [Rhai](https://rhai.rs), kept in a `.rhai` file in the
//! indicators folder. It says what the indicator is, what the user can change and what it draws,
//! and computes it over the bars of a chart with functions that work on all the bars at once.
//!
//! - [`series`]: the value a script works with, a number for every bar, and its arithmetic.
//! - [`api`]: every function a script can call.
//! - [`run`]: compiling, declaring and computing a script, with limits on what a run may do.

pub mod api;
pub mod docs;
mod draw_api;
pub mod library;
pub mod run;
pub mod series;
pub mod templates;

#[cfg(test)]
mod behavior;

use super::Spec;

/// The spec of the indicator whose script is `id`: what it declared, or an empty one when there is
/// no such script (it was deleted, or its file has a mistake).
pub fn spec_for(id: Option<&str>) -> Spec {
    let id = id.unwrap_or("");
    library::registry::get(id).map_or_else(|| library::missing_spec(id), |entry| entry.spec)
}

pub use run::{Computed, Limits, Problem, Severity};

#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::AtomicBool;

#[cfg(test)]
use super::{StudyConfig, StudyInput};

#[cfg(test)]
/// Computes the indicator a config holds over `input`. The script is looked up by the id in the
/// config, and the config's inputs are the values it reads.
///
/// # Errors
///
/// The problems: the script is not there or does not work, or it stopped on a mistake or on a
/// limit.
pub fn compute(
    config: &StudyConfig,
    input: &StudyInput,
    limits: Limits,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<Computed, Vec<Problem>> {
    let id = config.script.as_deref().unwrap_or("");
    let entry = library::registry::get(id).ok_or_else(|| {
        vec![Problem::error(
            0,
            0,
            format!("there is no indicator called \"{id}\""),
        )]
    })?;
    let Some(script) = entry.script.as_ref() else {
        return Err(entry.problems.clone());
    };
    script.compute(input, &config.inputs, limits, cancel)
}

#[cfg(test)]
mod tests {
    use super::library::{Library, registry};
    use super::*;

    #[test]
    fn a_chart_computes_a_script_it_holds_through_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::new(dir.path());
        library
            .create(
                None,
                "Config compute test",
                "indicator(#{ overlay: true });\n\
                 let len = input_int(\"length\", 3, #{ min: 1, max: 50 });\n\
                 plot(\"ma\", sma(close, len));",
            )
            .unwrap();
        library.publish();
        assert!(registry::contains("Config compute test"));

        let mut config = StudyConfig::for_script("Config compute test");
        assert!(config.is_ready() && config.is_script());
        assert_eq!(config.spec().label, "Config compute test");
        assert_eq!(config.input("length"), 3.0);
        config.inputs.insert("length".to_owned(), 2.0);

        let input = StudyInput {
            time: (0..6).map(|i| i * 60_000).collect(),
            open: vec![1.0; 6],
            high: vec![2.0; 6],
            low: vec![0.0; 6],
            close: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            volume: vec![1.0; 6],
            day: vec![0; 6],
        };
        let done = compute(&config, &input, Limits::default(), None).unwrap();
        let values = &done.output.plots[0].values;
        assert!(values[0].is_nan());
        assert_eq!(&values[1..], &[1.5, 2.5, 3.5, 4.5, 5.5]);
    }

    #[test]
    fn a_script_that_is_gone_keeps_its_saved_settings_and_says_it_is_missing() {
        let mut config = StudyConfig::for_script("Never existed");
        assert!(!config.is_ready());
        config.inputs.insert("length".to_owned(), 7.0);
        let config = config.normalized();
        assert_eq!(config.inputs["length"], 7.0, "the saved values stay");
        assert!(config.spec().inputs.is_empty());
        let problems =
            compute(&config, &StudyInput::default(), Limits::default(), None).unwrap_err();
        assert!(problems[0].message.contains("Never existed"));
    }

    #[test]
    fn a_mistake_typed_in_a_script_does_not_wipe_the_settings_of_the_charts_that_hold_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::new(dir.path());
        let good = "let n2 = input_int(\"length\", 5, #{ min: 1, max: 50 });
plot(\"a\", sma(close, n2));";
        library.create(None, "Broken while typing", good).unwrap();
        library.publish();
        let mut config = StudyConfig::for_script("Broken while typing");
        config.inputs.insert("length".to_owned(), 9.0);
        let config = config.normalized();
        assert_eq!(config.inputs["length"], 9.0);

        library
            .save("Broken while typing", "plot(\"a\", sma(close, );")
            .unwrap();
        library.publish();
        assert!(!config.is_ready());
        let config = config.normalized();
        assert_eq!(
            config.inputs["length"], 9.0,
            "the saved value survives a mistake"
        );
        assert_eq!(config.plots.len(), 1);

        library.save("Broken while typing", good).unwrap();
        library.publish();
        assert!(config.normalized().is_ready());
    }

    #[test]
    fn a_config_of_a_script_is_saved_with_its_id_and_read_back() {
        let mut config = StudyConfig::for_script("Saved id");
        config.inputs.insert("length".to_owned(), 9.0);
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("kind = \"custom\"") && text.contains("script = \"Saved id\""));
        let back: StudyConfig = toml::from_str(&text).unwrap();
        assert_eq!(back, config);
        // The built-in ones save no script.
        assert!(
            !toml::to_string(&StudyConfig::new(super::super::StudyKind::Sma))
                .unwrap()
                .contains("script")
        );
    }
}
