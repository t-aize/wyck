//! The pointer: `locations.toml`, the file that says which folder each kind of data lives in.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{FILE, Kind};
use crate::error::{ConfigError, Result};
use crate::fs_util::{atomic_write, read_text};
use crate::migrate::{self, Outcome, Plan};

const SCHEMA_VERSION: u32 = 1;

/// How an older `locations.toml` is brought to the current layout.
const PLAN: Plan = Plan {
    document: "locations",
    current: SCHEMA_VERSION,
    steps: &[],
};

fn version() -> u32 {
    SCHEMA_VERSION
}

/// The folders the user chose. A kind with none is in its standard folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locations {
    #[serde(default = "version")]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounts: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicators: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backups: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<PathBuf>,
    /// Folders a move left behind (the old place, renamed), kept until the user deletes them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leftovers: Vec<PathBuf>,
}

impl Default for Locations {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: None,
            accounts: None,
            indicators: None,
            backups: None,
            logs: None,
            leftovers: Vec::new(),
        }
    }
}

impl Locations {
    /// The folder chosen for `kind`.
    #[must_use]
    pub(crate) fn get(&self, kind: Kind) -> Option<&Path> {
        match kind {
            Kind::Settings => &self.settings,
            Kind::Accounts => &self.accounts,
            Kind::Indicators => &self.indicators,
            Kind::Backups => &self.backups,
            Kind::Logs => &self.logs,
        }
        .as_deref()
    }

    /// Chooses the folder of `kind`, or with `None` goes back to the standard one.
    pub fn set(&mut self, kind: Kind, dir: Option<PathBuf>) {
        match kind {
            Kind::Settings => self.settings = dir,
            Kind::Accounts => self.accounts = dir,
            Kind::Indicators => self.indicators = dir,
            Kind::Backups => self.backups = dir,
            Kind::Logs => self.logs = dir,
        }
    }

    /// Whether nothing is chosen and nothing is left behind.
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Reads the pointer in `config_dir`; nothing there is the default.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Read`], [`ConfigError::Parse`] for a file that is not TOML of this shape,
    /// and [`ConfigError::UnsupportedSchema`] for one written by a newer version. The file is left
    /// as it is.
    pub fn load(config_dir: &Path) -> Result<Self> {
        let path = config_dir.join(FILE);
        if let Outcome::Newer { found, .. } = migrate::upgrade_file(&path, &PLAN)? {
            return Err(ConfigError::UnsupportedSchema {
                path,
                found,
                supported: SCHEMA_VERSION,
            });
        }
        let text = match read_text(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path,
            source: Box::new(source),
        })
    }

    /// Writes the pointer in `config_dir`, atomically.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Serialize`] (a path that is not valid text cannot be written) and
    /// [`ConfigError::Write`].
    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;
        atomic_write(&config_dir.join(FILE), text.as_bytes())
    }
}
