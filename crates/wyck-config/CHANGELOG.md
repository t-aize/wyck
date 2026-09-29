# Changelog

All notable changes to `wyck-config` are written here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate is versioned with the
workspace. There has been no release yet: formats are not frozen and carry no migration code.

## [Unreleased]

### Security

- The key of a secret is signed into its ciphertext (associated data), so copying the file of one
  secret over another's does not open.
- The Argon2id cost (19 MiB, 2 passes, 1 lane) is written in the envelope and bounded when it is
  read: a hostile file can no longer make the app allocate gigabytes.
- Keys and decrypted bytes are held in buffers that are wiped when dropped.
- Envelope file names include a fingerprint of the whole key, so two keys that differ only in
  characters a file name cannot hold (`a/b`, `a_b`) never share a file.
- Folders created by a write are `0700` on Unix.
- Names of named credentials are checked (`InvalidName`) instead of being put in a file name.

### Fixed

- A damaged envelope holding a multi-byte character where hex was expected made `hex_decode`
  panic. It is an error now.
- A change of `WyckConfig` (`add_profile`, `set_active_profile`, `set_openapi_profile`,
  `set_last_symbol`, `remove_profile`) was kept in memory even when saving it failed, so the memory
  and the disk disagreed until the next restart. Changes are applied to a copy, saved, and only then
  kept.
- `remove_profile` removed the profile from memory before deleting its credentials; a credential
  that could not be deleted left a profile with no credentials. The credentials go first now, and
  the profile stays when one refuses, so the call can be repeated.
- Writes were atomic but not durable: a power cut could leave a renamed file with no contents. The
  file and its folder are flushed (`fsync`) now.

### Added

- `WyckConfig::open()` and `WyckConfig::builder()` (`ConfigBuilder`: `paths`, `portable`,
  `keyring_service`, `encrypted_file`, `secret_store`).
- `WyckConfig::diagnose()` and the `Report`, `Finding` and `Severity` types; the `config_doctor`
  example and `scripts/check-config.sh`.
- The `sealed` module: `seal_text`, `open_text` and `is_sealed`, to encrypt an export or a backup
  with a passphrase (Argon2id and ChaCha20-Poly1305, with a label that cannot be swapped).
- The `names` module: the one rule for names that become part of a path.
- `DocumentStore::list`, `list_scopes`, `load_text` and `save_text`, so a backup
  no longer needs to know how the folders are laid out.
- `AppPaths::state_dir`, `scopes_dir`, `backups_dir` and `pictures_dir`; `WYCK_CONFIG_DIR` and `WYCK_DATA_DIR`
  move the folders.
- `WyckConfig::delete_profile_secret`, `OpenApiTokenStorage::clear`, and the `CLIENT_SECRET` and
  `OAUTH_TOKENS` constants.
- `stale_temp_files`, to find what a crash left behind.
- `ConfigError::UnsupportedSchema`, `InvalidName`, `Sealed` and `WrongPassphrase`.
- End-to-end tests (`tests/lifecycle.rs`) and property tests.

### Changed

- **One strict format, no migration.** `config.toml` requires `schema_version` and refuses a
  higher one (`UnsupportedSchema`); a secret envelope requires every field, including the
  Argon2id cost, and only `version = 1` is read. Nothing written by an earlier layout is read,
  and the file name of an envelope no longer has a fallback to the old naming.
- `WyckConfig::set_profile_secret`, `profile_secret` and `delete_profile_secret` check the name
  they are given.
- The OAuth token pair moved to its own module; the types are still exported from the crate root.

### Removed

- The redundant parts of the API, so there is one way to do each thing:
  the profile's main token (`add_profile` no longer takes a token or an endpoint, and `token_for`,
  `SecretKey::for_profile` and `ProfileConfig::endpoint` are gone; credentials are named secrets and
  the OAuth pair); `WyckConfig::global_documents`, `scoped_documents`, `openapi_tokens` and
  `save_openapi_tokens` (use `DocumentStore::global(config.paths())` and
  `config.openapi_token_storage(id)`); `write_atomically` (`atomic_write` is the only writer, and
  `std::io::Error` converts from `ConfigError`); `DocumentStore::scoped_checked` (validate with
  `names::validate_name`).

- The dependency of the desktop app on `directories`: it asks `AppPaths::pictures_dir` instead.
