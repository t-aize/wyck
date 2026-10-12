//! Bringing the saved documents of an older version to the layout this version writes.
//!
//! The engine is [`crate::infra::storage::migrate`]; this module owns what only the app knows: which
//! documents have a `schema_version`, which version each one is at, and the steps between. It runs
//! once at start, after a restore waiting for this start was applied and before anything reads a
//! document, on the shared documents and on those of every account.
//!
//! # Changing the layout of a document
//!
//! 1. Raise the `SCHEMA_VERSION` of the document's type.
//! 2. Add the step for the old version to its plan below: a function when keys move or change
//!    shape, [`Step::unchanged`] when the only news is a new field with a default.
//! 3. Add a fixture of the old layout to `tests/fixtures` and a test that reads it.
//!
//! A test fails until step 2 is done, so a version cannot be raised and forgotten. A document
//! written by a newer version is not read at all: see [`crate::infra::storage::migrate`].
//!
//! The documents with no `schema_version` (appearance, the indicator folder, the editor drafts) are
//! not here. They only rely on every field having a default, which is enough until one of them has
//! to change shape: that is the day it gets a version.

use tracing::{info, warn};

use crate::app::{alerts, drawings, workspace};
use crate::domain::drawings::model::SCHEMA_VERSION as DRAWINGS_VERSION;
use crate::infra::storage::migrate::{Outcome, Plan, Step, upgrade_document};
use crate::infra::storage::{AppPaths, DocumentStore};

/// The layouts of the documents, one plan each.
pub const PLANS: [Plan; 4] = [
    Plan {
        document: workspace::PREFERENCES,
        current: workspace::SCHEMA_VERSION,
        steps: &[],
    },
    Plan {
        document: workspace::WATCHLISTS,
        current: workspace::SCHEMA_VERSION,
        steps: &[],
    },
    Plan {
        document: alerts::DOCUMENT,
        current: alerts::model::SCHEMA_VERSION,
        // Version 2 added the columns of the history, each with a default.
        steps: &[Step::unchanged(1)],
    },
    Plan {
        document: drawings::DOCUMENT,
        current: DRAWINGS_VERSION,
        steps: &[],
    },
];

/// What [`run`] did, to be logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Documents brought to the current layout.
    pub upgraded: usize,
    /// Documents written by a newer version, set aside.
    pub set_aside: usize,
    /// Documents that could not be upgraded and were left as they are.
    pub failed: usize,
}

impl Report {
    /// Whether nothing happened.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Upgrades the documents of `paths`: the shared ones and those of each account.
pub fn run(paths: &AppPaths) -> Report {
    let mut stores = vec![("shared".to_owned(), paths.documents())];
    match DocumentStore::list_scopes(paths) {
        Ok(scopes) => {
            stores.extend(scopes.into_iter().map(|scope| {
                let store = paths.scope(&scope);
                (scope, store)
            }));
        }
        Err(error) => warn!(%error, "could not list the accounts to upgrade their documents"),
    }

    let mut report = Report::default();
    for (label, store) in &stores {
        for plan in &PLANS {
            match upgrade_document(store, plan) {
                Ok(Outcome::Migrated { from, to, .. }) => {
                    info!(
                        store = label,
                        document = plan.document,
                        from,
                        to,
                        "upgraded a document"
                    );
                    report.upgraded += 1;
                }
                Ok(Outcome::Newer { found, kept_as }) => {
                    warn!(
                        store = label,
                        document = plan.document,
                        found,
                        kept_as = ?kept_as,
                        "a document from a newer version was set aside"
                    );
                    report.set_aside += 1;
                }
                Ok(Outcome::Absent | Outcome::Current) => {}
                Err(error) => {
                    // The document is read as it is: when it is damaged, the loader sets it aside.
                    warn!(%error, store = label, document = plan.document, "could not upgrade a document");
                    report.failed += 1;
                }
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::alerts::AlertBook;
    use crate::app::workspace::{Preferences, Watchlists};
    use crate::domain::drawings::model::DrawingsDoc;

    #[test]
    fn every_plan_has_a_step_for_each_version_it_left_behind() {
        for plan in &PLANS {
            assert!(
                plan.is_complete(),
                "`{}` is at version {} but its plan lacks a step: add it in app/migrations.rs",
                plan.document,
                plan.current
            );
        }
    }

    #[test]
    fn every_plan_is_at_the_version_its_document_writes() {
        let written = [
            (
                workspace::PREFERENCES,
                Preferences::default().schema_version,
            ),
            (workspace::WATCHLISTS, Watchlists::default().schema_version),
            (alerts::DOCUMENT, AlertBook::default().schema_version),
            (drawings::DOCUMENT, DrawingsDoc::default().schema_version),
        ];
        for (document, version) in written {
            let plan = PLANS.iter().find(|plan| plan.document == document).unwrap();
            assert_eq!(plan.current, version, "{document}");
        }
        let mut names: Vec<_> = PLANS.iter().map(|plan| plan.document).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PLANS.len(), "one plan per document");
    }

    #[test]
    fn a_fresh_install_has_nothing_to_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        assert!(run(&AppPaths::at(dir.path())).is_empty());
    }

    #[test]
    fn the_documents_of_an_older_version_are_upgraded_everywhere_and_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        // Written by the version that had no log columns in the history, in an account.
        let old_alerts = "schema_version = 1\nnext_id = 4\n[[history]]\nalert = 3\nsymbol = \"EURUSD\"\nat = 1000\ntext = \"EURUSD crossed 1.1\"\n";
        let account = paths.scope("demo-1");
        std::fs::create_dir_all(account.dir()).unwrap();
        std::fs::write(account.path(alerts::DOCUMENT), old_alerts).unwrap();
        let shared = paths.documents();
        std::fs::create_dir_all(shared.dir()).unwrap();
        std::fs::write(
            shared.path(workspace::PREFERENCES),
            include_str!("../../tests/fixtures/preferences-0.1.toml"),
        )
        .unwrap();

        let report = run(&paths);

        assert_eq!(
            report,
            Report {
                upgraded: 1,
                ..Report::default()
            },
            "only the alerts were behind"
        );
        let book: AlertBook = account.load(alerts::DOCUMENT).unwrap().unwrap();
        assert_eq!(book.schema_version, 2);
        assert_eq!(book.history.len(), 1, "the history came through");
        assert_eq!(
            std::fs::read_to_string(
                account
                    .dir()
                    .join(format!("{}.toml.v1.bak", alerts::DOCUMENT))
            )
            .unwrap(),
            old_alerts,
            "the old layout is kept"
        );
        assert!(run(&paths).is_empty(), "a second start has nothing to do");
        let preferences: Preferences = shared.load(workspace::PREFERENCES).unwrap().unwrap();
        assert!(preferences.magnet, "a current document was not touched");
    }

    #[test]
    fn a_document_from_a_newer_version_is_set_aside_and_the_app_starts_it_afresh() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        let shared = paths.documents();
        std::fs::create_dir_all(shared.dir()).unwrap();
        let future = "schema_version = 9\nmagnet = true\nsomething_new = 1\n";
        std::fs::write(shared.path(workspace::PREFERENCES), future).unwrap();

        let report = run(&paths);

        assert_eq!(report.set_aside, 1);
        assert_eq!(
            shared.load_or_default::<Preferences>(workspace::PREFERENCES),
            Preferences::default()
        );
        assert_eq!(
            std::fs::read_to_string(shared.dir().join("preferences.toml.from-v9")).unwrap(),
            future
        );
    }

    #[test]
    fn a_damaged_document_is_counted_and_left_for_the_loader() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        let shared = paths.documents();
        std::fs::create_dir_all(shared.dir()).unwrap();
        std::fs::write(shared.path(workspace::PREFERENCES), "magnet = [[[").unwrap();

        assert_eq!(run(&paths).failed, 1);
        assert_eq!(
            std::fs::read_to_string(shared.path(workspace::PREFERENCES)).unwrap(),
            "magnet = [[["
        );
    }
}
