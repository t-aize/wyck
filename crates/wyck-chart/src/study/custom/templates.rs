//! The two blank scripts a new indicator can start from.

/// A script to start from.
#[derive(Debug, Clone, Copy)]
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    pub source: &'static str,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "My overlay",
        description: "An indicator drawn on the prices, with one input and one line.",
        source: include_str!("starters/blank_overlay.rhai"),
    },
    Template {
        name: "My oscillator",
        description: "An indicator in a pane of its own, with levels.",
        source: include_str!("starters/blank_pane.rhai"),
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::super::StudyInput;
    use super::super::run::{Limits, Script};
    use super::*;

    #[test]
    fn starters_compile_and_compute() {
        let input = StudyInput {
            time: (0..100).map(|i| i * 60_000).collect(),
            open: vec![100.0; 100],
            high: vec![101.0; 100],
            low: vec![99.0; 100],
            close: vec![100.0; 100],
            volume: vec![10.0; 100],
            day: vec![0; 100],
        };
        for template in TEMPLATES {
            assert!(template.source.is_ascii());
            let script = Script::compile(template.source).unwrap();
            assert_eq!(script.declaration.meta.name.as_deref(), Some(template.name));
            assert!(
                script
                    .compute(&input, &BTreeMap::new(), Limits::default(), None)
                    .is_ok()
            );
        }
    }
}
