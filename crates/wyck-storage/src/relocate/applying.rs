//! Making the moves that wait, at the start of the app: each kind on its own, the choice saved only
//! when its files are in place and checked.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use super::copying::{copy_tree, verify};
use super::planning::plan;
use super::staging::{parse_staged, remove_file};
use super::{Action, PENDING, Step};
use crate::error::{ConfigError, Result};
use crate::fs_util::{read_text, remove_stray, retry_when_busy};
use crate::locations::{self, Kind};
use crate::paths::AppPaths;

/// What [`apply_pending`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// The kinds whose files were copied to the new folder.
    pub copied: Vec<Kind>,
    /// The kinds that now use data that was already in the new folder.
    pub adopted: Vec<Kind>,
    /// The kinds moved with nothing to carry.
    pub switched: Vec<Kind>,
    /// The kinds that could not be moved, with why. Each is where it was.
    pub failed: Vec<(Kind, String)>,
    /// Why the whole move was set aside, when it was.
    pub refused: Option<String>,
}

impl Applied {
    /// Whether nothing happened.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the folder of `kind` is not the same as before.
    #[must_use]
    pub fn moved(&self, kind: Kind) -> bool {
        [&self.copied, &self.adopted, &self.switched]
            .iter()
            .any(|kinds| kinds.contains(&kind))
    }
}

/// Makes the moves that wait, if any: call it once at the start of the app, before the folders are
/// read, then read the paths again (see [`AppPaths::discover`]). `stamp` names the folders that
/// are kept aside.
///
/// Each kind is moved on its own and the choice is saved only when its files are in place and
/// checked, so a failure leaves that kind where it was and the others done. The file that waits is
/// removed at the end whatever happened: a move that failed is not tried again at every start.
pub fn apply_pending(paths: &AppPaths, stamp: &str) -> Applied {
    let mut applied = Applied::default();
    let file = paths.config_dir().join(PENDING);
    let text = match read_text(&file) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return applied,
        Err(error) => {
            warn!(%error, "could not read the move that waits");
            applied.refused = Some(format!("the file of the move could not be read: {error}"));
            return applied;
        }
    };
    let Some(changes) = parse_staged(&text) else {
        warn!("the move that waits cannot be read, setting it aside");
        if let Err(error) = fs::rename(&file, paths.config_dir().join(format!("{PENDING}.bad"))) {
            warn!(%error, "could not set the move that waits aside");
        }
        applied.refused = Some("the file of the move could not be understood".to_owned());
        return applied;
    };
    if let Some(error) = paths.pointer_error() {
        // Saving the choices would write over a file the user may want to repair.
        warn!(
            error,
            "the file of chosen folders is damaged, the move is dropped"
        );
        applied.refused = Some(format!(
            "the file of chosen folders cannot be read: {error}"
        ));
        forget_move(&file);
        return applied;
    }
    let steps = match plan(paths, &changes) {
        Ok(steps) => steps,
        Err(error) => {
            warn!(%error, "the move that waits cannot be made");
            applied.refused = Some(error.to_string());
            forget_move(&file);
            return applied;
        }
    };

    let mut locations = paths.locations().clone();
    for step in steps {
        match carry_out(paths, &step) {
            Ok(()) => {
                locations.set(step.kind, (!step.to_standard).then(|| step.to.clone()));
                if let Err(error) = locations.save(paths.config_dir()) {
                    warn!(%error, kind = step.kind.code(), "the new folder is ready but the choice could not be saved");
                    applied.failed.push((step.kind, error.to_string()));
                    continue;
                }
                info!(kind = step.kind.code(), from = %step.from.display(), to = %step.to.display(), action = ?step.action, "moved saved data");
                match step.action {
                    Action::Copy { .. } => applied.copied.push(step.kind),
                    Action::Adopt { .. } => applied.adopted.push(step.kind),
                    Action::Switch => applied.switched.push(step.kind),
                }
                if matches!(step.action, Action::Copy { .. } | Action::Adopt { .. })
                    && let Some(kept) = set_aside(&step.from, stamp)
                {
                    locations.leftovers.push(kept);
                    if let Err(error) = locations.save(paths.config_dir()) {
                        warn!(%error, "the folder that was set aside could not be recorded");
                    }
                }
            }
            Err(error) => {
                warn!(%error, kind = step.kind.code(), "could not move saved data, it stays where it was");
                applied.failed.push((step.kind, error.to_string()));
            }
        }
    }
    forget_move(&file);
    applied
}

/// Drops the file of a move that has been dealt with. If it cannot be removed it stays, and the
/// move is read again at the next start: worth a warning.
fn forget_move(file: &Path) {
    if let Err(error) = remove_file(file) {
        warn!(%error, "the file of the move could not be removed");
    }
}

/// Puts the files of a step in the new folder and checks them. The choice is not saved here.
fn carry_out(paths: &AppPaths, step: &Step) -> Result<()> {
    let write_error = |source| ConfigError::Write {
        path: step.to.clone(),
        source,
    };
    fs::create_dir_all(&step.to).map_err(write_error)?;
    if matches!(step.action, Action::Copy { .. }) {
        let mut made = Vec::new();
        let copied = copy_tree(&step.from, &step.to, &mut made)
            .map_err(write_error)
            .and_then(|()| verify(&step.from, &step.to));
        if let Err(error) = copied {
            // What was made here is ours (the folder was empty): take it back.
            for file in made.iter().rev() {
                remove_stray(file);
            }
            return Err(error);
        }
    }
    if !step.to_standard {
        locations::write_marker(&step.to, step.kind, paths.mode())?;
    }
    Ok(())
}

/// Renames `dir` to `<name>.moved-<stamp>` beside itself and returns the new name; `None` when
/// there is no such folder or it cannot be renamed (it then stays, under its own name).
fn set_aside(dir: &Path, stamp: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let stamp: String = stamp
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut target = dir.with_file_name(format!("{name}.moved-{stamp}"));
    let mut number = 2;
    while target.exists() {
        target = dir.with_file_name(format!("{name}.moved-{stamp}-{number}"));
        number += 1;
    }
    match retry_when_busy(|| fs::rename(dir, &target)) {
        Ok(()) => Some(target),
        Err(error) => {
            warn!(%error, dir = %dir.display(), "could not set the old folder aside, it stays where it is");
            None
        }
    }
}
