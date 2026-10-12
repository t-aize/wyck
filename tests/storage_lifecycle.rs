//! The crate used the way an app uses it, through its public API only: an install is created,
//! used, closed and opened again, damaged, upgraded and backed up.
// A failed `unwrap` or `expect` is how a test fails. clippy.toml covers only `#[test]` functions
// and `#[cfg(test)]` modules, not the helpers of an integration test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use wyck::infra::storage::migrate::{Outcome, Plan, Step, StepError, upgrade_document};
use wyck::infra::storage::{
    AppPaths, CLIENT_SECRET, ConfigError, DocumentStore, OpenApiTokens, WyckConfig,
};

fn passphrase(text: &str) -> SecretString {
    SecretString::from(text.to_owned())
}

fn open(dir: &Path) -> WyckConfig {
    WyckConfig::builder()
        .portable(dir)
        .encrypted_file(passphrase("install-passphrase"))
        .build()
        .expect("the install opens")
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
struct Layout {
    #[serde(default)]
    columns: Vec<String>,
    #[serde(default)]
    zoom: f64,
}

fn every_file(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_first_run_a_working_session_and_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let account = {
        let mut config = open(dir.path());
        assert!(config.profiles().is_empty(), "a first run starts empty");

        let id = config
            .add_profile("Demo: scalping", "ctrader-openapi")
            .unwrap();
        config
            .set_openapi_profile(&id, "public-client-id".into(), 8765, 4242)
            .unwrap();
        config
            .set_profile_secret(&id, CLIENT_SECRET, &passphrase("APP-SECRET-VALUE"))
            .unwrap();
        config
            .openapi_token_storage(&id)
            .save(&OpenApiTokens {
                access_token: passphrase("ACCESS-TOKEN-VALUE"),
                refresh_token: passphrase("REFRESH-TOKEN-VALUE"),
                expires_at: None,
            })
            .unwrap();
        config.set_active_profile(Some(id.clone())).unwrap();
        config.set_last_symbol(Some("EURUSD".into())).unwrap();

        DocumentStore::global(config.paths())
            .save(
                "layout",
                &Layout {
                    columns: vec!["price".into(), "volume".into()],
                    zoom: 1.25,
                },
            )
            .unwrap();
        DocumentStore::scoped(config.paths(), "demo-4242")
            .save(
                "layout",
                &Layout {
                    columns: vec!["pnl".into()],
                    zoom: 0.5,
                },
            )
            .unwrap();
        id
    };

    // The app is closed and opened again.
    let config = open(dir.path());
    assert_eq!(config.active_profile().unwrap().id, account);
    assert_eq!(config.active_profile().unwrap().account_id, Some(4242));
    assert_eq!(config.last_symbol(), Some("EURUSD"));
    assert_eq!(
        config
            .profile_secret(&account, CLIENT_SECRET)
            .unwrap()
            .unwrap()
            .expose_secret(),
        "APP-SECRET-VALUE"
    );
    assert_eq!(
        config
            .openapi_token_storage(&account)
            .load()
            .unwrap()
            .unwrap()
            .refresh_token
            .expose_secret(),
        "REFRESH-TOKEN-VALUE"
    );
    assert_eq!(
        DocumentStore::global(config.paths())
            .load::<Layout>("layout")
            .unwrap()
            .unwrap()
            .zoom,
        1.25
    );
    assert_eq!(
        DocumentStore::scoped(config.paths(), "demo-4242")
            .load::<Layout>("layout")
            .unwrap()
            .unwrap()
            .columns,
        ["pnl"]
    );
    assert!(config.diagnose().is_healthy(), "{}", config.diagnose());

    // No secret is anywhere on disk in the clear, in any file.
    for path in every_file(dir.path()) {
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for secret in [
            "APP-SECRET-VALUE",
            "ACCESS-TOKEN-VALUE",
            "REFRESH-TOKEN-VALUE",
        ] {
            assert!(!text.contains(secret), "{secret} is in {}", path.display());
        }
    }
}

#[test]
fn removing_an_account_leaves_no_credential_and_no_profile() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = open(dir.path());
    let id = config.add_profile("Old", "ctrader-openapi").unwrap();
    config
        .set_profile_secret(&id, CLIENT_SECRET, &passphrase("s"))
        .unwrap();
    config
        .openapi_token_storage(&id)
        .save(&OpenApiTokens {
            access_token: passphrase("a"),
            refresh_token: passphrase("r"),
            expires_at: None,
        })
        .unwrap();
    assert_eq!(every_file(&AppPaths::at(dir.path()).secrets_dir()).len(), 2);

    config.remove_profile(&id).unwrap();

    assert!(every_file(&AppPaths::at(dir.path()).secrets_dir()).is_empty());
    assert!(open(dir.path()).profiles().is_empty());
}

#[test]
fn a_wrong_passphrase_is_reported_as_such_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = open(dir.path());
    let id = config.add_profile("Demo", "s").unwrap();
    config
        .set_profile_secret(&id, CLIENT_SECRET, &passphrase("token"))
        .unwrap();
    drop(config);

    let intruder = WyckConfig::builder()
        .portable(dir.path())
        .encrypted_file(passphrase("guess"))
        .build()
        .unwrap();
    let error = intruder.profile_secret(&id, CLIENT_SECRET).unwrap_err();
    assert!(matches!(error, ConfigError::Crypto { .. }), "{error:?}");
    assert!(
        !error.to_string().contains("token"),
        "an error never carries the value"
    );

    assert_eq!(
        open(dir.path())
            .profile_secret(&id, CLIENT_SECRET)
            .unwrap()
            .unwrap()
            .expose_secret(),
        "token",
        "the right passphrase still works"
    );
}

#[test]
fn a_damaged_config_is_reported_and_never_overwritten_and_damaged_documents_are_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = open(dir.path());
    config.add_profile("Demo", "s").unwrap();
    DocumentStore::global(config.paths())
        .save("layout", &Layout::default())
        .unwrap();
    drop(config);

    let paths = AppPaths::at(dir.path());
    std::fs::write(paths.config_file(), "profiles = [[[").unwrap();
    let store = DocumentStore::global(&paths);
    std::fs::write(store.path("layout"), "columns = 12 ==").unwrap();

    // The config refuses to load, so nothing can save over the file the user may want to repair.
    let result = WyckConfig::builder()
        .portable(dir.path())
        .encrypted_file(passphrase("install-passphrase"))
        .build();
    assert!(matches!(result, Err(ConfigError::Parse { .. })));
    assert_eq!(
        std::fs::read_to_string(paths.config_file()).unwrap(),
        "profiles = [[["
    );

    // A document does not take the app down: it starts fresh and keeps the damaged copy.
    assert_eq!(store.load_or_default::<Layout>("layout"), Layout::default());
    assert!(store.dir().join("layout.toml.bad").is_file());
}

#[test]
fn a_config_written_by_a_newer_version_is_refused_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path());
    let future = "schema_version = 7\nsomething_new = \"kept\"\n";
    std::fs::write(paths.config_file(), future).unwrap();

    let result = WyckConfig::builder().portable(dir.path()).build();

    assert!(matches!(
        result,
        Err(ConfigError::UnsupportedSchema { found: 7, .. })
    ));
    assert_eq!(
        std::fs::read_to_string(paths.config_file()).unwrap(),
        future
    );
}

#[test]
fn files_in_a_format_this_build_does_not_write_are_refused_not_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path());

    // A config with no version, as an earlier layout would have it.
    std::fs::write(paths.config_file(), "last_symbol = \"EURUSD\"\n").unwrap();
    assert!(matches!(
        WyckConfig::builder().portable(dir.path()).build(),
        Err(ConfigError::Parse { .. })
    ));

    // A secret envelope of another version.
    std::fs::remove_file(paths.config_file()).unwrap();
    let mut config = open(dir.path());
    let id = config.add_profile("Demo", "s").unwrap();
    config
        .set_profile_secret(&id, CLIENT_SECRET, &passphrase("token"))
        .unwrap();
    let file = every_file(&paths.secrets_dir()).pop().unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("version = 1", "version = 0")).unwrap();
    assert!(config.profile_secret(&id, CLIENT_SECRET).is_err());
}

fn rename_zoom(table: &mut toml::Table) -> Result<(), StepError> {
    if let Some(zoom) = table.remove("zoom") {
        table.insert("scale".to_owned(), zoom);
    }
    Ok(())
}

fn add_columns(table: &mut toml::Table) -> Result<(), StepError> {
    table
        .entry("columns")
        .or_insert_with(|| toml::Value::Array(vec![toml::Value::String("price".to_owned())]));
    Ok(())
}

const LAYOUT_PLAN: Plan = Plan {
    document: "layout",
    current: 3,
    steps: &[
        Step {
            from: 1,
            apply: rename_zoom,
        },
        Step {
            from: 2,
            apply: add_columns,
        },
    ],
};

/// What an update does to an install, in the order it does it: a copy of everything, then, at the
/// next start, the documents brought forward. The copy brings the old layout back.
#[test]
fn an_update_keeps_a_copy_then_brings_the_documents_forward_and_the_copy_can_undo_it() {
    use wyck::infra::storage::backup::store::BackupKind;

    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path());
    let store = paths.documents();
    let old = "schema_version = 1\nzoom = 1.5\n";
    store.save_text("layout", old).unwrap();

    // The update is about to be installed.
    let copy = paths
        .backups()
        .snapshot_before_update(None, "0.2.0", "2026-10-12T09:00:00Z")
        .unwrap()
        .unwrap();
    assert_eq!(copy.kind, BackupKind::BeforeUpdate);

    // The next start, by the new version, two layouts later.
    let outcome = upgrade_document(&store, &LAYOUT_PLAN).unwrap();
    assert!(matches!(outcome, Outcome::Migrated { from: 1, to: 3, .. }));
    let upgraded = store.load_text("layout").unwrap().unwrap();
    assert!(upgraded.contains("schema_version = 3") && upgraded.contains("scale = 1.5"));
    assert!(
        !upgraded.contains("zoom"),
        "the document is in the new layout: {upgraded}"
    );

    // The user goes back: the copy is staged, applied at the next start, and the old layout is
    // there again, to be brought forward once more by whichever version runs next.
    paths.backups().restore(&copy.id, None).unwrap();
    wyck::infra::storage::backup::staged::apply_pending(&paths, "2026-10-12T09:05:00Z").unwrap();
    assert_eq!(store.load_text("layout").unwrap().as_deref(), Some(old));
    assert!(matches!(
        upgrade_document(&store, &LAYOUT_PLAN).unwrap(),
        Outcome::Migrated { from: 1, .. }
    ));
    assert_eq!(
        std::fs::read_to_string(store.dir().join("layout.toml.v1.bak")).unwrap(),
        old,
        "the first copy of the old layout is the one kept"
    );
}

#[test]
fn a_document_from_a_newer_version_is_never_rewritten_by_this_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = AppPaths::at(dir.path()).documents();
    let future = "schema_version = 8\nscale = 2.0\nfrom_the_future = [1, 2]\n";
    store.save_text("layout", future).unwrap();

    let outcome = upgrade_document(&store, &LAYOUT_PLAN).unwrap();

    let Outcome::Newer { found: 8, kept_as } = outcome else {
        panic!("expected a newer document, got {outcome:?}");
    };
    assert_eq!(
        std::fs::read_to_string(kept_as.unwrap()).unwrap(),
        future,
        "byte for byte"
    );
    assert_eq!(store.load_or_default::<Layout>("layout"), Layout::default());
}

/// What a user does with a cloud folder: the data of one computer is moved there, the app starts
/// again on it, a second computer joins it, and the other build is kept out.
#[test]
fn data_moved_to_a_cloud_folder_is_followed_shared_and_kept_from_the_other_build() {
    use wyck::infra::platform::build_info::BuildMode;
    use wyck::infra::storage::locations::{Kind, Locations, cloud_layout};
    use wyck::infra::storage::relocate::{Change, apply_pending, stage};

    let temp = tempfile::tempdir().unwrap();
    let cloud = temp.path().join("OneDrive");
    std::fs::create_dir_all(&cloud).unwrap();
    let start = |base: &Path, mode| {
        AppPaths::at_for(mode, base).with_locations(Locations::load(base).unwrap())
    };
    let preset = |mode| -> Vec<Change> {
        cloud_layout(&cloud, mode)
            .into_iter()
            .map(|(kind, dir)| Change {
                kind,
                to: Some(dir),
            })
            .collect()
    };

    // The first computer: some data, then the move, applied at the next start.
    let first = temp.path().join("first");
    let paths = start(&first, BuildMode::Production);
    paths
        .documents()
        .save_text("preferences", "magnet = true\n")
        .unwrap();
    paths
        .scope("demo-1")
        .save_text("alerts", "next_id = 3\n")
        .unwrap();
    stage(&paths, &preset(BuildMode::Production)).unwrap();
    let applied = apply_pending(&paths, "first");
    assert!(
        applied.failed.is_empty() && applied.refused.is_none(),
        "{applied:?}"
    );

    let paths = start(&first, BuildMode::Production);
    for kind in [
        Kind::Settings,
        Kind::Accounts,
        Kind::Indicators,
        Kind::Backups,
    ] {
        assert!(paths.dir(kind).starts_with(cloud.join("Wyck")), "{kind}");
    }
    assert!(
        !paths.is_custom(Kind::Logs),
        "the logs stay on the computer"
    );
    assert_eq!(
        paths
            .documents()
            .load_text("preferences")
            .unwrap()
            .as_deref(),
        Some("magnet = true\n")
    );
    assert_eq!(
        paths
            .scope("demo-1")
            .load_text("alerts")
            .unwrap()
            .as_deref(),
        Some("next_id = 3\n"),
        "an account's data is found by its number"
    );
    assert!(
        paths
            .documents()
            .path("preferences")
            .starts_with(cloud.join("Wyck"))
    );

    // A second computer picks the same folders and uses what is in them.
    let second = temp.path().join("second");
    let other = start(&second, BuildMode::Production);
    other
        .documents()
        .save_text("preferences", "magnet = false\n")
        .unwrap();
    stage(&other, &preset(BuildMode::Production)).unwrap();
    apply_pending(&other, "second");
    let other = start(&second, BuildMode::Production);
    assert_eq!(
        other
            .documents()
            .load_text("preferences")
            .unwrap()
            .as_deref(),
        Some("magnet = true\n"),
        "what the first computer saved wins"
    );

    // A development build neither takes that folder nor shares its own with the installed app.
    let dev = start(&temp.path().join("dev"), BuildMode::Development);
    let error = stage(&dev, &preset(BuildMode::Production)).unwrap_err();
    assert!(
        error.to_string().contains("belongs to the installed app"),
        "{error}"
    );
    stage(&dev, &preset(BuildMode::Development)).unwrap();
    assert!(
        cloud_layout(&cloud, BuildMode::Development)[0]
            .1
            .starts_with(cloud.join("Wyck (dev)"))
    );
}

#[test]
fn two_installs_share_nothing() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut first = open(a.path());
    let id = first.add_profile("Only in A", "s").unwrap();
    first
        .set_profile_secret(&id, CLIENT_SECRET, &passphrase("t"))
        .unwrap();
    let second = open(b.path());
    assert!(second.profiles().is_empty());
    assert!(every_file(&AppPaths::at(b.path()).secrets_dir()).is_empty());
}

#[test]
fn many_threads_saving_one_document_never_tear_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(DocumentStore::global(&AppPaths::at(dir.path())));
    let threads: Vec<_> = (0..6)
        .map(|n| {
            let store = store.clone();
            std::thread::spawn(move || {
                for round in 0..20 {
                    let layout = Layout {
                        columns: (0..200).map(|i| format!("w{n}-c{i}")).collect(),
                        zoom: f64::from(round),
                    };
                    store.save("layout", &layout).unwrap();
                    // Whatever is read is one writer's complete document.
                    let read = store.load::<Layout>("layout").unwrap().unwrap();
                    assert_eq!(read.columns.len(), 200);
                    let writer = read.columns[0].split('-').next().unwrap().to_owned();
                    assert!(read.columns.iter().all(|c| c.starts_with(&writer)));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(wyck::infra::storage::stale_temp_files(store.dir()).is_empty());
}

/// A backup carried in a sealed file: every document of an install collected as text, sealed with
/// a passphrase, opened on another install, and restored. This is how an app makes an encrypted
/// export out of the pieces of the crate.
#[test]
fn a_sealed_backup_moves_every_document_and_script_to_another_install() {
    use wyck::infra::storage::backup::{self, Backup};

    let (from, to) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let source = AppPaths::at(from.path());
    let global = source.documents();
    global
        .save(
            "layout",
            &Layout {
                columns: vec!["a".into()],
                zoom: 2.0,
            },
        )
        .unwrap();
    let account = source.scope("demo-1");
    account.save("layout", &Layout::default()).unwrap();
    account
        .save_text("notes", "# kept as written\nx = 1\n")
        .unwrap();
    source
        .scripts()
        .write("Trend/Average", "plot(\"a\", close);")
        .unwrap();

    // Export, sealed, to a file that can be put anywhere.
    let file = to.path().join("carried").join("backup.toml");
    let made = backup::staged::export_to_file(
        &source,
        Some(&source.scripts()),
        &file,
        Some(&passphrase("carry-me")),
        "1.0",
        "2026-09-29T10:00:00Z",
    )
    .unwrap();
    assert_eq!(made.contents().documents(), 3);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("kept as written") && !text.contains("demo-1"));

    // Import, elsewhere: checked and staged, applied at the next start.
    let target = AppPaths::at(to.path().join("other"));
    assert!(matches!(
        backup::staged::stage_import_file(&target, &file, None),
        Err(wyck::infra::storage::ConfigError::PassphraseRequired)
    ));
    assert!(matches!(
        backup::staged::stage_import_file(&target, &file, Some(&passphrase("wrong"))),
        Err(wyck::infra::storage::ConfigError::WrongPassphrase)
    ));
    let staged =
        backup::staged::stage_import_file(&target, &file, Some(&passphrase("carry-me"))).unwrap();
    assert_eq!(staged, made);
    assert!(backup::staged::import_pending(&target));
    assert_eq!(target.scope("demo-1").load_text("notes").unwrap(), None);

    let applied = backup::staged::apply_pending(&target, "2026-09-29T10:05:00Z").unwrap();
    assert_eq!(applied.imported, Some(4));
    assert_eq!(
        target
            .scope("demo-1")
            .load_text("notes")
            .unwrap()
            .as_deref(),
        Some("# kept as written\nx = 1\n"),
        "text comes back byte for byte, comments included"
    );
    assert_eq!(
        target
            .documents()
            .load::<Layout>("layout")
            .unwrap()
            .unwrap()
            .zoom,
        2.0
    );
    assert_eq!(
        target.scripts().read("Trend/Average").unwrap().as_deref(),
        Some("plot(\"a\", close);")
    );
    assert!(!Backup::collect(&target, None, "1", "t").unwrap().is_empty());
}

#[test]
fn the_copies_an_install_keeps_can_be_listed_restored_and_pruned() {
    use wyck::infra::storage::backup;
    use wyck::infra::storage::backup::store::{AutoPolicy, BackupKind};

    let dir = tempfile::tempdir().unwrap();
    let mut config = WyckConfig::builder()
        .portable(dir.path())
        .encrypted_file(passphrase("pw"))
        .build()
        .unwrap();
    config.add_profile("Demo", "ctrader-openapi").unwrap();
    config
        .documents()
        .save_text("preferences", "magnet = true\n")
        .unwrap();

    // One automatic copy, and not a second one right after.
    let policy = AutoPolicy::default();
    let store = config.backups();
    let auto = store
        .auto_snapshot(
            Some(&config.scripts()),
            "1.0",
            "2026-09-29T10:00:00Z",
            policy,
        )
        .unwrap()
        .unwrap();
    assert!(
        store
            .auto_snapshot(None, "1.0", "later", policy)
            .unwrap()
            .is_none()
    );

    // The user changes their mind: a reset, then the copy brings everything back.
    config
        .documents()
        .save_text("preferences", "magnet = false\n")
        .unwrap();
    backup::staged::stage_reset(config.paths()).unwrap();
    let applied = backup::staged::apply_pending(config.paths(), "2026-09-29T11:00:00Z").unwrap();
    assert!(applied.reset);
    assert_eq!(config.documents().load_text("preferences").unwrap(), None);

    let kinds: Vec<_> = store.list().unwrap().iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&BackupKind::BeforeReset), "{kinds:?}");
    store.restore(&auto.id, None).unwrap();
    backup::staged::apply_pending(config.paths(), "2026-09-29T11:05:00Z").unwrap();
    assert_eq!(
        config
            .documents()
            .load_text("preferences")
            .unwrap()
            .as_deref(),
        Some("magnet = true\n")
    );

    // Neither a reset nor a restore touches the profiles: the sign-in stays.
    assert_eq!(config.profiles().len(), 1);
    assert!(store.prune(BackupKind::BeforeReset, 0).unwrap() >= 1);
}
