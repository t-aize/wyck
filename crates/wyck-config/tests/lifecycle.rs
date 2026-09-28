//! The crate used the way an app uses it, through its public API only: an install is created,
//! used, closed and opened again, damaged, upgraded and backed up.

use std::path::Path;
use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use wyck_config::{
    AppPaths, CLIENT_SECRET, ConfigError, DocumentStore, OpenApiTokens, WyckConfig, sealed,
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
    assert!(wyck_config::stale_temp_files(store.dir()).is_empty());
}

/// A backup carried in a sealed file: every document of an install collected as text, sealed with
/// a passphrase, opened on another install, and restored. This is how an app makes an encrypted
/// export out of the pieces of the crate.
#[test]
fn a_sealed_backup_moves_every_document_to_another_install() {
    #[derive(Serialize, Deserialize)]
    struct Entry {
        /// Empty for the documents shared by every account.
        scope: String,
        name: String,
        content: String,
    }
    #[derive(Serialize, Deserialize)]
    struct Bundle {
        files: Vec<Entry>,
    }

    let (from, to) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let source = AppPaths::at(from.path());
    let global = DocumentStore::global(&source);
    global
        .save(
            "layout",
            &Layout {
                columns: vec!["a".into()],
                zoom: 2.0,
            },
        )
        .unwrap();
    let account = DocumentStore::scoped(&source, "demo-1");
    account.save("layout", &Layout::default()).unwrap();
    account
        .save_text("notes", "# kept as written\nx = 1\n")
        .unwrap();

    // Collect.
    let mut files = Vec::new();
    for name in global.list().unwrap() {
        files.push(Entry {
            scope: String::new(),
            content: global.load_text(&name).unwrap().unwrap(),
            name,
        });
    }
    for scope in DocumentStore::list_scopes(&source).unwrap() {
        let store = DocumentStore::scoped(&source, &scope);
        for name in store.list().unwrap() {
            files.push(Entry {
                scope: scope.clone(),
                content: store.load_text(&name).unwrap().unwrap(),
                name,
            });
        }
    }
    let text = toml::to_string(&Bundle { files }).unwrap();
    let file = sealed::seal_text(&passphrase("carry-me"), "wyck-backup", &text).unwrap();
    assert!(!file.contains("kept as written") && !file.contains("demo-1"));

    // Restore, elsewhere.
    assert!(sealed::open_text(&passphrase("wrong"), "wyck-backup", &file).is_err());
    let opened = sealed::open_text(&passphrase("carry-me"), "wyck-backup", &file).unwrap();
    let bundle: Bundle = toml::from_str(&opened).unwrap();
    let target = AppPaths::at(to.path());
    for entry in bundle.files {
        let store = if entry.scope.is_empty() {
            DocumentStore::global(&target)
        } else {
            wyck_config::names::validate_name(&entry.scope).unwrap();
            DocumentStore::scoped(&target, &entry.scope)
        };
        store.save_text(&entry.name, &entry.content).unwrap();
    }

    let restored = DocumentStore::scoped(&target, "demo-1");
    assert_eq!(
        restored.load_text("notes").unwrap().as_deref(),
        Some("# kept as written\nx = 1\n"),
        "text comes back byte for byte, comments included"
    );
    assert_eq!(
        DocumentStore::global(&target)
            .load::<Layout>("layout")
            .unwrap()
            .unwrap()
            .zoom,
        2.0
    );
}
