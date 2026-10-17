//! A folder the data can live in: whether it can be used, the marker that says which build and
//! which kind it is for, and which folder is used when a chosen one cannot be.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

use super::{Kind, Locations, MARKER};
use crate::error::{ConfigError, Result};
use crate::fs_util::{atomic_write, read_text};
use wyck_platform::build_info::BuildMode;

const MARKER_FORMAT: &str = "wyck-location";
/// The layout of the marker file.
const MARKER_VERSION: u32 = 1;

/// Why a folder cannot be used for a kind of data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The path is not absolute: it would mean a different place each time.
    NotAbsolute,
    /// There is nothing there. A synced or removable drive may not be mounted.
    Missing,
    /// It is a file.
    NotAFolder,
    /// A file cannot be made in it.
    NotWritable,
    /// It holds other things and was not made by Wyck: data would be mixed with them.
    NotEmpty,
    /// It was made by the other build (`found` is `production` or `development`).
    OtherBuild { found: String },
    /// It was made for another kind of data (`found` is the code of that kind).
    OtherKind { found: String },
    /// It could not be looked into.
    Unreadable,
}

/// A folder that cannot be used, with the kind it was for and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub kind: Kind,
    pub path: PathBuf,
    pub reason: Reason,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        match &self.reason {
            Reason::NotAbsolute => write!(f, "`{path}` is not a full path"),
            Reason::Missing => write!(f, "`{path}` was not found"),
            Reason::NotAFolder => write!(f, "`{path}` is a file, not a folder"),
            Reason::NotWritable => write!(f, "Wyck cannot write in `{path}`"),
            Reason::NotEmpty => write!(
                f,
                "`{path}` holds other files and was not made by Wyck, so it would be mixed with them"
            ),
            Reason::OtherBuild { found } => {
                let (this, other) = if found == "production" {
                    ("a development build", "the installed app")
                } else {
                    ("the installed app", "a development build")
                };
                write!(f, "`{path}` belongs to {other}, and this is {this}")
            }
            Reason::OtherKind { found } => write!(
                f,
                "`{path}` holds `{found}` data, not `{}`",
                self.kind.code()
            ),
            Reason::Unreadable => write!(f, "`{path}` could not be looked into"),
        }
    }
}

impl std::error::Error for Problem {}

/// What a folder that can be used holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checked {
    /// Whether it already holds data: it was made by Wyck for this kind, or it is not empty.
    pub has_data: bool,
}

/// How strict [`check`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// A folder already in use. It must be there.
    Existing,
    /// A folder just chosen to receive data. It may not exist yet (what is above it must), and if
    /// it does it must be empty or made by Wyck.
    Chosen,
    /// A folder the user already keeps their own files in, such as the scripts they wrote, to be
    /// read in place. As [`Self::Chosen`], but other files in it are theirs and welcome.
    Own,
}

/// The content of a marker.
#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    format: String,
    #[serde(default)]
    schema_version: u32,
    kind: String,
    build: String,
}

/// Writes the marker that says this folder is for `kind`, made by this build.
///
/// # Errors
///
/// [`ConfigError::Serialize`] and [`ConfigError::Write`].
pub(crate) fn write_marker(dir: &Path, kind: Kind, mode: BuildMode) -> Result<()> {
    let marker = Marker {
        format: MARKER_FORMAT.to_owned(),
        schema_version: MARKER_VERSION,
        kind: kind.code().to_owned(),
        build: mode.code().to_owned(),
    };
    let text = toml::to_string_pretty(&marker).map_err(ConfigError::Serialize)?;
    atomic_write(&dir.join(MARKER), text.as_bytes())
}

/// The marker of `dir`: `None` when there is none (or it is not a marker of Wyck).
fn read_marker(dir: &Path) -> std::result::Result<Option<Marker>, ()> {
    match read_text(&dir.join(MARKER)) {
        Ok(text) => match toml::from_str::<Marker>(&text) {
            Ok(marker) if marker.format == MARKER_FORMAT => Ok(Some(marker)),
            _ => Ok(None),
        },
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

/// Whether anything but the marker and the leftovers of a write is in `dir`.
pub(crate) fn holds_files(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name != MARKER && !name.contains(".tmp-") && !name.starts_with(".wyck-probe")
        })
    })
}

/// Whether a file can be made in `dir`.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".wyck-probe-{}", std::process::id()));
    let made = fs::write(&probe, b"").is_ok();
    crate::fs_util::remove_stray(&probe);
    made
}

/// Whether `dir` can hold the data of `kind` for this build.
///
/// # Errors
///
/// The [`Problem`] that rules it out.
pub fn check(
    dir: &Path,
    kind: Kind,
    mode: BuildMode,
    how: Use,
) -> std::result::Result<Checked, Problem> {
    let fail = |reason| Problem {
        kind,
        path: dir.to_path_buf(),
        reason,
    };
    if !dir.is_absolute() {
        return Err(fail(Reason::NotAbsolute));
    }
    match fs::metadata(dir) {
        Ok(metadata) if !metadata.is_dir() => Err(fail(Reason::NotAFolder)),
        Ok(_) => {
            let marker = read_marker(dir).map_err(|()| fail(Reason::Unreadable))?;
            let has_data = match &marker {
                Some(marker) => {
                    if marker.build != mode.code() {
                        return Err(fail(Reason::OtherBuild {
                            found: marker.build.clone(),
                        }));
                    }
                    if marker.kind != kind.code() {
                        return Err(fail(Reason::OtherKind {
                            found: marker.kind.clone(),
                        }));
                    }
                    true
                }
                None => holds_files(dir),
            };
            if marker.is_none() && has_data && how == Use::Chosen {
                return Err(fail(Reason::NotEmpty));
            }
            if !writable(dir) {
                return Err(fail(Reason::NotWritable));
            }
            Ok(Checked { has_data })
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => match how {
            Use::Existing => Err(fail(Reason::Missing)),
            Use::Chosen | Use::Own => {
                // A folder to be made, with whatever is missing above it: the nearest folder that
                // exists has to be one, and open to writing. A drive or a service that is not there
                // has no such folder, and nothing is made on it.
                match dir.ancestors().skip(1).find(|above| above.exists()) {
                    None => Err(fail(Reason::Missing)),
                    Some(above) if !above.is_dir() => Err(fail(Reason::NotAFolder)),
                    Some(above) if !writable(above) => Err(fail(Reason::NotWritable)),
                    Some(_) => Ok(Checked { has_data: false }),
                }
            }
        },
        // Unix says so when a part of the path is a file; Windows says "not found".
        Err(source) if source.kind() == std::io::ErrorKind::NotADirectory => {
            Err(fail(Reason::NotAFolder))
        }
        Err(_) => Err(fail(Reason::Unreadable)),
    }
}

/// The folder to use when the user picked `picked` for `kind`: itself when it is empty or already
/// Wyck's, and a folder named after the kind inside it when it holds other things, so that nothing
/// is ever mixed with files that are not ours.
#[must_use]
pub fn target_in(picked: &Path, kind: Kind, mode: BuildMode) -> PathBuf {
    match check(picked, kind, mode, Use::Chosen) {
        Err(Problem {
            reason: Reason::NotEmpty,
            ..
        }) => picked.join(kind.code()),
        _ => picked.to_path_buf(),
    }
}

/// The folder for `kind` that `locations` names if it can be used, the standard one otherwise;
/// a folder that cannot be used is added to `problems`.
pub(crate) fn resolve(
    locations: &Locations,
    kind: Kind,
    standard: &Path,
    mode: BuildMode,
    problems: &mut Vec<Problem>,
) -> PathBuf {
    let Some(chosen) = locations.get(kind) else {
        return standard.to_path_buf();
    };
    match check(chosen, kind, mode, Use::Existing) {
        Ok(_) => chosen.to_path_buf(),
        Err(problem) => {
            warn!(%problem, kind = kind.code(), "a chosen folder cannot be used, the standard one is used for now");
            problems.push(problem);
            standard.to_path_buf()
        }
    }
}
