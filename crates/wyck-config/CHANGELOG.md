# Changelog

All notable changes to `wyck-config` are written here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The crate is versioned with the
workspace; releases before this file are in the git history.

## [Unreleased]

### Security

- Secret envelopes are now version 2: the key of the secret is signed into the ciphertext
  (associated data), so copying the file of one secret over another's no longer opens. Version 1
  envelopes are still read and are replaced by version 2 the next time the secret is stored.
- The Argon2id cost (19 MiB, 2 passes, 1 lane) is written in the envelope and bounded when it is
  read: a hostile file can no longer make the app allocate gigabytes.
- Keys and decrypted bytes are held in buffers that are wiped when dropped.
- Envelope file names now include a fingerprint of the whole key, so two keys that differ only in
  characters a file name cannot hold (`a/b`, `a_b`) no longer share a file. Envelopes named the old
  way are still found.
- Folders created by a write are `0700` on Unix, not the default of the process.
- Names of named credentials are checked (`InvalidName`) instead of being put in a file name.

### Fixed

- A damaged envelope holding a multi-byte character where hex was expected made `hex_decode`
  panic. It is an error now.
- A change of `WyckConfig` (`add_profile`, `set_active_profile`, `set_openapi_profile`,
  `set_last_symbol`, `remove_profile`) was kept in memory even when saving it failed, so the memory
  and the disk disagreed until the next restart. Changes are applied to a copy, saved, and only then
  kept.
- `add_profile` left the token it had stored behind when the profile could not be saved.
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
- `DocumentStore::list`, `list_scopes`, `load_text`, `save_text` and `scoped_checked`, so a backup
  no longer needs to know how the folders are laid out.
- `AppPaths::state_dir`, `scopes_dir` and `pictures_dir`; `WYCK_CONFIG_DIR` and `WYCK_DATA_DIR`
  move the folders.
- `WyckConfig::delete_profile_secret`, `OpenApiTokenStorage::clear`, and the `CLIENT_SECRET` and
  `OAUTH_TOKENS` constants.
- `stale_temp_files`, to find what a crash left behind.
- `ConfigError::UnsupportedSchema`, `InvalidName`, `Sealed` and `WrongPassphrase`.
- End-to-end tests (`tests/lifecycle.rs`), property tests, and fixtures of files as the first
  release wrote them.

### Changed

- `config.toml` written by a newer version is refused with `UnsupportedSchema` instead of being
  read and written back without what this version does not know.
- `WyckConfig::set_profile_secret`, `profile_secret` and `delete_profile_secret` check the name
  they are given.
- The OAuth token pair moved to its own module; the types are still exported from the crate root.

### Removed

- The dependency of the desktop app on `directories`: it asks `AppPaths::pictures_dir` instead.
