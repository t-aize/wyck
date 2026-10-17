//! Where each kind of saved data lives, and the file that says so.
//!
//! By default everything is in the standard folders of the system (see [`AppPaths`]). A user can
//! keep some of it elsewhere, such as a folder that `OneDrive`, Dropbox or another service syncs
//! between computers, or a data disk. Five kinds can move, each on its own ([`Kind`]):
//!
//! * the settings (layout, shortcuts, favorites, everything the look and the charts keep),
//! * the data of the accounts (drawings, alerts, watchlists),
//! * the indicator scripts,
//! * the backups,
//! * the logs.
//!
//! What never moves: the credentials (the system keyring, or the encrypted files of the data folder)
//! and `config.toml`, which lists the signed-in accounts and ties each to its credentials on this
//! computer. A synced folder holds nothing that signs in.
//!
//! # The pointer
//!
//! The choice is saved in `locations.toml`, in the standard config folder of the build (the one
//! [`AppPaths`] starts from, or where `WYCK_CONFIG_DIR` says). That file is never moved: something
//! has to be at a known place for the rest to be found, as in the profile list of a browser. A
//! development build and the installed app have different standard folders, so their pointers are
//! apart.
//!
//! # Dev and production
//!
//! Every moved folder holds a small marker, `wyck-location.toml`, that says which build made it and
//! which kind of data it is for. A development build refuses a folder marked for the installed app
//! and the other way round, and a folder of one kind is never taken for another. This is the same
//! rule as the keyring service and the default folders: testing cannot reach real data.
//!
//! # A folder that is not there
//!
//! A synced folder can be missing at start (the service has not mounted it, the drive is out).
//! The pointer is kept, that kind is read from and written to its standard folder for this run,
//! and [`AppPaths::problems`] says why, so the app can tell the user. Nothing is created in a place
//! that was not found, and nothing is written over a folder made by the other build.
//!
//! [`AppPaths`]: crate::AppPaths
//! [`AppPaths::problems`]: crate::AppPaths::problems

mod cloud;
mod folder;
mod kind;
mod pointer;
#[cfg(test)]
mod tests;

pub use self::cloud::{
    CloudFolder, cloud_folder_name, cloud_folders, cloud_layout, conflict_copies,
};
pub use self::folder::{Checked, Problem, Reason, Use, check, target_in};
pub(crate) use self::folder::{holds_files, resolve, write_marker};
pub use self::kind::Kind;
pub use self::pointer::Locations;

/// The name of the pointer, in the config folder.
pub const FILE: &str = "locations.toml";
/// The name of the marker, inside a moved folder.
pub const MARKER: &str = "wyck-location.toml";
