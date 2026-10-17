//! Moving a kind of data to another folder, safely.
//!
//! The user asks for a move in the settings ([`stage`]); nothing is moved while the app runs,
//! because the documents are held in memory and saved to the folder they were read from. The next
//! start applies it ([`apply_pending`]), before anything reads a document, as a restore does.
//!
//! # What happens to the files
//!
//! * **The new folder is empty** (or does not exist): the files are copied there one by one, each
//!   through a temporary name, then every file is read back and compared. Only when all of them
//!   match is the new folder marked as Wyck's (see [`crate::locations`]) and the
//!   choice written in `locations.toml`. A copy that is cut short never looks like a folder with
//!   data in it, because the marker is the last thing written.
//! * **The new folder already holds data of Wyck** (a second computer joining a cloud folder the
//!   first one filled): it is used as it is, and nothing from this computer is copied over it.
//! * **The old folder is kept.** After a copy or a switch to data that was already there, the old
//!   folder is renamed `<name>.moved-<date>` beside where it was and listed in `locations.toml`,
//!   so nothing is lost if the user changes their mind; [`delete_leftover`] removes one on request.
//! * **The logs are not carried**: a log says what happened on one computer, in one place.
//!
//! # What is refused
//!
//! A folder that is the folder of another kind or inside it (one kind of data must not be mixed with
//! another), one that holds the config or the credentials or is inside the credentials folder (a
//! synced folder holds nothing that signs in), and every folder [`crate::locations::check`] refuses: one
//! that is not Wyck's and is not empty, one made by the other build, one that is not there.

mod applying;
mod copying;
mod left_behind;
mod planning;
mod staging;
#[cfg(test)]
mod tests;
mod tree;

pub use self::applying::{Applied, apply_pending};
pub use self::left_behind::{Leftover, delete_leftover, leftovers};
pub use self::planning::plan;
pub use self::staging::{cancel, pending, stage};

use std::path::PathBuf;

use crate::error::ConfigError;
use crate::locations::{Kind, Problem};

/// The name of the file a move waits in, in the config folder.
pub const PENDING: &str = "pending-relocation.toml";

/// Moves `kind` to `to`, or with `None` back to its standard folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: Kind,
    pub to: Option<PathBuf>,
}

/// What a move does to the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The new folder is taken as it is: there is nothing to carry.
    Switch,
    /// The files of the old folder are copied to the new one.
    Copy { files: usize, bytes: u64 },
    /// The new folder already holds data (`files` of them): it is used, and the files of this
    /// computer are kept aside.
    Adopt { files: usize },
}

/// One kind to move, with where from, where to and what happens to its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub kind: Kind,
    pub from: PathBuf,
    pub to: PathBuf,
    /// Whether `to` is the standard folder, so the choice is forgotten rather than saved.
    pub to_standard: bool,
    pub action: Action,
}

/// Why a move cannot be made.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("{0}")]
    Folder(#[from] Problem),
    #[error("{a} and {b} cannot be kept in the same folder, or one inside the other")]
    Mixed { a: Kind, b: Kind },
    #[error(
        "`{0}` holds the sign-in or the list of accounts of this computer, and nothing like that goes into a folder you move"
    )]
    Sensitive(PathBuf),
    #[error("`{0}` cannot be saved: its name is not valid text")]
    NotText(PathBuf),
    #[error(transparent)]
    Storage(#[from] ConfigError),
}
