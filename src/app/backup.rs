//! A backup of everything the user made, in one file: the look of the app, the layout and the
//! settings of the charts and their indicators, the favorites, the drawings and their saved looks,
//! the watchlists and the alerts. It is meant to be kept somewhere else, or carried to another
//! machine.
//!
//! The file is a TOML document that holds the text of each saved document as it is on disk, next
//! to the scope it belongs to. Keeping the text, not the parsed values, means a backup is right for
//! whatever the documents hold, and stays readable by a later version that knows more fields.
//!
//! What is never in it: anything that signs in (the secrets are in the system keyring and the
//! encrypted store, not in the documents) and the app's own config file. A backup is safe to put
//! in a cloud folder for that reason.
//!
//! Restoring is done in two steps, so nothing running can write over it. Importing checks the file
//! and puts it aside ([`stage`]); the next start of the app applies it before anything is loaded
//! ([`apply_pending`]), after copying what it replaces into a folder of backups.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The value of `format` in a backup file.
pub const FORMAT: &str = "wyck-backup";
/// The version of the layout of the file. A file of a newer version is refused, not misread.
pub const VERSION: u32 = 1;
/// The most a single document, and a whole backup, can weigh.
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
/// The most documents a backup holds.
const MAX_DOCUMENTS: usize = 500;
/// The name of the file a checked import waits in.
const PENDING: &str = "pending-import.toml";
/// The scope of the documents shared by every account.
pub const GLOBAL: &str = "global";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    /// The version of the app that made it.
    #[serde(default)]
    pub app_version: String,
    /// When it was made, as text.
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub files: Vec<BackupFile>,
    /// The indicators written as scripts. A backup made before they existed has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<BackupScript>,
}
/// One indicator script: its id (its path in the indicators folder, without the extension) and its
/// text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupScript {
    pub id: String,
    pub content: String,
}

/// One saved document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupFile {
    /// `global`, or `scope:` and the name of an account's scope.
    pub scope: String,
    /// The name of the document, without its extension.
    pub name: String,
    /// Its text.
    pub content: String,
}

#[derive(Debug)]
pub enum BackupError {
    Io(io::Error),
    /// Not a backup of this app.
    NotABackup,
    /// Made by a newer version than this one knows.
    TooNew(u32),
    /// A part of the file is not what it should be, with what is wrong.
    Damaged(String),
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::NotABackup => write!(f, "This file is not a wyck backup."),
            Self::TooNew(version) => write!(
                f,
                "This backup was made by a newer version of wyck (format {version})."
            ),
            Self::Damaged(what) => write!(f, "This backup is damaged: {what}."),
        }
    }
}

impl std::error::Error for BackupError {}

impl From<io::Error> for BackupError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Whether `name` is safe to become part of a path: letters, digits, `-`, `_` and `.` inside.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.contains("..")
}

/// Whether `scope` names a place documents can go: `global`, or `scope:` and a safe name.
fn safe_scope(scope: &str) -> bool {
    scope == GLOBAL || scope.strip_prefix("scope:").is_some_and(safe_name)
}

/// The folder of the documents of `scope`, under the config directory.
fn scope_dir(config_dir: &Path, scope: &str) -> PathBuf {
    match scope.strip_prefix("scope:") {
        Some(name) => config_dir.join("scopes").join(name),
        None => config_dir.join("state"),
    }
}

/// The documents of a folder: the `.toml` files in it, by name.
fn documents_in(dir: &Path) -> io::Result<Vec<(String, String)>> {
    let mut found = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(found),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|n| n.to_str()) else {
            continue;
        };
        if !safe_name(name) {
            continue;
        }
        // A document that cannot be read as text is left out, not fatal to the rest.
        if let Ok(content) = fs::read_to_string(&path) {
            found.push((name.to_owned(), content));
        }
    }
    found.sort();
    Ok(found)
}

/// Reads every saved document under `config_dir` into a backup made by `app_version` at `created`.
pub fn collect(config_dir: &Path, app_version: &str, created: &str) -> io::Result<Backup> {
    let mut files = Vec::new();
    for (name, content) in documents_in(&config_dir.join("state"))? {
        files.push(BackupFile {
            scope: GLOBAL.to_owned(),
            name,
            content,
        });
    }
    let scopes = config_dir.join("scopes");
    let mut scope_names: Vec<String> = match fs::read_dir(&scopes) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| safe_name(name))
            .collect(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    scope_names.sort();
    for scope in scope_names {
        for (name, content) in documents_in(&scopes.join(&scope))? {
            files.push(BackupFile {
                scope: format!("scope:{scope}"),
                name,
                content,
            });
        }
    }
    Ok(Backup {
        format: FORMAT.to_owned(),
        version: VERSION,
        app_version: app_version.to_owned(),
        created: created.to_owned(),
        files,
        scripts: Vec::new(),
    })
}

/// [`collect`], with the indicator scripts of `scripts_dir` too.
pub fn collect_with_scripts(
    config_dir: &Path,
    scripts_dir: &Path,
    app_version: &str,
    created: &str,
) -> io::Result<Backup> {
    use crate::app::chart::study::custom::library::Library;
    let mut backup = collect(config_dir, app_version, created)?;
    let mut library = Library::new(scripts_dir);
    library.refresh();
    backup.scripts = library
        .entries()
        .iter()
        .filter(|entry| !entry.source.is_empty())
        .map(|entry| BackupScript {
            id: entry.id.clone(),
            content: entry.source.to_string(),
        })
        .collect();
    Ok(backup)
}

/// The text of a backup.
pub fn to_text(backup: &Backup) -> Result<String, BackupError> {
    toml::to_string_pretty(backup).map_err(|e| BackupError::Damaged(e.to_string()))
}

/// Reads a backup from its text, and checks it: the format, the version, that every document has
/// a safe place and is valid TOML, and that the whole is a size that makes sense.
pub fn parse(text: &str) -> Result<Backup, BackupError> {
    if text.len() > MAX_TOTAL_BYTES + 1024 * 1024 {
        return Err(BackupError::Damaged("it is far too large".to_owned()));
    }
    let backup: Backup = toml::from_str(text).map_err(|_| BackupError::NotABackup)?;
    if backup.format != FORMAT {
        return Err(BackupError::NotABackup);
    }
    if backup.version > VERSION {
        return Err(BackupError::TooNew(backup.version));
    }
    if backup.files.len() > MAX_DOCUMENTS {
        return Err(BackupError::Damaged(
            "it holds too many documents".to_owned(),
        ));
    }
    check_scripts(&backup)?;
    let mut total = 0;
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for file in &backup.files {
        if !safe_scope(&file.scope) || !safe_name(&file.name) {
            return Err(BackupError::Damaged(format!(
                "\"{}\" in \"{}\" has no safe place to go",
                file.name, file.scope
            )));
        }
        if seen.contains(&(file.scope.as_str(), file.name.as_str())) {
            return Err(BackupError::Damaged(format!(
                "\"{}\" is in it twice",
                file.name
            )));
        }
        seen.push((&file.scope, &file.name));
        if file.content.len() > MAX_DOCUMENT_BYTES {
            return Err(BackupError::Damaged(format!(
                "\"{}\" is too large",
                file.name
            )));
        }
        total += file.content.len();
        if toml::from_str::<toml::Table>(&file.content).is_err() {
            return Err(BackupError::Damaged(format!(
                "\"{}\" is not a valid document",
                file.name
            )));
        }
    }
    if total > MAX_TOTAL_BYTES {
        return Err(BackupError::Damaged("it is far too large".to_owned()));
    }
    Ok(backup)
}

/// Checks the scripts of a backup: each has a safe id inside the indicators folder, a size that makes
/// sense, and is there once.
fn check_scripts(backup: &Backup) -> Result<(), BackupError> {
    use crate::app::chart::study::custom::library::{MAX_FILE_BYTES, MAX_SCRIPTS, clean_id};
    if backup.scripts.len() > MAX_SCRIPTS {
        return Err(BackupError::Damaged(
            "it holds too many indicator scripts".to_owned(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for script in &backup.scripts {
        if clean_id(&script.id).ok().as_deref() != Some(script.id.as_str()) {
            return Err(BackupError::Damaged(format!(
                "the script \"{}\" has no safe place to go",
                script.id
            )));
        }
        if !seen.insert(script.id.to_lowercase()) {
            return Err(BackupError::Damaged(format!(
                "the script \"{}\" is in it twice",
                script.id
            )));
        }
        if script.content.len() as u64 > MAX_FILE_BYTES {
            return Err(BackupError::Damaged(format!(
                "the script \"{}\" is too large",
                script.id
            )));
        }
    }
    Ok(())
}

/// What a backup holds, in words a person reads: which kinds of things, and how many accounts.
pub fn summary(backup: &Backup) -> Vec<String> {
    let has = |scope_global: bool, name: &str| {
        backup
            .files
            .iter()
            .any(|f| (f.scope == GLOBAL) == scope_global && f.name == name)
    };
    let mut lines = Vec::new();
    if has(true, "appearance") {
        lines.push("Appearance: themes, colors, font".to_owned());
    }
    if has(true, "preferences") {
        lines.push("Layout, charts and their indicators, favorites, ticket settings".to_owned());
    }
    let accounts: Vec<&str> = {
        let mut names: Vec<&str> = backup
            .files
            .iter()
            .filter_map(|f| f.scope.strip_prefix("scope:"))
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    };
    let count = |name: &str| {
        backup
            .files
            .iter()
            .filter(|f| f.scope != GLOBAL && f.name == name)
            .count()
    };
    if count("drawings") > 0 {
        lines.push(format!(
            "Drawings and saved looks ({} account(s))",
            count("drawings")
        ));
    }
    if count("watchlists") > 0 {
        lines.push(format!("Watchlists ({} account(s))", count("watchlists")));
    }
    if count("alerts") > 0 {
        lines.push(format!("Price alerts ({} account(s))", count("alerts")));
    }
    let known = [
        "appearance",
        "preferences",
        "drawings",
        "watchlists",
        "alerts",
    ];
    let others = backup
        .files
        .iter()
        .filter(|f| !known.contains(&f.name.as_str()))
        .count();
    if others > 0 {
        lines.push(format!("{others} other document(s)"));
    }
    if !backup.scripts.is_empty() {
        lines.push(format!("Indicator scripts ({})", backup.scripts.len()));
    }
    if lines.is_empty() {
        lines.push("Nothing: the backup is empty".to_owned());
    } else if !accounts.is_empty() {
        lines.push(format!("Accounts: {}", accounts.join(", ")));
    }
    lines
}

/// Checks the text of a backup and puts it aside, to be applied when the app starts again.
pub fn stage(config_dir: &Path, text: &str) -> Result<Backup, BackupError> {
    let backup = parse(text)?;
    fs::create_dir_all(config_dir)?;
    write_file(&config_dir.join(PENDING), text.as_bytes())?;
    Ok(backup)
}

/// Whether an import waits for the next start.
pub fn pending(config_dir: &Path) -> bool {
    config_dir.join(PENDING).is_file()
}

/// Forgets an import that waits, for one the user changed their mind about.
pub fn cancel_pending(config_dir: &Path) -> io::Result<()> {
    match fs::remove_file(config_dir.join(PENDING)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// What applying an import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// How many documents were written.
    pub written: usize,
    /// Where what they replaced was copied, when they replaced anything.
    pub kept_in: Option<PathBuf>,
}

/// Applies the import that waits, if there is one: copies every document it replaces into
/// `backups/import-<stamp>`, writes the new ones, and removes the import. Call it before anything
/// reads the documents. An import that cannot be read is set aside as `.bad`, not tried again.
pub fn apply_pending(config_dir: &Path, stamp: &str) -> io::Result<Option<Applied>> {
    let path = config_dir.join(PENDING);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let backup = match parse(&text) {
        Ok(backup) => backup,
        Err(_) => {
            let _ = fs::rename(&path, config_dir.join(format!("{PENDING}.bad")));
            return Ok(None);
        }
    };
    let keep = config_dir
        .join("backups")
        .join(format!("import-{}", stamp_name(stamp)));
    let mut kept = false;
    for file in &backup.files {
        let dir = scope_dir(config_dir, &file.scope);
        let target = dir.join(format!("{}.toml", file.name));
        if target.is_file() {
            let relative = target.strip_prefix(config_dir).unwrap_or(&target);
            let copy = keep.join(relative);
            if let Some(parent) = copy.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&target, &copy)?;
            kept = true;
        }
        fs::create_dir_all(&dir)?;
        write_file(&target, file.content.as_bytes())?;
    }
    // The scripts go in the default indicators folder; one that is there already and differs is
    // copied aside first, like the documents.
    for script in &backup.scripts {
        let mut target = config_dir.join("indicators");
        for part in script.id.split('/') {
            target.push(part);
        }
        target.set_extension("rhai");
        if target.is_file() {
            if fs::read_to_string(&target).ok().as_deref() == Some(script.content.as_str()) {
                continue;
            }
            let relative = target.strip_prefix(config_dir).unwrap_or(&target);
            let copy = keep.join(relative);
            if let Some(parent) = copy.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&target, &copy)?;
            kept = true;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_file(&target, script.content.as_bytes())?;
    }
    fs::remove_file(&path)?;
    Ok(Some(Applied {
        written: backup.files.len() + backup.scripts.len(),
        kept_in: kept.then_some(keep),
    }))
}

/// A stamp made safe to be part of a folder name.
fn stamp_name(stamp: &str) -> String {
    let cleaned: String = stamp
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "now".to_owned()
    } else {
        cleaned
    }
}

/// Writes a file whole: a crash halfway leaves the old one, never half of a new one.
fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, relative: &str, content: &str) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn setup() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "state/preferences.toml", "magnet = true\n");
        write(dir.path(), "state/appearance.toml", "mode = \"light\"\n");
        write(dir.path(), "scopes/demo-1/drawings.toml", "next_id = 4\n");
        write(dir.path(), "scopes/demo-1/watchlists.toml", "x = 1\n");
        write(dir.path(), "scopes/live-2/drawings.toml", "next_id = 9\n");
        // Never in a backup: the app's own config, a damaged copy, and what is not a document.
        write(dir.path(), "config.toml", "client_id = \"secret\"\n");
        write(dir.path(), "state/preferences.toml.bad", "junk");
        write(dir.path(), "state/notes.txt", "hello");
        dir
    }

    #[test]
    fn a_backup_holds_every_document_and_nothing_else() {
        let dir = setup();
        let backup = collect(dir.path(), "1.2.3", "today").unwrap();
        let listed: Vec<(String, String)> = backup
            .files
            .iter()
            .map(|f| (f.scope.clone(), f.name.clone()))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("global".to_owned(), "appearance".to_owned()),
                ("global".to_owned(), "preferences".to_owned()),
                ("scope:demo-1".to_owned(), "drawings".to_owned()),
                ("scope:demo-1".to_owned(), "watchlists".to_owned()),
                ("scope:live-2".to_owned(), "drawings".to_owned()),
            ]
        );
        assert!(
            !to_text(&backup).unwrap().contains("secret"),
            "the config file stays out"
        );
        assert_eq!(
            (backup.app_version.as_str(), backup.created.as_str()),
            ("1.2.3", "today")
        );
    }

    #[test]
    fn the_indicator_scripts_travel_in_a_backup_and_come_back() {
        let dir = setup();
        let scripts = tempfile::tempdir().unwrap();
        write(
            scripts.path(),
            "Trend/My average.rhai",
            "plot(\"a\", close);",
        );
        write(scripts.path(), "Other.rhai", "plot(\"b\", open);");
        let backup = collect_with_scripts(dir.path(), scripts.path(), "1", "now").unwrap();
        assert_eq!(backup.scripts.len(), 2);
        let text = to_text(&backup).unwrap();
        let parsed = parse(&text).unwrap();
        assert_eq!(parsed.scripts, backup.scripts);
        assert!(
            summary(&parsed)
                .iter()
                .any(|l| l.contains("Indicator scripts (2)"))
        );

        // Applied on another machine: the scripts land in the default folder.
        let other = tempfile::tempdir().unwrap();
        stage(other.path(), &text).unwrap();
        let applied = apply_pending(other.path(), "s").unwrap().unwrap();
        assert_eq!(applied.written, backup.files.len() + 2);
        let restored = other
            .path()
            .join("indicators")
            .join("Trend")
            .join("My average.rhai");
        assert_eq!(fs::read_to_string(restored).unwrap(), "plot(\"a\", close);");
    }

    #[test]
    fn a_script_that_is_there_and_differs_is_kept_aside_when_a_backup_is_applied() {
        let scripts = tempfile::tempdir().unwrap();
        write(scripts.path(), "Mine.rhai", "plot(\"new\", close);");
        let backup = collect_with_scripts(
            tempfile::tempdir().unwrap().path(),
            scripts.path(),
            "1",
            "now",
        )
        .unwrap();
        let target = tempfile::tempdir().unwrap();
        write(
            target.path(),
            "indicators/Mine.rhai",
            "plot(\"old\", close);",
        );
        stage(target.path(), &to_text(&backup).unwrap()).unwrap();
        let applied = apply_pending(target.path(), "s").unwrap().unwrap();
        assert!(applied.kept_in.is_some());
        assert_eq!(
            fs::read_to_string(target.path().join("indicators/Mine.rhai")).unwrap(),
            "plot(\"new\", close);"
        );
        let kept = applied.kept_in.unwrap().join("indicators/Mine.rhai");
        assert_eq!(fs::read_to_string(kept).unwrap(), "plot(\"old\", close);");
    }

    #[test]
    fn a_script_with_an_unsafe_place_makes_the_backup_refused() {
        for bad in ["../escape", "a/../b", "con", "C:/x", ".hidden"] {
            let backup = Backup {
                format: FORMAT.to_owned(),
                version: VERSION,
                app_version: String::new(),
                created: String::new(),
                files: Vec::new(),
                scripts: vec![BackupScript {
                    id: bad.to_owned(),
                    content: String::new(),
                }],
            };
            assert!(parse(&to_text(&backup).unwrap()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_backup_of_nothing_is_valid_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let backup = collect(dir.path(), "1", "t").unwrap();
        assert!(backup.files.is_empty());
        let back = parse(&to_text(&backup).unwrap()).unwrap();
        assert_eq!(
            summary(&back),
            vec!["Nothing: the backup is empty".to_owned()]
        );
    }

    #[test]
    fn what_is_written_is_read_back_and_summarized() {
        let dir = setup();
        let backup = collect(dir.path(), "1", "t").unwrap();
        let back = parse(&to_text(&backup).unwrap()).unwrap();
        assert_eq!(back, backup);
        let lines = summary(&back);
        assert!(lines.iter().any(|l| l.starts_with("Appearance")));
        assert!(lines.iter().any(|l| l.contains("Layout, charts")));
        assert!(
            lines
                .iter()
                .any(|l| l == "Drawings and saved looks (2 account(s))")
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("demo-1") && l.contains("live-2"))
        );
    }

    #[test]
    fn a_file_that_is_not_a_backup_or_is_broken_is_refused() {
        assert!(matches!(parse("hello"), Err(BackupError::NotABackup)));
        assert!(matches!(
            parse("format = \"other\"\nversion = 1\n"),
            Err(BackupError::NotABackup)
        ));
        assert!(matches!(
            parse("format = \"wyck-backup\"\nversion = 99\n"),
            Err(BackupError::TooNew(99))
        ));
        let broken = "format = \"wyck-backup\"\nversion = 1\n[[files]]\nscope = \"global\"\nname = \"a\"\ncontent = \"not = = toml\"\n";
        assert!(matches!(parse(broken), Err(BackupError::Damaged(_))));
        let twice = "format = \"wyck-backup\"\nversion = 1\n[[files]]\nscope = \"global\"\nname = \"a\"\ncontent = \"\"\n[[files]]\nscope = \"global\"\nname = \"a\"\ncontent = \"\"\n";
        assert!(matches!(parse(twice), Err(BackupError::Damaged(_))));
    }

    #[test]
    fn a_document_cannot_go_outside_the_config_folder() {
        for (scope, name) in [
            ("global", "../evil"),
            ("global", "a/b"),
            ("global", "..\\evil"),
            ("global", ".hidden"),
            ("global", ""),
            ("scope:../x", "a"),
            ("scope:", "a"),
            ("elsewhere", "a"),
            ("scope:a/b", "a"),
        ] {
            let text = format!(
                "format = \"wyck-backup\"\nversion = 1\n[[files]]\nscope = \"{}\"\nname = \"{}\"\ncontent = \"\"\n",
                scope.replace('\\', "\\\\"),
                name.replace('\\', "\\\\")
            );
            assert!(parse(&text).is_err(), "{scope:?} {name:?} was accepted");
        }
    }

    #[test]
    fn an_import_waits_and_is_applied_at_the_next_start_keeping_what_it_replaces() {
        let source = setup();
        let text = to_text(&collect(source.path(), "1", "t").unwrap()).unwrap();

        // Another machine: one document differs, one is new, one is not in the backup.
        let target = tempfile::tempdir().unwrap();
        write(target.path(), "state/preferences.toml", "magnet = false\n");
        write(target.path(), "scopes/other/alerts.toml", "keep = true\n");
        let staged = stage(target.path(), &text).unwrap();
        assert_eq!(staged.files.len(), 5);
        assert!(pending(target.path()));
        assert_eq!(
            fs::read_to_string(target.path().join("state/preferences.toml")).unwrap(),
            "magnet = false\n",
            "nothing is written until the next start"
        );

        let applied = apply_pending(target.path(), "2026-09-24 10:00")
            .unwrap()
            .unwrap();
        assert_eq!(applied.written, 5);
        assert!(!pending(target.path()));
        assert_eq!(
            fs::read_to_string(target.path().join("state/preferences.toml")).unwrap(),
            "magnet = true\n"
        );
        assert_eq!(
            fs::read_to_string(target.path().join("scopes/live-2/drawings.toml")).unwrap(),
            "next_id = 9\n"
        );
        assert_eq!(
            fs::read_to_string(target.path().join("scopes/other/alerts.toml")).unwrap(),
            "keep = true\n",
            "what the backup does not hold is left alone"
        );
        // What was replaced is kept.
        let kept = applied.kept_in.unwrap();
        assert_eq!(
            fs::read_to_string(kept.join("state/preferences.toml")).unwrap(),
            "magnet = false\n"
        );
        // Applied once.
        assert!(apply_pending(target.path(), "again").unwrap().is_none());
    }

    #[test]
    fn a_staged_import_can_be_cancelled_and_a_broken_one_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let text = to_text(&collect(setup().path(), "1", "t").unwrap()).unwrap();
        stage(dir.path(), &text).unwrap();
        cancel_pending(dir.path()).unwrap();
        assert!(!pending(dir.path()));
        cancel_pending(dir.path()).unwrap();

        write(dir.path(), "pending-import.toml", "this is not a backup");
        assert!(apply_pending(dir.path(), "t").unwrap().is_none());
        assert!(!pending(dir.path()));
        assert!(dir.path().join("pending-import.toml.bad").is_file());
        assert!(stage(dir.path(), "garbage").is_err());
        assert!(!pending(dir.path()), "a bad file is not staged");
    }
}
