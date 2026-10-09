//! Lists, saves, exports and restores the backups of a Wyck install from the command line.
//!
//! ```text
//! cargo run --example config_backup -- list
//! cargo run --example config_backup -- save
//! cargo run --example config_backup -- export ./my-backup.toml
//! WYCK_BACKUP_PASSPHRASE=... cargo run --example config_backup -- export ./locked.toml
//! cargo run --example config_backup -- import ./my-backup.toml
//! cargo run --example config_backup -- restore <id>
//! cargo run --example config_backup -- prune 5
//! ```
//!
//! * `--dir <folder>` works on a portable install instead of the standard folders.
//! * With `WYCK_BACKUP_PASSPHRASE` set, `export` seals the file, and `import` and `restore` open a
//!   sealed one.
//! * `import` and `restore` only stage: the app applies the backup when it starts (or run
//!   `apply`, with the app closed).

use std::path::PathBuf;
use std::process::ExitCode;

use secrecy::SecretString;
use wyck::infra::storage::backup::{self, Backup, BackupKind};
use wyck::infra::storage::{AppPaths, ConfigError};

const USAGE: &str = "usage: config_backup [--dir <folder>] <list | save | export <file> | import <file> | restore <id> | apply | prune <keep>>";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("unix-{seconds}")
}

fn describe(backup: &Backup) -> String {
    let contents = backup.contents();
    format!(
        "{} document(s) in {} account(s), {} script(s)",
        contents.documents(),
        contents.scopes.len(),
        contents.scripts
    )
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let paths = match args.iter().position(|a| a == "--dir") {
        Some(at) => {
            let dir = args.get(at + 1).cloned().ok_or(USAGE)?;
            args.drain(at..=at + 1);
            AppPaths::at(dir)
        }
        None => AppPaths::discover().map_err(|e| e.to_string())?,
    };
    let passphrase = std::env::var("WYCK_BACKUP_PASSPHRASE")
        .ok()
        .filter(|p| !p.is_empty())
        .map(SecretString::from);
    let store = paths.backups();
    let failed = |e: ConfigError| e.to_string();
    let scripts = paths.scripts();

    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["list"] => {
            let entries = store.list().map_err(failed)?;
            if entries.is_empty() {
                println!("no backup in {}", store.dir().display());
            }
            for e in entries {
                println!(
                    "{:<34} {:<16} {:>9} bytes{}",
                    e.id,
                    e.kind.label(),
                    e.bytes,
                    if e.sealed { "  (sealed)" } else { "" }
                );
            }
        }
        ["save"] => {
            let backup = Backup::collect(&paths, Some(&scripts), env!("CARGO_PKG_VERSION"), &now())
                .map_err(failed)?;
            let entry = store
                .save(BackupKind::Manual, &backup, passphrase.as_ref())
                .map_err(failed)?;
            println!("saved {} ({})", entry.path.display(), describe(&backup));
        }
        ["export", file] => {
            let backup = backup::export_to_file(
                &paths,
                Some(&scripts),
                &PathBuf::from(file),
                passphrase.as_ref(),
                env!("CARGO_PKG_VERSION"),
                &now(),
            )
            .map_err(failed)?;
            println!("wrote {file} ({})", describe(&backup));
        }
        ["import", file] => {
            let backup =
                backup::stage_import_file(&paths, &PathBuf::from(file), passphrase.as_ref())
                    .map_err(failed)?;
            println!("staged, applied at the next start ({})", describe(&backup));
        }
        ["restore", id] => {
            let backup = store.restore(id, passphrase.as_ref()).map_err(failed)?;
            println!("staged, applied at the next start ({})", describe(&backup));
        }
        ["apply"] => {
            let applied = backup::apply_pending(&paths, &now()).map_err(failed)?;
            println!("{applied:?}");
        }
        ["prune", keep] => {
            let keep: usize = keep.parse().map_err(|_| USAGE)?;
            let removed = store.prune(BackupKind::Automatic, keep).map_err(failed)?;
            println!("removed {removed} automatic backup(s)");
        }
        _ => return Err(USAGE.to_owned()),
    }
    Ok(())
}
