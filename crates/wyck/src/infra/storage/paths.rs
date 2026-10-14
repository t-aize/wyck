//! OS-standard config/data directory resolution.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use tracing::{debug, warn};

use crate::infra::platform::build_info::BuildMode;
use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::identity::application_name;
use crate::infra::storage::locations::{self, Kind, Locations, Problem};

/// The environment variable that moves the config directory (and the data directory too, unless
/// [`DATA_DIR_ENV`] says otherwise): for a portable install, a test, or a second profile of the
/// app on the same machine.
pub const CONFIG_DIR_ENV: &str = "WYCK_CONFIG_DIR";
/// The environment variable that moves the data directory alone.
pub const DATA_DIR_ENV: &str = "WYCK_DATA_DIR";

const QUALIFIER: &str = "sh";
const ORGANIZATION: &str = "wyck";
/// The name of the folder of the logs, inside the data folder when that is moved by hand.
const LOGS: &str = "logs";

/// Where the system expects the logs of a user's app. Not the folders of the settings: those can
/// be roaming (they follow the user from one machine to another) and a log is about one machine.
fn standard_logs_dir(dirs: &ProjectDirs, mode: BuildMode) -> PathBuf {
    if cfg!(target_os = "macos")
        && let Some(base) = directories::BaseDirs::new()
    {
        return base.home_dir().join("Library").join("Logs").join(format!(
            "{QUALIFIER}.{ORGANIZATION}.{}",
            application_name(mode)
        ));
    }
    dirs.state_dir()
        .unwrap_or_else(|| dirs.data_local_dir())
        .join(LOGS)
}

/// The directories this crate reads and writes: a config directory for
/// [`crate::infra::storage::AppConfig`]'s TOML file, and a data directory for anything a
/// [`crate::infra::storage::secret::SecretStore`] backend needs to persist on disk (currently only
/// [`crate::infra::storage::secret::EncryptedFileSecretStore`]: the default
/// [`crate::infra::storage::secret::KeyringSecretStore`] backend stores nothing here, the OS credential
/// store owns that).
///
/// On a real install, resolve via [`Self::discover`], which asks the OS for its
/// standard per-user application-data locations (under `%APPDATA%\wyck` on Windows,
/// `~/Library/Application Support/sh.wyck.wyck` on macOS, `~/.config/wyck` on Linux, via
/// the `directories` crate). A development (debug) build uses `wyck-dev` where those say `wyck`,
/// so it never touches the data of the installed app. For tests, or a caller that wants a
/// portable/overridden location, [`Self::at`] points both directories at an arbitrary path
/// instead.
///
/// Five kinds of data can be kept elsewhere than in their standard folder, each on its own (see
/// [`crate::infra::storage::locations`]): [`Self::dir`] is where one is read from and written to,
/// [`Self::standard_dir`] where it would be by default. The config folder and the data folder
/// (credentials) never move.
#[derive(Debug, Clone)]
pub struct AppPaths {
    mode: BuildMode,
    config_dir: PathBuf,
    data_dir: PathBuf,
    /// Where each kind is when the user chose nothing.
    standard: Dirs,
    /// Where each kind is now.
    current: Dirs,
    locations: Locations,
    problems: Vec<Problem>,
    /// Why the file of choices could not be read, when that is so.
    pointer_error: Option<String>,
}

/// One folder per movable kind.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Dirs {
    settings: PathBuf,
    accounts: PathBuf,
    indicators: PathBuf,
    backups: PathBuf,
    logs: PathBuf,
}

impl Dirs {
    fn get(&self, kind: Kind) -> &Path {
        match kind {
            Kind::Settings => &self.settings,
            Kind::Accounts => &self.accounts,
            Kind::Indicators => &self.indicators,
            Kind::Backups => &self.backups,
            Kind::Logs => &self.logs,
        }
    }

    fn set(&mut self, kind: Kind, dir: PathBuf) {
        match kind {
            Kind::Settings => self.settings = dir,
            Kind::Accounts => self.accounts = dir,
            Kind::Indicators => self.indicators = dir,
            Kind::Backups => self.backups = dir,
            Kind::Logs => self.logs = dir,
        }
    }
}

impl AppPaths {
    /// The paths of a build whose standard folders are the ones given, with nothing moved.
    fn standard_in(mode: BuildMode, config_dir: PathBuf, data_dir: PathBuf, logs: PathBuf) -> Self {
        let standard = Dirs {
            settings: config_dir.join("state"),
            accounts: config_dir.join("scopes"),
            indicators: config_dir.join("indicators"),
            backups: config_dir.join("backups"),
            logs,
        };
        Self {
            mode,
            config_dir,
            data_dir,
            current: standard.clone(),
            standard,
            locations: Locations::default(),
            problems: Vec::new(),
            pointer_error: None,
        }
    }

    /// These paths with the folders `locations` names put in place: each one that can be used
    /// for its kind, and the standard folder for those that cannot (they are listed in
    /// [`Self::problems`]).
    #[must_use]
    pub fn with_locations(mut self, locations: Locations) -> Self {
        self.problems.clear();
        for kind in Kind::ALL {
            let dir = locations::resolve(
                &locations,
                kind,
                self.standard.get(kind),
                self.mode,
                &mut self.problems,
            );
            self.current.set(kind, dir);
        }
        self.locations = locations;
        self
    }

    /// The folders the user chose, as saved in `locations.toml` (see
    /// [`crate::infra::storage::locations`]).
    #[must_use]
    pub fn locations(&self) -> &Locations {
        &self.locations
    }

    /// The folders that were chosen and cannot be used right now, so the standard ones are used
    /// until they can.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// Why `locations.toml` could not be read, when it could not: every kind is then in its
    /// standard folder and the file is left as it is.
    #[must_use]
    pub fn pointer_error(&self) -> Option<&str> {
        self.pointer_error.as_deref()
    }

    /// The build these paths are for.
    #[must_use]
    pub fn mode(&self) -> BuildMode {
        self.mode
    }

    /// The folder `kind` is read from and written to now.
    #[must_use]
    pub fn dir(&self, kind: Kind) -> PathBuf {
        self.current.get(kind).to_path_buf()
    }

    /// The folder `kind` is in when nothing was chosen.
    #[must_use]
    pub fn standard_dir(&self, kind: Kind) -> &Path {
        self.standard.get(kind)
    }

    /// Whether `kind` is somewhere else than its standard folder.
    #[must_use]
    pub fn is_custom(&self, kind: Kind) -> bool {
        self.current.get(kind) != self.standard.get(kind)
    }

    /// Resolves the config and data directories for `wyck`: the ones the environment names
    /// ([`CONFIG_DIR_ENV`], [`DATA_DIR_ENV`]) if it names any, the OS-standard ones otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::NoProjectDirs`] if the environment names nothing and the OS
    /// reports no home directory for the current user (e.g. running as a system service account
    /// on some platforms): see [`directories::ProjectDirs::from`].
    pub fn discover() -> Result<Self> {
        Self::discover_with(|name| std::env::var_os(name))
    }

    /// [`Self::discover`], reading the environment through `var` (so a test does not have to
    /// change the real one). A variable that is empty counts as not set.
    ///
    /// # Errors
    ///
    /// As [`Self::discover`].
    pub fn discover_with(var: impl Fn(&str) -> Option<OsString>) -> Result<Self> {
        Self::discover_for(BuildMode::CURRENT, var)
    }

    /// [`Self::discover_with`] for a given build mode: the standard folders of a development
    /// build are not those of the installed app (see [`crate::infra::storage::identity`]).
    ///
    /// # Errors
    ///
    /// As [`Self::discover`].
    pub fn discover_for(mode: BuildMode, var: impl Fn(&str) -> Option<OsString>) -> Result<Self> {
        let set = |name: &str| {
            var(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let (config_override, data_override) = (set(CONFIG_DIR_ENV), set(DATA_DIR_ENV));
        let mut paths = match (config_override, data_override) {
            (Some(config_dir), data_dir) => {
                let data_dir = data_dir.unwrap_or_else(|| config_dir.clone());
                let logs = data_dir.join(LOGS);
                Self::standard_in(mode, config_dir, data_dir, logs)
            }
            (None, data_dir) => {
                let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, application_name(mode))
                    .ok_or(ConfigError::NoProjectDirs)?;
                let logs = data_dir
                    .as_ref()
                    .map_or_else(|| standard_logs_dir(&dirs, mode), |dir| dir.join(LOGS));
                Self::standard_in(
                    mode,
                    dirs.config_dir().to_path_buf(),
                    data_dir.unwrap_or_else(|| dirs.data_dir().to_path_buf()),
                    logs,
                )
            }
        };
        // The folders the user moved: the pointer is in the config folder, which never moves.
        match Locations::load(&paths.config_dir) {
            Ok(locations) => paths = paths.with_locations(locations),
            Err(error) => {
                warn!(%error, "the file of chosen folders could not be read, the standard ones are used");
                paths.pointer_error = Some(error.to_string());
            }
        }
        debug!(
            config_dir = %paths.config_dir.display(),
            data_dir = %paths.data_dir.display(),
            logs_dir = %paths.logs_dir().display(),
            moved = !paths.locations.is_empty(),
            "resolved the config/data directories"
        );
        Ok(paths)
    }

    /// Points both the config and data directories at `dir`. Intended for tests and for
    /// callers that want a portable install (e.g. config alongside the executable)
    /// instead of the OS-standard per-user location. Nothing is moved: the file of chosen
    /// folders is not read (see [`Self::with_locations`]).
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self::at_for(BuildMode::CURRENT, dir)
    }

    /// [`Self::at`] for a given build mode, which decides what a moved folder may be (see
    /// [`crate::infra::storage::locations`]).
    pub fn at_for(mode: BuildMode, dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let logs = dir.join(LOGS);
        Self::standard_in(mode, dir.clone(), dir, logs)
    }

    /// The directory [`crate::infra::storage::AppConfig`]'s TOML file lives in.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// The full path to the config TOML file.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The directory an on-disk [`crate::infra::storage::secret::SecretStore`] backend may persist
    /// files in.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The subdirectory [`crate::infra::storage::secret::EncryptedFileSecretStore`] persists its
    /// per-secret envelope files in.
    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }

    /// The folder of the log files and of the reports written when the app panics (see
    /// [`crate::infra::platform::logging`]). Where the system keeps logs, apart from the settings
    /// so that a backup or a reset never carries or removes them: `%LOCALAPPDATA%` on Windows,
    /// `~/Library/Logs` on macOS, `$XDG_STATE_HOME` (`~/.local/state`) on Linux. With
    /// [`CONFIG_DIR_ENV`] or [`DATA_DIR_ENV`] set it is the `logs` folder inside the data folder.
    pub fn logs_dir(&self) -> &Path {
        &self.current.logs
    }

    /// The folder of the documents shared by every account (see [`crate::infra::storage::DocumentStore::global`]).
    pub fn state_dir(&self) -> PathBuf {
        self.dir(Kind::Settings)
    }

    /// The folder that holds one folder of documents per scope (see
    /// [`crate::infra::storage::DocumentStore::scoped`]).
    pub fn scopes_dir(&self) -> PathBuf {
        self.dir(Kind::Accounts)
    }

    /// The folder of the backups the app keeps (see [`crate::infra::storage::backup::store::BackupStore`]): the automatic
    /// ones, and a copy of what an import or a reset replaced.
    pub fn backups_dir(&self) -> PathBuf {
        self.dir(Kind::Backups)
    }

    /// The folder the scripted indicators are read from unless the user chose another.
    pub fn indicators_dir(&self) -> PathBuf {
        self.dir(Kind::Indicators)
    }

    /// The documents shared by every account: [`crate::infra::storage::DocumentStore::global`].
    pub fn documents(&self) -> crate::infra::storage::DocumentStore {
        crate::infra::storage::DocumentStore::global(self)
    }

    /// The documents of one scope (an account): [`crate::infra::storage::DocumentStore::scoped`].
    pub fn scope(&self, scope: &str) -> crate::infra::storage::DocumentStore {
        crate::infra::storage::DocumentStore::scoped(self, scope)
    }

    /// The indicator scripts of the default folder: [`crate::infra::storage::scripts::ScriptStore::in_config`].
    pub fn scripts(&self) -> crate::infra::storage::scripts::ScriptStore {
        crate::infra::storage::scripts::ScriptStore::in_config(self)
    }

    /// The copies kept in [`Self::backups_dir`]: [`crate::infra::storage::backup::store::BackupStore::new`].
    pub fn backups(&self) -> crate::infra::storage::backup::store::BackupStore {
        crate::infra::storage::backup::store::BackupStore::new(self)
    }

    /// The user's pictures folder, or the home folder when the system has none: where the app puts
    /// a picture it saves. `None` only when the system knows no home folder either.
    pub fn pictures_dir() -> Option<PathBuf> {
        let dirs = directories::UserDirs::new()?;
        Some(
            dirs.picture_dir()
                .unwrap_or_else(|| dirs.home_dir())
                .to_path_buf(),
        )
    }

    /// The user's documents folder, where a file dialog starts: the temporary folder when the
    /// system has none.
    pub fn documents_dir() -> PathBuf {
        directories::UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_uses_the_same_directory_for_config_and_data() {
        let paths = AppPaths::at("/tmp/example");
        assert_eq!(paths.config_dir(), Path::new("/tmp/example"));
        assert_eq!(paths.data_dir(), Path::new("/tmp/example"));
        assert_eq!(paths.config_file(), Path::new("/tmp/example/config.toml"));
        assert_eq!(paths.secrets_dir(), Path::new("/tmp/example/secrets"));
        assert_eq!(paths.indicators_dir(), Path::new("/tmp/example/indicators"));
        assert_eq!(paths.state_dir(), Path::new("/tmp/example/state"));
        assert_eq!(paths.scopes_dir(), Path::new("/tmp/example/scopes"));
        assert_eq!(paths.backups_dir(), Path::new("/tmp/example/backups"));
        assert_eq!(paths.logs_dir(), Path::new("/tmp/example/logs"));
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn the_environment_moves_the_directories() {
        let paths = AppPaths::discover_with(env(&[(CONFIG_DIR_ENV, "/portable/cfg")])).unwrap();
        assert_eq!(paths.config_dir(), Path::new("/portable/cfg"));
        assert_eq!(
            paths.data_dir(),
            Path::new("/portable/cfg"),
            "data follows config"
        );

        let split = AppPaths::discover_with(env(&[
            (CONFIG_DIR_ENV, "/portable/cfg"),
            (DATA_DIR_ENV, "/portable/data"),
        ]))
        .unwrap();
        assert_eq!(split.config_dir(), Path::new("/portable/cfg"));
        assert_eq!(split.data_dir(), Path::new("/portable/data"));
        assert_eq!(split.secrets_dir(), Path::new("/portable/data/secrets"));
        assert_eq!(
            split.logs_dir(),
            Path::new("/portable/data/logs"),
            "the logs follow the data folder"
        );
        assert_eq!(paths.logs_dir(), Path::new("/portable/cfg/logs"));
    }

    #[test]
    fn an_empty_variable_is_the_same_as_none() {
        let with_empty = AppPaths::discover_with(env(&[(CONFIG_DIR_ENV, "")]));
        let without = AppPaths::discover_with(env(&[]));
        match (with_empty, without) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.config_dir(), b.config_dir());
                assert_eq!(a.data_dir(), b.data_dir());
            }
            (Err(_), Err(_)) => {} // a machine with no home folder
            other => panic!("an empty variable changed the outcome: {other:?}"),
        }
    }

    #[test]
    fn a_development_build_and_the_installed_app_use_different_folders() {
        let production = AppPaths::discover_for(BuildMode::Production, env(&[]));
        let development = AppPaths::discover_for(BuildMode::Development, env(&[]));
        let (Ok(production), Ok(development)) = (production, development) else {
            return; // a machine with no home folder
        };
        assert_ne!(production.config_dir(), development.config_dir());
        assert_ne!(production.data_dir(), development.data_dir());
        assert!(
            development
                .config_dir()
                .to_string_lossy()
                .contains("wyck-dev")
        );
        assert!(
            !production
                .config_dir()
                .to_string_lossy()
                .contains("wyck-dev")
        );
        // The folders hang off the config folder, so they are apart too.
        assert_ne!(production.state_dir(), development.state_dir());
        assert_ne!(production.backups_dir(), development.backups_dir());
        assert_ne!(production.logs_dir(), development.logs_dir());
        assert!(
            development
                .logs_dir()
                .to_string_lossy()
                .contains("wyck-dev")
        );
    }

    #[test]
    fn a_chosen_folder_replaces_the_standard_one_for_its_kind_only() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("config");
        let chosen = temp.path().join("cloud").join("settings");
        std::fs::create_dir_all(&chosen).unwrap();
        let mut choices = Locations::default();
        choices.set(Kind::Settings, Some(chosen.clone()));

        let paths = AppPaths::at(&base).with_locations(choices);

        assert_eq!(paths.state_dir(), chosen);
        assert_eq!(paths.dir(Kind::Settings), chosen);
        assert!(paths.is_custom(Kind::Settings) && !paths.is_custom(Kind::Backups));
        assert_eq!(paths.scopes_dir(), base.join("scopes"));
        assert_eq!(paths.backups_dir(), base.join("backups"));
        assert_eq!(paths.standard_dir(Kind::Settings), base.join("state"));
        assert_eq!(paths.documents().dir(), chosen, "the documents follow");
        assert!(paths.problems().is_empty());
        assert_eq!(paths.config_dir(), base, "the config folder never moves");
    }

    #[test]
    fn a_chosen_folder_that_is_not_there_is_a_problem_and_the_standard_one_is_used() {
        let temp = tempfile::tempdir().unwrap();
        let mut choices = Locations::default();
        choices.set(Kind::Backups, Some(temp.path().join("unplugged")));

        let paths = AppPaths::at(temp.path().join("config")).with_locations(choices);

        assert_eq!(
            paths.backups_dir(),
            temp.path().join("config").join("backups")
        );
        assert!(!paths.is_custom(Kind::Backups));
        assert_eq!(paths.problems().len(), 1);
        assert_eq!(paths.problems()[0].kind, Kind::Backups);
        assert!(
            paths.locations().get(Kind::Backups).is_some(),
            "the choice is kept for when it is back"
        );
    }

    #[test]
    fn the_choices_are_read_from_the_config_folder_the_environment_names() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        let chosen = temp.path().join("elsewhere");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&chosen).unwrap();
        let mut choices = Locations::default();
        choices.set(Kind::Indicators, Some(chosen.clone()));
        choices.save(&config).unwrap();
        let config_text = config.to_str().unwrap().to_owned();

        let paths = AppPaths::discover_for(BuildMode::Production, |name| {
            (name == CONFIG_DIR_ENV).then(|| OsString::from(&config_text))
        })
        .unwrap();

        assert_eq!(paths.indicators_dir(), chosen);
        assert!(paths.pointer_error().is_none());

        // A file that cannot be read leaves every folder in its place, and says why.
        std::fs::write(config.join(locations::FILE), "settings = [[[").unwrap();
        let damaged = AppPaths::discover_for(BuildMode::Production, |name| {
            (name == CONFIG_DIR_ENV).then(|| OsString::from(&config_text))
        })
        .unwrap();
        assert_eq!(damaged.indicators_dir(), config.join("indicators"));
        assert!(damaged.pointer_error().is_some());
        assert_eq!(
            std::fs::read_to_string(config.join(locations::FILE)).unwrap(),
            "settings = [[[",
            "the damaged file is left for the user to repair"
        );
    }

    #[test]
    fn the_logs_live_where_the_system_keeps_them_not_with_the_settings() {
        let Ok(paths) = AppPaths::discover_for(BuildMode::Production, env(&[])) else {
            return; // a machine with no home folder
        };
        assert!(
            !paths.logs_dir().starts_with(paths.config_dir()),
            "a backup or a reset of the settings must not reach the logs"
        );
        assert!(paths.logs_dir().ends_with("logs") || cfg!(target_os = "macos"));
    }

    #[test]
    fn the_environment_wins_over_the_build_mode() {
        let var = env(&[(CONFIG_DIR_ENV, "/real/data")]);
        let paths = AppPaths::discover_for(BuildMode::Development, var).unwrap();
        assert_eq!(paths.config_dir(), Path::new("/real/data"));
    }

    #[test]
    fn the_os_directories_are_used_when_nothing_is_set() {
        if let Ok(paths) = AppPaths::discover_with(env(&[])) {
            assert!(
                paths.config_dir().ends_with("wyck")
                    || paths.config_dir().to_string_lossy().contains("wyck")
            );
        }
    }
}
