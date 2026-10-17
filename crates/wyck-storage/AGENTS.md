# wyck-storage

The one place that decides where the app's files live, how they are written, and how anything
secret is kept: the config, the documents (layouts, drawings, favorites), the indicator scripts, the
backups, the credentials and the encryption behind them. No screen, no network.

Depends on: `wyck-clock`, `wyck-platform`, `serde`, `serde_json`, `toml`, `secrecy`, `zeroize`, `keyring`,
`directories`, `argon2`, `chacha20poly1305`, `getrandom`, `uuid`, `thiserror`, `tracing`
(`proptest` and `tempfile` in tests only).

## Module map

The API is `WyckConfig` and the types re-exported at the root; six modules are public because the
app names them.

| Module | What it holds |
|---|---|
| (root) | `WyckConfig` (open it, add profiles, read secrets, `documents()`, `backups()`, `diagnose()`), `AppConfig`, `ProfileConfig`, `ProfileId`, `AppPaths`, `DocumentStore`, `SecretStore` and `SecretKey`, `ConfigError` and `Result`, `atomic_write`. |
| `paths` | `AppPaths`: the standard folders of the system, or the ones the environment or the user chose. |
| `locations`, `relocate` | Where each kind of data lives and the file that says so (public): `kind` (the five kinds), `pointer` (`locations.toml`), `folder` (can a folder be used, its marker), `cloud` (cloud folders, sync conflict copies). Moving a kind to another folder, staged and made at the next start (public): `planning` (what a move would do), `staging` (the file that waits), `applying` (the start-up move), `copying` (copy and read back), `left_behind` (old folders kept aside). |
| `identity` | The names a build keeps its state under, so a development build never shares a folder or a credential with the installed app (public). |
| `documents` | `DocumentStore`: typed TOML documents, one file each, with a `schema_version`. |
| `migrate` | Upgrading a saved file step by step when its layout changes (public). |
| `backup` | `Backup` (everything the user made, in one file), `store::BackupStore` (the copies kept in `backups/`), `staged` (export, import, reset: the change waits for the next start), `kind` (why a copy was made) (public). |
| `scripts` | `ScriptStore`: the folder of indicator scripts, which names they may have (public). |
| `secret`, `tokens` | `SecretStore` with two backends (`KeyringSecretStore`, `EncryptedFileSecretStore`); the OAuth token pair of a profile as one credential. |
| `crypto`, `sealed` | Argon2id key derivation and ChaCha20-Poly1305 encryption, in one place; text sealed with a passphrase to carry a backup away. |
| `names`, `fs_util`, `doctor`, `app_config`, `profile`, `error` | The rule for names that become paths; the atomic durable write; the check-up (`Report`); the config file; a profile; the error type. |

## Invariants and traps

- **A token never appears in `config.toml`.** The file holds a profile's public settings; a
  `SecretKey` derived from the profile id says where the secret is looked up. Secrets are
  `secrecy::SecretString` in memory (never printed by `Debug`), and errors and logs carry the key,
  never the value.
- **Every write is atomic and durable** (`atomic_write`): a crash leaves the old file or the new one.
  Never write a file of the app any other way.
- **The memory never gets ahead of the disk**: a change of `WyckConfig` is saved before it is kept.
- **A damaged file never takes the app down** (`DocumentStore::load_or_default` sets it aside as
  `.bad`); **a file from a newer version is left alone**; **an older one is upgraded** by `migrate`,
  keeping the old text as `.v<n>.bak`. Raising a `SCHEMA_VERSION` needs a step in `migrate`.
- **Names cannot leave their folder**: any name that becomes part of a path goes through `names`
  (ASCII letters, digits, `-` and `_`, at most 100 characters).
- **Moving data is staged**: `relocate` and `backup::staged` never write over a running app. The next
  start applies them, before anything reads the documents, after saving what they replace.
- `config.toml` and the credentials never move. Settings, account data, scripts, backups and logs
  can each live in a folder the user chose (for example one a cloud service syncs).
- The environment can move things: `WYCK_CONFIG_DIR`, `WYCK_DATA_DIR`, `WYCK_KEYRING_SERVICE`,
  `WYCK_PASSPHRASE` (credentials in encrypted files, for a machine with no keyring).

## Global state

None: every function takes the paths or the stores it works on. The environment variables above
are read in `paths` and `identity`.

## Tests

Unit tests next to the code, on temporary folders, plus property tests (`proptest`) for the names
and the encryption. Migrations, crypto and secrets, the atomic write and the staged moves are the
tests that must never be weakened. A few methods exist only for tests (`#[cfg(test)]`): they are
candidates for deletion with their tests in Phase 6. Run: `cargo test -p wyck-storage`.

## Where to add X

- A new document: a type with a `schema_version` and a `DocumentStore` name; its steps in `migrate`.
- A new kind of data with its own folder: `locations::Kind`, `paths`, and the move in `relocate`.
- A new secret: a `SecretKey` under a new namespace, never a field of the config.
- A new setting of the config: `app_config`, a default, and a migration if a file already exists.

## Do not

- Do not use `tokio`, `reqwest`, `gpui` or any screen crate: `cargo xtask deps` fails on them.
- Do not write a file without `atomic_write`, nor build a path from a name that did not go through
  `names`.
- Do not put a secret in an error, a log line, the config or a document.
- Do not depend on any crate above `wyck-platform`.
