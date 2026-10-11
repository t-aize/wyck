use super::*;
use secrecy::ExposeSecret;
use std::time::{Duration, UNIX_EPOCH};

fn config_in(dir: &std::path::Path) -> WyckConfig {
    WyckConfig::builder()
        .portable(dir)
        .encrypted_file(SecretString::from("test-passphrase".to_owned()))
        .build()
        .unwrap()
}

fn secret(text: &str) -> SecretString {
    SecretString::from(text.to_owned())
}

#[test]
fn the_last_symbol_survives_a_restart_and_a_config_without_one_still_loads() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = config_in(temp_dir.path());
    assert_eq!(config.last_symbol(), None, "a first run remembers nothing");

    config
        .set_last_symbol(Some("  XAUUSD ".to_owned()))
        .unwrap();
    assert_eq!(config.last_symbol(), Some("XAUUSD"));
    drop(config);
    let mut again = config_in(temp_dir.path());
    assert_eq!(again.last_symbol(), Some("XAUUSD"));

    again.set_last_symbol(Some("   ".to_owned())).unwrap();
    assert_eq!(again.last_symbol(), None, "a blank name forgets it");
    assert!(
        !std::fs::read_to_string(AppPaths::at(temp_dir.path()).config_file())
            .unwrap()
            .contains("last_symbol"),
        "nothing is written for a forgotten symbol"
    );
}

#[test]
fn a_profile_and_its_secret_persist_across_a_restart() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = config_in(temp_dir.path());

    let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
    config
        .set_profile_secret(&id, CLIENT_SECRET, &secret("token-123"))
        .unwrap();

    assert_eq!(config.profiles().len(), 1);
    assert_eq!(config.profile(&id).unwrap().display_name, "Demo");

    // Re-load fresh from disk to prove persistence, not just in-memory state.
    let reloaded = config_in(temp_dir.path());
    assert_eq!(reloaded.profiles().len(), 1);
    assert_eq!(
        reloaded
            .profile_secret(&id, CLIENT_SECRET)
            .unwrap()
            .unwrap()
            .expose_secret(),
        "token-123"
    );
}

#[test]
fn a_new_profile_has_no_secret() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = config_in(temp_dir.path());
    let id = config
        .add_profile("Local desktop", "ctrader-local")
        .unwrap();
    assert!(config.profile_secret(&id, CLIENT_SECRET).unwrap().is_none());
    assert!(config.openapi_token_storage(&id).load().unwrap().is_none());
}

#[test]
fn remove_profile_deletes_config_entry_secrets_and_active_pointer() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = config_in(temp_dir.path());
    let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
    config.set_active_profile(Some(id.clone())).unwrap();
    config
        .set_profile_secret(&id, CLIENT_SECRET, &secret("app-secret"))
        .unwrap();
    config
        .set_profile_secret(&id, OAUTH_TOKENS, &secret("refresh"))
        .unwrap();

    config.remove_profile(&id).unwrap();

    assert!(config.profiles().is_empty());
    assert!(config.active_profile().is_none());
    for name in [CLIENT_SECRET, OAUTH_TOKENS] {
        assert!(
            config
                .secrets
                .retrieve(&profile_secret_key(&id, name))
                .unwrap()
                .is_none(),
            "{name} is gone"
        );
    }
    // And it stays removed after a restart.
    assert!(config_in(temp_dir.path()).profiles().is_empty());
}

#[test]
fn openapi_profile_settings_and_secrets_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config_in(dir.path());
    let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
    config
        .set_openapi_profile(&id, "client-id".into(), 8765, 42)
        .unwrap();
    config
        .set_profile_secret(&id, CLIENT_SECRET, &secret("secret"))
        .unwrap();
    config
        .openapi_token_storage(&id)
        .save(&OpenApiTokens {
            access_token: secret("access"),
            refresh_token: secret("refresh"),
            expires_at: Some(UNIX_EPOCH + Duration::from_secs(1_800_000_000)),
        })
        .unwrap();
    drop(config);

    let config = config_in(dir.path());
    let profile = config.profile(&id).unwrap();
    assert_eq!(profile.client_id.as_deref(), Some("client-id"));
    assert_eq!(profile.callback_port, Some(8765));
    assert_eq!(profile.account_id, Some(42));
    assert_eq!(
        config
            .profile_secret(&id, CLIENT_SECRET)
            .unwrap()
            .unwrap()
            .expose_secret(),
        "secret"
    );
    let tokens = config.openapi_token_storage(&id).load().unwrap().unwrap();
    assert_eq!(tokens.access_token.expose_secret(), "access");
    assert_eq!(tokens.refresh_token.expose_secret(), "refresh");
    assert_eq!(
        tokens.expires_at,
        Some(UNIX_EPOCH + Duration::from_secs(1_800_000_000))
    );
    assert!(
        !std::fs::read_to_string(config.paths().config_file())
            .unwrap()
            .contains("secret")
    );
}

#[test]
fn unknown_profiles_are_rejected_everywhere() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = config_in(temp_dir.path());
    let ghost = ProfileId::new_random();

    assert!(matches!(
        config.remove_profile(&ghost),
        Err(ConfigError::UnknownProfile(_))
    ));
    assert!(matches!(
        config.set_active_profile(Some(ghost.clone())),
        Err(ConfigError::UnknownProfile(_))
    ));
    assert!(matches!(
        config.set_openapi_profile(&ghost, "c".into(), 1, 2),
        Err(ConfigError::UnknownProfile(_))
    ));
    assert!(matches!(
        config.set_profile_secret(&ghost, CLIENT_SECRET, &secret("x")),
        Err(ConfigError::UnknownProfile(_))
    ));
    assert!(matches!(
        config.profile_secret(&ghost, CLIENT_SECRET),
        Err(ConfigError::UnknownProfile(_))
    ));
}

#[test]
fn a_credential_name_cannot_be_a_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = config_in(temp_dir.path());
    let id = config.add_profile("P", "s").unwrap();
    for bad in ["", "../x", "a/b", "a:b", "with space"] {
        assert!(
            matches!(
                config.set_profile_secret(&id, bad, &secret("x")),
                Err(ConfigError::InvalidName { .. })
            ),
            "{bad:?}"
        );
    }
    config
        .set_profile_secret(&id, "api-key_2", &secret("ok"))
        .unwrap();
    config.delete_profile_secret(&id, "api-key_2").unwrap();
    assert!(config.profile_secret(&id, "api-key_2").unwrap().is_none());
}

/// A store that fails on demand, to see what a failure leaves behind.
struct Flaky {
    inner: EncryptedFileSecretStore,
    fail_deletes: std::sync::atomic::AtomicBool,
}

impl SecretStore for Flaky {
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()> {
        self.inner.store(key, secret)
    }
    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>> {
        self.inner.retrieve(key)
    }
    fn delete(&self, key: &SecretKey) -> Result<()> {
        if self.fail_deletes.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ConfigError::SecretStore {
                key: key.to_string(),
                message: "the keyring is locked".into(),
            });
        }
        self.inner.delete(key)
    }
}

fn flaky_config(dir: &std::path::Path) -> (WyckConfig, Arc<Flaky>) {
    let paths = AppPaths::at(dir);
    let flaky = Arc::new(Flaky {
        inner: EncryptedFileSecretStore::new(paths.secrets_dir(), secret("pw")),
        fail_deletes: std::sync::atomic::AtomicBool::new(false),
    });
    struct Shared(Arc<Flaky>);
    impl SecretStore for Shared {
        fn store(&self, k: &SecretKey, s: &SecretString) -> Result<()> {
            self.0.store(k, s)
        }
        fn retrieve(&self, k: &SecretKey) -> Result<Option<SecretString>> {
            self.0.retrieve(k)
        }
        fn delete(&self, k: &SecretKey) -> Result<()> {
            self.0.delete(k)
        }
    }
    let config = WyckConfig::load(paths, Box::new(Shared(flaky.clone()))).unwrap();
    (config, flaky)
}

#[test]
fn a_failed_credential_delete_keeps_the_profile_so_the_removal_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let (mut config, flaky) = flaky_config(dir.path());
    let id = config.add_profile("Demo", "s").unwrap();
    config
        .set_profile_secret(&id, CLIENT_SECRET, &secret("token"))
        .unwrap();
    flaky
        .fail_deletes
        .store(true, std::sync::atomic::Ordering::SeqCst);

    assert!(config.remove_profile(&id).is_err());
    assert_eq!(config.profiles().len(), 1, "the profile is still there");
    assert_eq!(config_in_flaky_reload(dir.path()).profiles().len(), 1);

    flaky
        .fail_deletes
        .store(false, std::sync::atomic::Ordering::SeqCst);
    config.remove_profile(&id).unwrap();
    assert!(config.profiles().is_empty());
}

fn config_in_flaky_reload(dir: &std::path::Path) -> WyckConfig {
    WyckConfig::load(
        AppPaths::at(dir),
        Box::new(EncryptedFileSecretStore::new(
            AppPaths::at(dir).secrets_dir(),
            secret("pw"),
        )),
    )
    .unwrap()
}

#[test]
fn a_failed_save_leaves_the_memory_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config_in(dir.path());
    let kept = config.add_profile("Kept", "s").unwrap();

    // The config file can no longer be replaced: a directory sits where it goes.
    let file = config.paths().config_file();
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();

    assert!(config.add_profile("Lost", "s").is_err());
    assert!(config.set_active_profile(Some(kept.clone())).is_err());
    assert!(config.set_last_symbol(Some("EURUSD".into())).is_err());
    assert!(config.set_openapi_profile(&kept, "c".into(), 1, 2).is_err());

    assert_eq!(config.profiles().len(), 1, "no profile was kept in memory");
    assert!(
        config.active_profile().is_none(),
        "no change was kept in memory"
    );
    assert_eq!(config.last_symbol(), None);
    assert_eq!(config.profile(&kept).unwrap().client_id, None);
}

#[test]
fn the_builder_defaults_are_the_os_keyring_and_the_standard_folders() {
    // Nothing is read until `build`, and a portable folder is honored.
    let dir = tempfile::tempdir().unwrap();
    let config = WyckConfig::builder()
        .portable(dir.path())
        .secret_store(Box::new(EncryptedFileSecretStore::new(
            dir.path().join("s"),
            secret("pw"),
        )))
        .build()
        .unwrap();
    assert_eq!(config.paths().config_dir(), dir.path());
    assert!(config.profiles().is_empty());
}

#[test]
fn a_healthy_config_has_nothing_to_report_and_a_broken_one_says_what() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config_in(dir.path());
    let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
    assert!(config.diagnose().is_healthy());

    // An Open API profile with no application secret stored.
    config
        .set_openapi_profile(&id, "client".into(), 8765, 1)
        .unwrap();
    let report = config.diagnose();
    assert!(
        report.is_healthy(),
        "a missing secret is a warning, not an error"
    );
    assert_eq!(report.worst(), Some(Severity::Warning));
    assert!(
        report.to_string().contains("no application secret"),
        "{report}"
    );

    // A crash left a temporary file, and a document was set aside.
    std::fs::write(config.paths().config_dir().join(".config.toml.tmp-1"), b"x").unwrap();
    let state = DocumentStore::global(config.paths());
    std::fs::create_dir_all(state.dir()).unwrap();
    std::fs::write(state.dir().join("prefs.toml.bad"), b"junk").unwrap();
    let text = config.diagnose().to_string();
    assert!(text.contains("unfinished write"), "{text}");
    assert!(text.contains("set aside"), "{text}");
}
