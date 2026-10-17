//! Checking a move before it is staged or made: what it would do to the files, and why it is
//! refused.

use std::path::{Path, PathBuf};

use super::tree::{data_files, normalized, within};
use super::{Action, Change, PlanError, Step};
use crate::locations::{self, Kind, Use};
use crate::paths::AppPaths;

fn refuse_sensitive(paths: &AppPaths, to: &Path) -> Result<(), PlanError> {
    let secrets = paths.secrets_dir();
    let holds = |protected: &Path| within(protected, to);
    if holds(paths.config_dir()) || holds(paths.data_dir()) || within(to, &secrets) {
        return Err(PlanError::Sensitive(to.to_path_buf()));
    }
    Ok(())
}

/// What the moves in `changes` would do, checked against the folders as they are now. A change that
/// leaves a kind where it already is is left out.
///
/// # Errors
///
/// The [`PlanError`] of the first change that cannot be made.
pub fn plan(paths: &AppPaths, changes: &[Change]) -> Result<Vec<Step>, PlanError> {
    let mut steps = Vec::new();
    // Where every kind will be once the changes are made, to see that no two are mixed.
    let mut after: Vec<(Kind, PathBuf)> = Kind::ALL
        .into_iter()
        .map(|kind| (kind, paths.dir(kind)))
        .collect();

    for change in changes {
        let standard = paths.standard_dir(change.kind);
        let (to, to_standard) = match &change.to {
            Some(dir) if normalized(dir) != normalized(standard) => (dir.clone(), false),
            _ => (standard.to_path_buf(), true),
        };
        let from = paths.dir(change.kind);
        if normalized(&from) == normalized(&to) {
            continue;
        }
        if to.to_str().is_none() {
            return Err(PlanError::NotText(to));
        }
        let has_data = if to_standard {
            locations::holds_files(&to)
        } else {
            refuse_sensitive(paths, &to)?;
            locations::check(&to, change.kind, paths.mode(), Use::Chosen)?.has_data
        };
        if let Some(slot) = after.iter_mut().find(|(kind, _)| *kind == change.kind) {
            slot.1.clone_from(&to);
        }

        let carried = data_files(&from);
        let action = if !change.kind.carries_files() || carried.is_empty() {
            Action::Switch
        } else if has_data {
            Action::Adopt {
                files: data_files(&to).len(),
            }
        } else {
            Action::Copy {
                files: carried.len(),
                bytes: carried.iter().map(|(_, size)| size).sum(),
            }
        };
        steps.push(Step {
            kind: change.kind,
            from,
            to,
            to_standard,
            action,
        });
    }

    for (index, (a, dir_a)) in after.iter().enumerate() {
        for (b, dir_b) in after.iter().skip(index + 1) {
            if within(dir_a, dir_b) || within(dir_b, dir_a) {
                return Err(PlanError::Mixed { a: *a, b: *b });
            }
        }
    }
    Ok(steps)
}
