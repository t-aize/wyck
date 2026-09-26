//! The scripts a new indicator can start from, and the examples the app puts in the folder the
//! first time it is used.
//!
//! They are files in `examples/`, so they read as scripts and are compiled by the tests: an
//! example that does not work cannot ship.

use super::library::{Library, LibraryError};

/// A script to start from.
#[derive(Debug, Clone, Copy)]
pub struct Template {
    /// The name the file gets.
    pub name: &'static str,
    pub description: &'static str,
    pub source: &'static str,
    /// Whether it is also put in the folder as an example.
    pub example: bool,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "My overlay",
        description: "An indicator drawn on the prices, with one input and one line.",
        source: include_str!("examples/blank_overlay.rhai"),
        example: false,
    },
    Template {
        name: "My oscillator",
        description: "An indicator in a pane of its own, with levels.",
        source: include_str!("examples/blank_pane.rhai"),
        example: false,
    },
    Template {
        name: "Two averages",
        description: "Two moving averages of a kind the user picks, shaded between.",
        source: include_str!("examples/two_averages.rhai"),
        example: true,
    },
    Template {
        name: "RSI with bands",
        description: "RSI with levels, a shaded band and a smoothed line.",
        source: include_str!("examples/rsi_bands.rhai"),
        example: true,
    },
    Template {
        name: "Bollinger %B",
        description: "The position of the price between its Bollinger bands.",
        source: include_str!("examples/percent_b.rhai"),
        example: true,
    },
    Template {
        name: "Volume pressure",
        description: "Volume in the color of its bar, with an average.",
        source: include_str!("examples/volume_pressure.rhai"),
        example: true,
    },
    Template {
        name: "Supertrend",
        description: "A trailing line in two colors that flips with the trend.",
        source: include_str!("examples/supertrend.rhai"),
        example: true,
    },
    Template {
        name: "Z-score",
        description: "How far the price is from its average, in standard deviations.",
        source: include_str!("examples/z_score.rhai"),
        example: true,
    },
    Template {
        name: "Recursive smoothing",
        description: "A filter written as a loop, to show how a series is built bar by bar.",
        source: include_str!("examples/smoothing_loop.rhai"),
        example: true,
    },
];

/// The folder the examples go in.
pub const EXAMPLES_FOLDER: &str = "Examples";

impl Library {
    /// Puts the examples in the `Examples` folder, the ones that are not there already. Returns
    /// how many were written.
    ///
    /// # Errors
    ///
    /// When a file cannot be written.
    pub fn install_examples(&mut self) -> Result<usize, LibraryError> {
        let mut written = 0;
        for template in TEMPLATES.iter().filter(|t| t.example) {
            let id = format!("{EXAMPLES_FOLDER}/{}", template.name);
            if self.path_of(&id).exists() {
                continue;
            }
            self.create(Some(EXAMPLES_FOLDER), template.name, template.source)?;
            written += 1;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::super::StudyInput;
    use super::super::run::{Limits, Script};
    use super::*;

    fn bars(n: usize) -> StudyInput {
        let close: Vec<f64> = (0..n)
            .map(|i| 100.0 + (i as f64 * 0.21).sin() * 7.0 + i as f64 * 0.03)
            .collect();
        StudyInput {
            time: (0..n as i64).map(|i| i * 60_000).collect(),
            open: close.iter().map(|c| c - 0.3).collect(),
            high: close.iter().map(|c| c + 1.0).collect(),
            low: close.iter().map(|c| c - 1.0).collect(),
            volume: (0..n).map(|i| 5.0 + (i % 9) as f64).collect(),
            day: (0..n).map(|i| (i / 50) as i64).collect(),
            close,
        }
    }

    #[test]
    fn every_template_compiles_computes_and_is_plain_ascii() {
        let input = bars(400);
        for template in TEMPLATES {
            assert!(template.source.is_ascii(), "{} is not ASCII", template.name);
            let script = Script::compile(template.source)
                .unwrap_or_else(|p| panic!("{}: {p:?}", template.name));
            assert!(!script.declaration.plots.is_empty(), "{}", template.name);
            assert!(
                script.declaration.meta.name.is_some(),
                "{} has no name",
                template.name
            );
            let done = script
                .compute(&input, &BTreeMap::new(), Limits::default(), None)
                .unwrap_or_else(|p| panic!("{}: {p:?}", template.name));
            assert_eq!(done.output.plots.len(), script.declaration.plots.len());
            assert!(
                done.output
                    .plots
                    .iter()
                    .any(|p| p.values.iter().any(|v| v.is_finite())),
                "{} draws nothing",
                template.name
            );
        }
    }

    #[test]
    fn a_template_is_named_as_it_says_it_is() {
        for template in TEMPLATES {
            let script = Script::compile(template.source).unwrap();
            assert_eq!(
                script.declaration.meta.name.as_deref(),
                Some(template.name),
                "the file of {} says another name",
                template.name
            );
        }
        let mut names: Vec<_> = TEMPLATES.iter().map(|t| t.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TEMPLATES.len());
    }

    #[test]
    fn the_examples_are_put_in_the_folder_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::new(dir.path());
        let count = TEMPLATES.iter().filter(|t| t.example).count();
        assert_eq!(library.install_examples().unwrap(), count);
        assert_eq!(library.install_examples().unwrap(), 0);
        let entries = library.entries();
        assert_eq!(entries.len(), count);
        assert!(
            entries
                .iter()
                .all(|e| e.is_ready() && e.id.starts_with("Examples/"))
        );
        assert!(entries.iter().all(|e| e.info.category == "Examples"));
    }
}
