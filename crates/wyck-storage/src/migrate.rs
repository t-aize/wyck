//! Upgrading the saved files when the layout they are written in changes.
//!
//! Every document and the config carry a `schema_version`. A build knows the version it writes
//! (the `current` of a [`Plan`]) and one [`Step`] per earlier version, each step turning layout `n`
//! into layout `n + 1`. On the first start after an update, a file found at an older version goes
//! through the steps in order, whatever the distance: 1 to 4 runs the steps 1, 2 and 3.
//!
//! The rules, which are the ones migration guides agree on:
//!
//! * **A step only changes the file.** It renames a key, moves a value, drops what is obsolete or
//!   fills what is missing. It reads no clock, no network and nothing of the app, so running a
//!   step twice gives what running it once gives.
//! * **Defaults come after.** The steps work on the raw TOML, so a default never hides a value
//!   that still has to be moved. The app fills the defaults when it reads the upgraded file into
//!   its own type, as it always does.
//! * **A copy first.** Before the first change the file is kept as `name.toml.v<old>.bak`, next to
//!   itself. A copy that is already there is not replaced: the oldest layout is the one worth
//!   keeping.
//! * **All or nothing.** The steps run in memory and the result is written in one atomic write.
//!   A step that fails leaves the file as it was.
//! * **A file from the future is left alone.** A newer file read with what this build knows and
//!   written back would lose what this build does not know. A document is set aside as
//!   `name.toml.from-v<found>` (the app then starts it afresh and the file stays there to be
//!   recovered), the config is refused (see `crate::AppConfig::load`).
//!
//! A change that only adds a field with a default needs no change to the data, but it still takes
//! a new version and a step ([`Step::unchanged`]): the plan must have a step for every version, and
//! [`Plan::is_complete`] is what a test checks, so a version cannot be bumped by forgetting it.
//!
//! A document without a `schema_version` is read as version 1, the first layout.

use std::fs;
use std::path::{Path, PathBuf};

use toml::{Table, Value};
use tracing::{info, warn};

use crate::documents::DocumentStore;
use crate::error::{ConfigError, Result};
use crate::fs_util::{atomic_write, read_text};

/// The key every versioned file carries.
pub(crate) const VERSION_KEY: &str = "schema_version";

/// The version of a file that does not say.
const FIRST_VERSION: u32 = 1;

/// Why a step could not change a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepError(pub &'static str);

/// Turns a file of one version into the next one.
pub(crate) type Apply = fn(&mut Table) -> std::result::Result<(), StepError>;

/// The change from layout `from` to layout `from + 1`.
#[derive(Clone, Copy)]
pub struct Step {
    pub from: u32,
    pub apply: Apply,
}

impl Step {
    /// A step that changes nothing: a new version that only added fields with a default.
    #[must_use]
    pub const fn unchanged(from: u32) -> Self {
        Self { from, apply: keep }
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "a migration step has this signature, whether it can fail or not"
)]
fn keep(_: &mut Table) -> std::result::Result<(), StepError> {
    Ok(())
}

/// How one kind of file gets from any older layout to the one this build writes.
pub struct Plan {
    /// The name of the document (or `config`), for the log and the error.
    pub document: &'static str,
    /// The version this build writes.
    pub current: u32,
    /// One step per earlier version, in order: `from` is 1, 2, ... `current - 1`.
    pub steps: &'static [Step],
}

impl Plan {
    /// Whether there is exactly one step for each version before `current`, in order.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        usize::try_from(self.current.saturating_sub(1)).is_ok_and(|n| n == self.steps.len())
            && self
                .steps
                .iter()
                .zip(FIRST_VERSION..)
                .all(|(step, from)| step.from == from)
    }
}

/// What [`upgrade_table`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Upgrade {
    /// Already the current layout: nothing was changed.
    Current,
    /// Brought from layout `from` to the current one.
    Upgraded { from: u32 },
    /// Written by a newer build: nothing was changed.
    Newer { found: u32 },
}

/// What `upgrade_file` and [`upgrade_document`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// There is no such file.
    Absent,
    /// It was already the current layout.
    Current,
    /// It was upgraded, and the layout it had is kept in `backup`.
    Migrated { from: u32, to: u32, backup: PathBuf },
    /// It was written by a newer build. `kept_as` is where a document was moved to (`None` when
    /// the file was only looked at).
    Newer {
        found: u32,
        kept_as: Option<PathBuf>,
    },
}

fn refused(path: &Path, from: u32, reason: &'static str) -> ConfigError {
    ConfigError::Migration {
        path: path.to_path_buf(),
        from,
        reason,
    }
}

/// The version a table says it has: 1 when it says nothing.
fn version_of(table: &Table, path: &Path) -> Result<u32> {
    match table.get(VERSION_KEY) {
        None => Ok(FIRST_VERSION),
        Some(Value::Integer(n)) => u32::try_from(*n)
            .ok()
            .filter(|n| *n >= FIRST_VERSION)
            .ok_or_else(|| refused(path, 0, "the schema version is out of range")),
        Some(_) => Err(refused(path, 0, "the schema version is not a whole number")),
    }
}

/// Runs the steps a table needs to reach `plan.current`, in order, and sets its version after each.
/// `path` only names the file in an error.
///
/// # Errors
///
/// [`ConfigError::Migration`] when the version is not a number, a step is missing from the plan,
/// or a step fails. The table is then half changed: drop it.
pub(crate) fn upgrade_table(table: &mut Table, plan: &Plan, path: &Path) -> Result<Upgrade> {
    let found = version_of(table, path)?;
    if found == plan.current {
        return Ok(Upgrade::Current);
    }
    if found > plan.current {
        return Ok(Upgrade::Newer { found });
    }
    for version in found..plan.current {
        let step = plan
            .steps
            .iter()
            .find(|step| step.from == version)
            .ok_or_else(|| refused(path, version, "this build has no step for this version"))?;
        (step.apply)(table).map_err(|StepError(reason)| refused(path, version, reason))?;
        table.insert(
            VERSION_KEY.to_owned(),
            Value::Integer(i64::from(version + 1)),
        );
    }
    Ok(Upgrade::Upgraded { from: found })
}

/// Upgrades the file at `path` in place when it is older than `plan.current`: the old text is
/// kept next to it first, then the upgraded file replaces it in one atomic write.
///
/// A file that is current, newer or missing is not touched.
///
/// # Errors
///
/// [`ConfigError::Read`], [`ConfigError::Parse`] when the file is not TOML,
/// [`ConfigError::Migration`] (see [`upgrade_table`]), [`ConfigError::Serialize`] and
/// [`ConfigError::Write`]. On any of them the file is as it was.
pub(crate) fn upgrade_file(path: &Path, plan: &Plan) -> Result<Outcome> {
    let text = match read_text(path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Outcome::Absent),
        Err(source) => {
            return Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut table: Table = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    match upgrade_table(&mut table, plan, path)? {
        Upgrade::Current => Ok(Outcome::Current),
        Upgrade::Newer { found } => Ok(Outcome::Newer {
            found,
            kept_as: None,
        }),
        Upgrade::Upgraded { from } => {
            let upgraded = toml::to_string_pretty(&table).map_err(ConfigError::Serialize)?;
            let backup = backup_path(path, from);
            if !backup.exists() {
                atomic_write(&backup, text.as_bytes())?;
            }
            atomic_write(path, upgraded.as_bytes())?;
            info!(
                document = plan.document,
                from,
                to = plan.current,
                backup = %backup.display(),
                "upgraded a saved file to the current layout"
            );
            Ok(Outcome::Migrated {
                from,
                to: plan.current,
                backup,
            })
        }
    }
}

/// `name.toml.v<from>.bak`, next to `path`.
fn backup_path(path: &Path, from: u32) -> PathBuf {
    sibling(path, &format!("v{from}.bak"))
}

/// `path` with `.suffix` added to its file name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

/// Upgrades the document `plan.document` of `store` (see `upgrade_file`), and moves it aside
/// when it was written by a newer build.
///
/// # Errors
///
/// As `upgrade_file`, and [`ConfigError::Read`] or [`ConfigError::Write`] when a newer file
/// cannot be moved.
pub fn upgrade_document(store: &DocumentStore, plan: &Plan) -> Result<Outcome> {
    let path = store.path(plan.document);
    match upgrade_file(&path, plan)? {
        Outcome::Newer { found, .. } => {
            let kept = set_aside_newer(&path, found)?;
            warn!(
                document = plan.document,
                found,
                supported = plan.current,
                kept_as = %kept.display(),
                "a saved document was written by a newer version and was set aside"
            );
            Ok(Outcome::Newer {
                found,
                kept_as: Some(kept),
            })
        }
        other => Ok(other),
    }
}

/// Moves a file written by a newer build to `name.toml.from-v<found>`, replacing an older copy
/// of the same.
fn set_aside_newer(path: &Path, found: u32) -> Result<PathBuf> {
    let kept = sibling(path, &format!("from-v{found}"));
    let bytes = fs::read(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    atomic_write(&kept, &bytes)?;
    fs::remove_file(path).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    #[expect(
        clippy::unnecessary_wraps,
        reason = "a migration step has this signature, whether it can fail or not"
    )]
    fn rename_zoom(table: &mut Table) -> std::result::Result<(), StepError> {
        if let Some(zoom) = table.remove("zoom") {
            table.insert("scale".to_owned(), zoom);
        }
        Ok(())
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "a migration step has this signature, whether it can fail or not"
    )]
    fn split_columns(table: &mut Table) -> std::result::Result<(), StepError> {
        if let Some(Value::String(text)) = table.get("columns").cloned() {
            let parts = text
                .split(',')
                .map(|s| Value::String(s.to_owned()))
                .collect();
            table.insert("columns".to_owned(), Value::Array(parts));
        }
        Ok(())
    }

    fn fails(_: &mut Table) -> std::result::Result<(), StepError> {
        Err(StepError("the shape is not the expected one"))
    }

    static LAYOUT: Plan = Plan {
        document: "layout",
        current: 4,
        steps: &[
            Step {
                from: 1,
                apply: rename_zoom,
            },
            Step::unchanged(2),
            Step {
                from: 3,
                apply: split_columns,
            },
        ],
    };

    static BROKEN: Plan = Plan {
        document: "layout",
        current: 3,
        steps: &[
            Step {
                from: 1,
                apply: rename_zoom,
            },
            Step {
                from: 2,
                apply: fails,
            },
        ],
    };

    const OLD: &str = "schema_version = 1\nzoom = 1.5\ncolumns = \"a,b\"\nfrom_a_plugin = true\n";

    fn store() -> (tempfile::TempDir, DocumentStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DocumentStore::global(&AppPaths::at(dir.path()));
        fs::create_dir_all(store.dir()).unwrap();
        (dir, store)
    }

    fn read(path: &Path) -> Table {
        toml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    const FIRST: Plan = Plan {
        document: "x",
        current: 1,
        steps: &[],
    };
    const MISSING: Plan = Plan {
        document: "layout",
        current: 3,
        steps: &[Step::unchanged(1)],
    };
    const OUT_OF_ORDER: Plan = Plan {
        document: "x",
        current: 3,
        steps: &[Step::unchanged(2), Step::unchanged(1)],
    };
    const EXTRA: Plan = Plan {
        document: "x",
        current: 2,
        steps: &[Step::unchanged(1), Step::unchanged(2)],
    };

    #[test]
    fn a_plan_has_one_step_for_each_earlier_version() {
        assert!(LAYOUT.is_complete());
        assert!(FIRST.is_complete(), "version 1 has nothing before it");
        assert!(!MISSING.is_complete());
        assert!(!OUT_OF_ORDER.is_complete());
        assert!(!EXTRA.is_complete());
    }

    #[test]
    fn a_file_several_versions_behind_runs_every_step_in_order() {
        let (_dir, store) = store();
        fs::write(store.path("layout"), OLD).unwrap();

        let outcome = upgrade_document(&store, &LAYOUT).unwrap();

        let Outcome::Migrated { from, to, backup } = outcome else {
            panic!("expected a migration, got {outcome:?}");
        };
        assert_eq!((from, to), (1, 4));
        let table = read(&store.path("layout"));
        assert_eq!(table["schema_version"].as_integer(), Some(4));
        assert_eq!(table["scale"].as_float(), Some(1.5), "step 1 renamed zoom");
        assert!(!table.contains_key("zoom"));
        assert_eq!(
            table["columns"].as_array().map(Vec::len),
            Some(2),
            "step 3 split the columns"
        );
        assert_eq!(
            table["from_a_plugin"].as_bool(),
            Some(true),
            "what no step knows is kept"
        );
        assert_eq!(
            fs::read_to_string(backup).unwrap(),
            OLD,
            "the old layout is kept as it was"
        );
    }

    #[test]
    fn a_file_one_version_behind_runs_only_the_steps_it_needs() {
        let (_dir, store) = store();
        fs::write(
            store.path("layout"),
            "schema_version = 3\ncolumns = \"x,y,z\"\nscale = 2.0\n",
        )
        .unwrap();
        upgrade_document(&store, &LAYOUT).unwrap();
        let table = read(&store.path("layout"));
        assert_eq!(table["schema_version"].as_integer(), Some(4));
        assert_eq!(table["columns"].as_array().map(Vec::len), Some(3));
        assert_eq!(table["scale"].as_float(), Some(2.0));
    }

    #[test]
    fn a_file_with_no_version_is_the_first_layout() {
        let (_dir, store) = store();
        fs::write(store.path("layout"), "zoom = 3.0\n").unwrap();
        let outcome = upgrade_document(&store, &LAYOUT).unwrap();
        assert!(matches!(outcome, Outcome::Migrated { from: 1, to: 4, .. }));
        assert_eq!(read(&store.path("layout"))["scale"].as_float(), Some(3.0));
    }

    #[test]
    fn a_current_file_is_not_touched_and_a_second_run_changes_nothing() {
        let (_dir, store) = store();
        let current = "schema_version = 4\nscale = 1.0\n";
        fs::write(store.path("layout"), current).unwrap();
        assert_eq!(upgrade_document(&store, &LAYOUT).unwrap(), Outcome::Current);
        assert_eq!(fs::read_to_string(store.path("layout")).unwrap(), current);

        fs::write(store.path("layout"), OLD).unwrap();
        upgrade_document(&store, &LAYOUT).unwrap();
        let once = fs::read_to_string(store.path("layout")).unwrap();
        assert_eq!(upgrade_document(&store, &LAYOUT).unwrap(), Outcome::Current);
        assert_eq!(fs::read_to_string(store.path("layout")).unwrap(), once);
    }

    #[test]
    fn a_missing_file_is_absent_not_an_error() {
        let (_dir, store) = store();
        assert_eq!(upgrade_document(&store, &LAYOUT).unwrap(), Outcome::Absent);
    }

    #[test]
    fn the_first_copy_is_the_one_kept() {
        let (_dir, store) = store();
        let kept = sibling(&store.path("layout"), "v1.bak");
        fs::write(&kept, "the oldest layout").unwrap();
        fs::write(store.path("layout"), OLD).unwrap();

        upgrade_document(&store, &LAYOUT).unwrap();

        assert_eq!(fs::read_to_string(kept).unwrap(), "the oldest layout");
    }

    #[test]
    fn a_failing_step_leaves_the_file_as_it_was() {
        let (_dir, store) = store();
        fs::write(store.path("layout"), OLD).unwrap();

        let error = upgrade_document(&store, &BROKEN).unwrap_err();

        assert!(
            matches!(error, ConfigError::Migration { from: 2, .. }),
            "{error:?}"
        );
        assert_eq!(fs::read_to_string(store.path("layout")).unwrap(), OLD);
        assert!(!sibling(&store.path("layout"), "v1.bak").exists());
    }

    #[test]
    fn a_plan_without_the_step_is_an_error_not_a_guess() {
        let (_dir, store) = store();
        fs::write(store.path("layout"), OLD).unwrap();
        let error = upgrade_document(&store, &MISSING).unwrap_err();
        assert!(
            matches!(error, ConfigError::Migration { from: 2, .. }),
            "{error:?}"
        );
        assert_eq!(fs::read_to_string(store.path("layout")).unwrap(), OLD);
    }

    #[test]
    fn a_file_from_a_newer_build_is_set_aside_whole() {
        let (_dir, store) = store();
        let future = "schema_version = 9\nsomething_new = \"kept\"\n";
        fs::write(store.path("layout"), future).unwrap();

        let outcome = upgrade_document(&store, &LAYOUT).unwrap();

        let Outcome::Newer { found, kept_as } = outcome else {
            panic!("expected a newer file, got {outcome:?}");
        };
        assert_eq!(found, 9);
        let kept = kept_as.unwrap();
        assert!(kept.ends_with("layout.toml.from-v9"), "{kept:?}");
        assert_eq!(fs::read_to_string(kept).unwrap(), future);
        assert!(!store.path("layout").exists(), "the app starts afresh");
        assert_eq!(
            store.list().unwrap(),
            Vec::<String>::new(),
            "the copy is not a document"
        );
    }

    #[test]
    fn looking_at_a_newer_file_does_not_move_it() {
        let (_dir, store) = store();
        let future = "schema_version = 9\n";
        fs::write(store.path("layout"), future).unwrap();
        let outcome = upgrade_file(&store.path("layout"), &LAYOUT).unwrap();
        assert_eq!(
            outcome,
            Outcome::Newer {
                found: 9,
                kept_as: None
            }
        );
        assert_eq!(fs::read_to_string(store.path("layout")).unwrap(), future);
    }

    #[test]
    fn a_file_that_is_not_toml_is_reported_and_left_alone() {
        let (_dir, store) = store();
        fs::write(store.path("layout"), "zoom = [[[").unwrap();
        let error = upgrade_document(&store, &LAYOUT).unwrap_err();
        assert!(matches!(error, ConfigError::Parse { .. }), "{error:?}");
        assert_eq!(
            fs::read_to_string(store.path("layout")).unwrap(),
            "zoom = [[["
        );
    }

    #[test]
    fn a_version_that_is_not_a_positive_whole_number_is_refused() {
        for bad in ["\"two\"", "0", "-3", "1.5", "99999999999"] {
            let (_dir, store) = store();
            let text = format!("schema_version = {bad}\n");
            fs::write(store.path("layout"), &text).unwrap();
            let error = upgrade_document(&store, &LAYOUT).unwrap_err();
            assert!(
                matches!(error, ConfigError::Migration { .. }),
                "{bad}: {error:?}"
            );
            assert_eq!(fs::read_to_string(store.path("layout")).unwrap(), text);
        }
    }
}
