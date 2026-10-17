//! The move that waits for the next start: the file it is kept in, and reading it back.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::info;

use super::planning::plan;
use super::{Change, PENDING, PlanError, Step};
use crate::error::{ConfigError, Result};
use crate::fs_util::{atomic_write, read_text};
use crate::locations::Kind;
use crate::paths::AppPaths;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StagedChange {
    pub(super) kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) to: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Staged {
    #[serde(default)]
    pub(super) schema_version: u32,
    pub(super) changes: Vec<StagedChange>,
}

/// Checks the moves in `changes` and puts them aside to be made at the next start. Returns what
/// they will do. Nothing is written when there is nothing to do. A move that waited is replaced:
/// the last thing asked for wins.
pub fn stage(paths: &AppPaths, changes: &[Change]) -> std::result::Result<Vec<Step>, PlanError> {
    let steps = plan(paths, changes)?;
    if steps.is_empty() {
        return Ok(steps);
    }
    let staged = Staged {
        schema_version: 1,
        changes: steps
            .iter()
            .map(|step| StagedChange {
                kind: step.kind.code().to_owned(),
                to: (!step.to_standard).then(|| step.to.clone()),
            })
            .collect(),
    };
    let text = toml::to_string_pretty(&staged).map_err(ConfigError::Serialize)?;
    atomic_write(&paths.config_dir().join(PENDING), text.as_bytes())?;
    info!(
        count = steps.len(),
        "staged a move of saved data for the next start"
    );
    Ok(steps)
}

/// What waits for the next start, if anything does.
#[must_use]
pub fn pending(paths: &AppPaths) -> Option<Vec<Change>> {
    let text = read_text(&paths.config_dir().join(PENDING)).ok()?;
    parse_staged(&text)
}

pub(super) fn parse_staged(text: &str) -> Option<Vec<Change>> {
    let staged: Staged = toml::from_str(text).ok()?;
    staged
        .changes
        .into_iter()
        .map(|change| {
            Some(Change {
                kind: Kind::from_code(&change.kind)?,
                to: change.to,
            })
        })
        .collect()
}

/// Forgets a move that waits. Nothing waiting is fine.
///
/// # Errors
///
/// [`ConfigError::Write`] when the file exists and cannot go.
pub fn cancel(paths: &AppPaths) -> Result<()> {
    remove_file(&paths.config_dir().join(PENDING))
}

pub(super) fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ConfigError::Write {
            path: path.to_path_buf(),
            source,
        }),
    }
}
