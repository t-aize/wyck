//! A check-up of the config: what is wrong, what is odd, and what could be tidied.
//!
//! [`crate::infra::storage::WyckConfig::diagnose`] looks at the config file, the profiles, the credentials they
//! need, the folders and their permissions, and the files that a crash or a failed read left
//! behind. It changes nothing. The report says what it found in words a person reads, so a front
//! end can show it as it is, and a script can look at [`Report::is_healthy`].

use std::fmt;
use std::path::Path;

use secrecy::ExposeSecret;

use crate::infra::storage::app_config::AppConfig;
use crate::infra::storage::documents::DocumentStore;
use crate::infra::storage::fs_util::stale_temp_files;
use crate::infra::storage::paths::AppPaths;
use crate::infra::storage::secret::SecretStore;
use crate::infra::storage::{CLIENT_SECRET, ProfileConfig};

/// How much a finding matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing, nothing to do.
    Info,
    /// Something is off and worth fixing; the app still works.
    Warning,
    /// Something does not work.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// One thing the check-up found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// How much it matters.
    pub severity: Severity,
    /// What it is about: `config`, `profile Demo`, `secrets`...
    pub area: String,
    /// What was found, and what to do about it when there is something to do.
    pub message: String,
}

/// Everything a check-up found, worst first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    findings: Vec<Finding>,
}

impl Report {
    fn add(&mut self, severity: Severity, area: impl Into<String>, message: impl Into<String>) {
        self.findings.push(Finding {
            severity,
            area: area.into(),
            message: message.into(),
        });
    }

    /// The findings, the worst first.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Whether nothing was found that does not work (warnings and notes do not count).
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }

    /// The worst severity found, if anything was.
    #[must_use]
    pub fn worst(&self) -> Option<Severity> {
        self.findings.iter().map(|finding| finding.severity).max()
    }

    fn sorted(mut self) -> Self {
        self.findings
            .sort_by(|a, b| b.severity.cmp(&a.severity).then(a.area.cmp(&b.area)));
        self
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.findings.is_empty() {
            return f.write_str("Nothing to report: the config is healthy.");
        }
        for (index, finding) in self.findings.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(
                f,
                "[{}] {}: {}",
                finding.severity, finding.area, finding.message
            )?;
        }
        Ok(())
    }
}

pub(crate) fn run(paths: &AppPaths, config: &AppConfig, secrets: &dyn SecretStore) -> Report {
    let mut report = Report::default();
    check_profiles(config, secrets, &mut report);
    check_folders(paths, &mut report);
    check_leftovers(paths, &mut report);
    report.sorted()
}

fn check_profiles(config: &AppConfig, secrets: &dyn SecretStore, report: &mut Report) {
    if let Some(id) = &config.active_profile
        && config.profile(id).is_none()
    {
        report.add(
            Severity::Warning,
            "config",
            format!("the active profile `{id}` no longer exists; pick another one"),
        );
    }
    let mut seen = std::collections::HashSet::new();
    for profile in &config.profiles {
        if !seen.insert(&profile.id) {
            report.add(
                Severity::Error,
                "config",
                format!("two profiles share the id `{}`; remove one", profile.id),
            );
        }
        check_profile(profile, secrets, report);
    }
}

fn check_profile(profile: &ProfileConfig, secrets: &dyn SecretStore, report: &mut Report) {
    let area = format!("profile {}", profile.display_name);
    if profile.display_name.trim().is_empty() {
        report.add(Severity::Info, &area, "it has no name");
    }
    // A profile that signs in through the Open API needs its application secret.
    if profile.client_id.is_some() {
        let key = crate::infra::storage::profile_secret_key(&profile.id, CLIENT_SECRET);
        match secrets.retrieve(&key) {
            Ok(Some(secret)) if secret.expose_secret().is_empty() => report.add(
                Severity::Error,
                &area,
                "the stored application secret is empty; enter it again",
            ),
            Ok(Some(_)) => {}
            Ok(None) => report.add(
                Severity::Warning,
                &area,
                "no application secret is stored for it; sign in again to store one",
            ),
            Err(error) => report.add(
                Severity::Error,
                &area,
                format!("the application secret could not be read: {error}"),
            ),
        }
        if profile.account_id.is_none() {
            report.add(
                Severity::Warning,
                &area,
                "no trading account is chosen for it yet",
            );
        }
    }
}

fn check_folders(paths: &AppPaths, report: &mut Report) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let too_open = |path: &Path, allowed: u32| {
            std::fs::metadata(path)
                .map(|meta| meta.permissions().mode() & 0o777 & !allowed != 0)
                .unwrap_or(false)
        };
        if too_open(&paths.config_file(), 0o600) {
            report.add(
                Severity::Warning,
                "config",
                format!(
                    "{} can be read by other users; run `chmod 600` on it",
                    paths.config_file().display()
                ),
            );
        }
        let secrets = paths.secrets_dir();
        if too_open(&secrets, 0o700) {
            report.add(
                Severity::Warning,
                "secrets",
                format!(
                    "{} can be entered by other users; run `chmod 700` on it",
                    secrets.display()
                ),
            );
        }
    }
    #[cfg(not(unix))]
    let _ = (paths, &report);
}

fn check_leftovers(paths: &AppPaths, report: &mut Report) {
    let mut folders = vec![
        paths.config_dir().to_path_buf(),
        paths.state_dir(),
        paths.secrets_dir(),
    ];
    if let Ok(scopes) = DocumentStore::list_scopes(paths) {
        folders.extend(
            scopes
                .iter()
                .map(|scope| DocumentStore::scoped(paths, scope).dir().to_path_buf()),
        );
    }
    folders.dedup();
    for folder in folders {
        let temp = stale_temp_files(&folder);
        if !temp.is_empty() {
            report.add(
                Severity::Info,
                "files",
                format!(
                    "{} unfinished write(s) left in {} by a crash; they are safe to delete",
                    temp.len(),
                    folder.display()
                ),
            );
        }
        for damaged in damaged_documents(&folder) {
            report.add(
                Severity::Warning,
                "documents",
                format!(
                    "{} was unreadable and set aside; the app started from defaults. Recover it by hand or delete it",
                    damaged.display()
                ),
            );
        }
    }
}

fn damaged_documents(folder: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bad"))
        .collect();
    found.sort();
    found
}
