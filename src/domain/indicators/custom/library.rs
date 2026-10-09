//! What the app knows about a scripted indicator: the compiled [`Entry`] of one file and the
//! [`registry`] every chart looks indicators up in.
//!
//! An indicator is a `.rhai` file. Its id is its path from the scripts folder without the
//! extension, `trend/my_average`, and that id is what a chart saves to remember which indicator it
//! holds. Reading the folder and the things a user does to files live in
//! `crate::app::scripts::library`; this module has no I/O.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use super::super::intern::{self, Slices};
use super::super::{InputSpec, Placement, PlotSpec, Spec};
use super::run::{Declaration, Limits, Problem, Script};

/// What a script may do while it is being declared (run on no bars): far less than a computation.
const DECLARE_LIMITS: Limits = Limits {
    operations: 2_000_000,
    time: Duration::from_secs(1),
};

/// What is said about an indicator apart from how it computes: the words the menus show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub name: String,
    pub short: String,
    pub category: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

/// One indicator file, as it was read.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The path from the folder, without the extension, with `/` between folders.
    pub id: String,
    pub path: PathBuf,
    pub source: Arc<str>,
    /// The compiled script; `None` when it does not compile or fail while declaring.
    pub script: Option<Arc<Script>>,
    pub problems: Vec<Problem>,
    /// What the panels need to show the indicator and its settings.
    pub spec: Spec,
    pub info: Info,
    /// Tells two versions of a script apart.
    pub stamp: u64,
}

impl Entry {
    pub fn is_ready(&self) -> bool {
        self.script.is_some()
    }

    /// An entry for a file that cannot be used: it keeps its place in the list, listed under
    /// "Broken" with the reasons.
    pub fn broken(id: &str, path: &Path, source: Arc<str>, problems: Vec<Problem>) -> Self {
        let (spec, mut info) = spec_of(id, &Declaration::default());
        info.category = "Broken".to_owned();
        Self {
            id: id.to_owned(),
            path: path.to_path_buf(),
            stamp: stamp_of(&source),
            source,
            script: None,
            problems,
            spec: missing_spec(id).with(spec.label, spec.short),
            info,
        }
    }

    /// Compiles `source`, the text of the file at `path`.
    pub fn compile(id: &str, path: &Path, source: String) -> Self {
        match Script::compile_with(&source, DECLARE_LIMITS) {
            Ok(script) => {
                let (spec, info) = spec_of(id, &script.declaration);
                Self {
                    id: id.to_owned(),
                    path: path.to_path_buf(),
                    stamp: stamp_of(&source),
                    source: Arc::from(source),
                    problems: script.warnings.clone(),
                    script: Some(Arc::new(script)),
                    spec,
                    info,
                }
            }
            Err(problems) => Self::broken(id, path, Arc::from(source), problems),
        }
    }
}

#[derive(Default)]
struct Registry {
    entries: BTreeMap<String, Arc<Entry>>,
    revision: u64,
}

static REGISTRY: RwLock<Option<Registry>> = RwLock::new(None);

fn with_registry<R>(f: impl FnOnce(&mut Registry) -> R) -> R {
    let mut guard = REGISTRY.write().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(Registry::default))
}

/// What every chart looks the indicators it holds up in.
pub mod registry {
    use super::{Arc, BTreeMap, Entry, with_registry};

    /// The indicator called `id`.
    pub fn get(id: &str) -> Option<Arc<Entry>> {
        with_registry(|r| r.entries.get(id).cloned())
    }

    /// Whether there is an indicator called `id`.
    pub fn contains(id: &str) -> bool {
        with_registry(|r| r.entries.contains_key(id))
    }

    /// Every indicator, by id.
    pub fn all() -> Vec<Arc<Entry>> {
        with_registry(|r| r.entries.values().cloned().collect())
    }

    /// A number that grows whenever what is published changes.
    #[cfg(test)]
    pub fn revision() -> u64 {
        with_registry(|r| r.revision)
    }

    /// Publishes `entries` as the whole list. Returns whether it differs from what was there.
    pub fn install(entries: Vec<Arc<Entry>>) -> bool {
        with_registry(|r| {
            let next: BTreeMap<String, Arc<Entry>> =
                entries.into_iter().map(|e| (e.id.clone(), e)).collect();
            let same = next.len() == r.entries.len()
                && next
                    .iter()
                    .all(|(id, e)| r.entries.get(id).is_some_and(|old| old.stamp == e.stamp));
            if !same {
                r.entries = next;
                r.revision += 1;
            }
            !same
        })
    }
}

/// A short name for the legend: the initials of several words, or the first letters of one.
pub fn short_of(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let text: String = if words.len() > 1 {
        words.iter().filter_map(|w| w.chars().next()).collect()
    } else {
        name.chars()
            .filter(|c| c.is_alphanumeric())
            .take(5)
            .collect()
    };
    let text: String = text.to_uppercase().chars().take(6).collect();
    if text.is_empty() {
        "?".to_owned()
    } else {
        text
    }
}

static INPUTS: Slices<InputSpec> = Slices::new();
static PLOTS: Slices<PlotSpec> = Slices::new();

/// The spec the panels use, from what a script declared.
pub fn spec_of(id: &str, declaration: &Declaration) -> (Spec, Info) {
    let meta = &declaration.meta;
    let stem = id.rsplit('/').next().unwrap_or(id);
    let name = meta.name.clone().unwrap_or_else(|| stem.to_owned());
    let short = meta.short.clone().unwrap_or_else(|| short_of(&name));
    let category = if !meta.category.is_empty() {
        meta.category.clone()
    } else if let Some((folder, _)) = id.rsplit_once('/') {
        folder.rsplit('/').next().unwrap_or(folder).to_owned()
    } else {
        "Custom".to_owned()
    };
    let inputs: Vec<InputSpec> = declaration
        .inputs
        .iter()
        .map(|i| InputSpec {
            key: intern::name(&i.key),
            label: intern::name(&i.label),
            kind: i.input_kind(),
            default: i.default,
            min: i.min,
            max: i.max,
            step: i.step,
            group: i.group.as_deref().map_or("", intern::name),
            tooltip: i.tooltip.as_deref().map_or("", intern::name),
            text: intern::name(&i.text),
        })
        .collect();
    let plots: Vec<PlotSpec> = declaration
        .plots
        .iter()
        .map(|p| PlotSpec {
            key: intern::name(&p.key),
            label: intern::name(&p.label),
            kind: p.kind,
            color: p.color,
            width: p.width,
            dash: p.dash,
            visible: true,
        })
        .collect();
    let spec = Spec {
        label: intern::name(&name),
        short: intern::name(&short),
        placement: if meta.overlay {
            Placement::Overlay
        } else {
            Placement::Pane
        },
        format: meta.format,
        inputs: INPUTS.get(&inputs),
        plots: PLOTS.get(&plots),
        range: meta.range,
    };
    let info = Info {
        name,
        short,
        category,
        description: meta.description.clone(),
        author: meta.author.clone(),
        version: meta.version.clone(),
    };
    (spec, info)
}

/// What a chart shows for an indicator whose script is gone or does not work.
pub fn missing_spec(id: &str) -> Spec {
    let stem = id.rsplit('/').next().unwrap_or(id);
    Spec {
        label: intern::name(stem),
        short: intern::name(&short_of(stem)),
        placement: Placement::Overlay,
        format: super::super::ValueFormat::Plain(2),
        inputs: &[],
        plots: &[],
        range: None,
    }
}

fn stamp_of(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

impl Spec {
    /// The same spec under other names.
    fn with(mut self, label: &'static str, short: &'static str) -> Self {
        self.label = label;
        self.short = short;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_are_made_from_the_name() {
        assert_eq!(short_of("Two lines"), "TL");
        assert_eq!(short_of("Momentum"), "MOMEN");
        assert_eq!(short_of("!!!"), "?");
    }
}
