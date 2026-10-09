# Storage (src/infra/storage)

Configuration, encrypted credentials and portable backups for [Wyck](../../README.md).

This crate is the one place the app decides **where its files live**, **how they are written**
and **how anything secret is kept**. Everything else in the workspace (the desktop app, the chart
engine, the Open API client) goes through it instead of touching the disk or the keyring on its own.

It knows nothing about cTrader, charts or drawings: a profile's `service` is a free-form tag, and
the shape of every saved document belongs to the code that owns the data. That is what keeps it
reusable.

- [What is in it](#what-is-in-it)
- [Quick start](#quick-start)
- [Where the files go](#where-the-files-go)
- [Security model](#security-model)
- [Guarantees](#guarantees)
- [Backups, export and import](#backups-export-and-import)
- [Indicator scripts](#indicator-scripts)
- [Checking an install](#checking-an-install)
- [File formats](#file-formats)
- [Errors](#errors)
- [Development](#development)
- [Compatibility](#compatibility)

## What is in it

| Need | Use | Stored in |
|---|---|---|
| Which profiles exist, which one is active | `WyckConfig`, `AppConfig`, `ProfileConfig` | `config.toml` (plain TOML) |
| Tokens, client secrets, OAuth pairs | `SecretStore`: `KeyringSecretStore` or `EncryptedFileSecretStore` | the OS keyring, or one encrypted file per secret |
| Anything else the app remembers (layouts, favorites, drawings) | `DocumentStore` | one TOML file per document |
| Indicator scripts (plain text files) | `scripts::ScriptStore` | one `.rhai` file per indicator |
| A backup of everything, to export, import, keep or restore | `backup::Backup`, `backup::BackupStore` | one TOML file, in `backups/` or wherever the user puts it |
| Text to carry to another machine, unreadable without a passphrase | `sealed::seal_text`, `sealed::open_text` | a small TOML document |
| Files written without ever leaving half a file | `atomic_write` | any path |
| Names that can never leave their folder | `names::validate_name`, `names::sanitize` | none |
| "Is my config healthy?" | `WyckConfig::diagnose` | none |

## Quick start

```rust
use secrecy::SecretString;
use wyck::infra::storage::{CLIENT_SECRET, WyckConfig};

// The standard folders of the OS, credentials in the OS keyring.
let mut config = WyckConfig::open()?;

let id = config.add_profile("Live: FTMO 100k", "ctrader-openapi")?;
config.set_profile_secret(&id, CLIENT_SECRET, &SecretString::from("the-secret".to_owned()))?;
config.set_active_profile(Some(id.clone()))?;

if let Some(secret) = config.profile_secret(&id, CLIENT_SECRET)? {
    // hand `secret` to the client that needs it
}
// The OAuth token pair of the profile has its own handle, movable to another thread:
let tokens = config.openapi_token_storage(&id);   // .load() / .save(..) / .clear()
```

The builder changes the folders or the credential store:

```rust
// A portable install, and a machine with no keyring: credentials in encrypted files.
let config = WyckConfig::builder()
    .portable("./wyck-data")
    .encrypted_file(SecretString::from(passphrase))
    .build()?;
```

Every store is one call away, from `WyckConfig` or from `AppPaths` (which is what code that
starts before the config is loaded, or has no profiles, holds):

```rust
config.documents();        // paths.documents():   shared by every account
config.scope("demo-4242"); // paths.scope(..):     one folder per account
config.scripts();          // paths.scripts():     the indicator scripts
config.backups();          // paths.backups():     the copies kept in backups/
```

Documents are typed, and the crate never looks inside them:

```rust
let store = config.documents();                  // = DocumentStore::global(config.paths())
let account = config.scope("demo-4242");         // = DocumentStore::scoped(config.paths(), ..)

store.save("layout", &my_layout)?;                   // atomic
let layout: MyLayout = store.load_or_default("layout"); // never fails
```

`load_or_default` reads the document, and if it is damaged, sets it aside as `layout.toml.bad` and
starts from `Default`: a corrupt file costs the user that document, never the app.

## Where the files go

`AppPaths::discover()` asks the operating system, unless the environment says otherwise:

| Variable | Effect |
|---|---|
| `WYCK_CONFIG_DIR` | moves the config folder (and the data folder with it) |
| `WYCK_DATA_DIR` | moves the data folder alone |

| OS | Config folder |
|---|---|
| Linux | `~/.config/wyck` |
| macOS | `~/Library/Application Support/sh.wyck.wyck` |
| Windows | `%APPDATA%\wyck\config` |

```text
<config>/
  config.toml            profiles, active profile, last symbol
  state/                 documents shared by every account      (DocumentStore::global)
  scopes/<scope>/        documents of one account               (DocumentStore::scoped)
  indicators/            indicator scripts, one .rhai file each  (scripts::ScriptStore)
  backups/               saved copies, one file each             (backup::BackupStore)
  pending-import.toml    a checked import waiting for the next start (temporary)
  reset-pending          a reset waiting for the next start           (temporary)
<data>/
  secrets/               encrypted envelopes, only with EncryptedFileSecretStore
```

## Security model

**What is protected.** A token or a secret never appears in `config.toml`, in a document, in a
log line or in an error message. In memory it is a `secrecy::SecretString`: wiped on drop, never
printed by `Debug`. Errors carry the *key* a secret is stored under (which is not sensitive), never
the value.

**The OS keyring** (`KeyringSecretStore`, the default) is the recommended store: the operating
system owns the keys, and there is no passphrase to manage.

**Encrypted files** (`EncryptedFileSecretStore`) are for machines with no keyring:

- a key is derived from the passphrase with **Argon2id** (19 MiB, 2 passes, 1 lane: the OWASP
  minimum), with a fresh 16-byte salt for every secret;
- the secret is encrypted with **ChaCha20-Poly1305** and a fresh 12-byte nonce, so a wrong
  passphrase or a changed bit is detected instead of decrypting to garbage;
- the **key of the secret is signed into the ciphertext** (associated data): copying the file of
  one secret over another's makes opening fail;
- the cost is written in the file, so it can be raised later without a new format, and a hostile
  file cannot ask for gigabytes (the cost is bounded before anything is allocated);
- keys and decrypted bytes live in buffers that are wiped when dropped;
- files are written with `0600` permissions, in folders created `0700` (Unix).

**What is not protected.** Anyone who can read the passphrase (or the memory of the running
process) can read the secrets. The passphrase is never stored by this crate: prompting for it is
the job of the front end. Windows has no `0600`; the files inherit the ACL of the per-user folder.
The crate does not lock files: one process should own a config at a time.

## Guarantees

- **Writes are atomic and durable.** Every file goes to a temporary sibling, is flushed
  (`fsync`), renamed over the target, and the folder is flushed too. A crash or a power cut leaves
  the old file or the new one, never half of one. Unfinished temporary files are found by
  `stale_temp_files` and by the check-up.
- **The memory never gets ahead of the disk.** A change of `WyckConfig` is applied to a copy,
  saved, and only then kept. When the save fails, the change did not happen.
  `remove_profile` deletes the credentials first and keeps the profile when one refuses to go,
  so the call can be repeated.
- **A file this build does not know is left alone.** `config.toml` with a higher `schema_version`
  is refused (`UnsupportedSchema`), and one without a version is a parse error: it is never read
  wrong and written back without what this version does not know.
- **Names cannot leave their folder.** Names of documents, scopes and named credentials are 1 to
  100 characters of `A-Z a-z 0-9 - _`. No dot, no separator: two different names never share a
  file. Script ids follow the rule of `scripts::clean_id`, and a backup is refused whole when one
  name in it breaks either rule.
- **A restore or a reset can be undone.** What it replaces is saved as a copy first, and when that
  fails nothing is touched.

## Backups, export and import

A backup is **one TOML file** with the text of every document (global and per account) and every
indicator script, as they are on disk. It never holds credentials or `config.toml`, so it is safe
in a cloud folder; add a passphrase and it is unreadable there too. The whole life of a backup is
in `wyck::infra::storage::backup`:

```rust
use wyck::infra::storage::backup::{self, AutoPolicy, Backup, BackupKind};

// Export: one call. `None` for a plain file, `Some(&passphrase)` to seal it.
backup::export_to_file(config.paths(), Some(&config.scripts()), &dest, None, "1.0", &now)?;

// Import: checked, then staged. Nothing is written until the app starts again.
let what: Backup = backup::stage_import_file(config.paths(), &file, None)?;
println!("{}", what.contents().documents());   // what it holds, to show before the restart

// At the start of the app, before anything reads a document:
let applied = backup::apply_pending(config.paths(), &now)?;
```

**The copies the app keeps** are in `backups/`, behind `BackupStore` (`config.backups()`):

```rust
let store = config.backups();

store.create(&Backup::collect(config.paths(), Some(&config.scripts()), "1.0", &now)?)?;
store.auto_snapshot(Some(&config.scripts()), "1.0", &now, AutoPolicy::default())?; // 1 a day, last 7

for entry in store.list()? {                       // newest first
    println!("{} {:?} {} bytes sealed={}", entry.id, entry.kind, entry.bytes, entry.sealed);
}
store.restore(&id, None)?;                         // staged, like an import
store.export(&id, &somewhere)?;                    // carry a copy away as it is
store.remove(&id)?;
store.prune(BackupKind::Automatic, 5)?;
```

| Kind | Made by | Name |
|---|---|---|
| `Manual` | `create`, or a file dropped in the folder | `backup-<date>` |
| `Automatic` | `auto_snapshot` (at most one per interval; the oldest go) | `auto-<date>` |
| `BeforeImport` | an import, right before it replaces a document or a script that differs | `before-import-<date>` |
| `BeforeReset` | a reset, right before it removes the documents | `before-reset-<date>` |

**Restoring never writes over a running app.** `stage_import` checks the backup and puts it aside
(`pending-import.toml`); `stage_reset` leaves a marker. The next start calls `apply_pending`,
before anything is loaded. It first saves what it is about to replace as a `BeforeImport` or
`BeforeReset` copy, so an import or a reset can itself be undone with `restore`. When that copy
cannot be saved, nothing is touched and the error comes back. An import that cannot be read is
set aside as `.bad`, and the last one asked for wins (a reset cancels a waiting import and the
other way round). A reset leaves `config.toml`, the credentials and the scripts alone.

- A backup is checked whole before anything is staged: the format and version (a newer one is
  refused, `BackupTooNew`), every document and script name (they are paths: `..`, separators and
  device names are refused), duplicates, sizes and counts, and that every document is valid TOML.
- `Backup::read` opens a plain or a sealed file: `PassphraseRequired` when a sealed one gets no
  passphrase, `WrongPassphrase` when it does not open, `NotABackup`/`BackupDamaged` otherwise.
- The label of a sealed backup (`"wyck-backup"`) is signed into it: a backup cannot be passed off as
  another kind of sealed document, and the other way round.
- `Backup::contents()` says what is inside by name (`Contents`), so a front end can phrase it
  ("Drawings for 2 accounts") without this crate knowing what a drawing is.
- `sealed::seal_text` and `sealed::open_text` are still there for any other text.

From the command line, on this machine or a portable install (`--dir`):

```sh
cargo run --example config_backup -- list
cargo run --example config_backup -- export ./my-backup.toml
WYCK_BACKUP_PASSPHRASE=... cargo run --example config_backup -- export ./locked.toml
cargo run --example config_backup -- import ./my-backup.toml   # then start the app
```

`tests/lifecycle.rs` holds complete examples (export sealed, import elsewhere, undo a reset).

## Indicator scripts

An indicator is a text file `<id>.rhai`. Its id is its path from the scripts folder without the
extension (`trend/my average`), which is what a chart saves to remember its indicators.
`scripts::ScriptStore` (`config.scripts()` for the default folder, `ScriptStore::new(dir)` for a
folder the user chose) owns what has to be the same everywhere the files are touched:

- the rule for names (`clean_id`: letters, digits, spaces and `- _ . ( ) % + , &`, 64 characters a
  part, at most 3 folders deep, no dot at either end, no Windows device names) and the limits
  (256 KB a script, 500 scripts);
- `scan`, `ids`, `read_all`, `read`: hidden files, the `.trash` folder, links and files of another
  kind are not scripts, and a file that is too big or not text is left out of a copy;
- `write`: atomic, and the id and size are checked; `export_all` copies the whole folder.

It does not know the language: compiling and running scripts is `wyck-chart`, which reads and
writes its library through this type. The backup takes the scripts from here too.

## Checking an install

```rust
let report = config.diagnose();
if !report.is_healthy() {
    eprintln!("{report}");
}
```

It reports, changing nothing: a dangling active profile, duplicated ids, Open API profiles with no
stored client secret or no chosen account, credentials the store cannot read, config and secrets
folders readable by other users, unfinished writes left by a crash, and documents that had to be
set aside.

From the command line:

```sh
cargo run --example config_doctor                    # this machine
cargo run --example config_doctor -- --dir ./data    # a portable install
WYCK_PASSPHRASE=... cargo run --example config_doctor -- --dir ./data
scripts/check-config.sh doctor                                      # the same, from the repo root
```

The exit code is 0 (healthy), 1 (an error was found) or 2 (the config could not be opened).

## File formats

`config.toml`, written by `AppConfig::save`:

```toml
schema_version = 1
active_profile = "8f0c1c0e-6d0e-4c0b-9d6e-3f7f6f3c9a11"
last_symbol = "EURUSD"

[[profiles]]
id = "8f0c1c0e-6d0e-4c0b-9d6e-3f7f6f3c9a11"
display_name = "Demo account"
service = "ctrader-openapi"
client_id = "public-client-id"
callback_port = 52123
account_id = 12345678
```

A secret envelope (`secrets/<key>-<fingerprint>.toml`):

```toml
version = 1
memory_kib = 19456
iterations = 2
parallelism = 1
salt = "..."        # hex, 16 bytes
nonce = "..."       # hex, 12 bytes
ciphertext = "..."  # hex
```

Every field is required. There is one format and no migration: a file with another `version`, or
with a field missing, is refused. A sealed document is described in the docs of the `sealed`
module.

## Errors

Every fallible call returns `wyck::infra::storage::Result<T>`. `ConfigError` says what failed and where
(the path or the secret key), never the secret. The ones worth handling by name:

| Variant | Meaning | What a front end does |
|---|---|---|
| `Parse` | a file is not valid TOML | offer to repair or reset; the file is untouched |
| `UnsupportedSchema` | written by a newer version | tell the user to update |
| `Crypto` | an envelope did not open | ask for the passphrase again |
| `WrongPassphrase` | a sealed document did not open | ask for the passphrase again |
| `UnknownProfile` | an id that is not configured | refresh the list |
| `InvalidName` | a name that could leave its folder | reject the input |
| `SecretStore` | the OS keyring failed | suggest `EncryptedFileSecretStore` |
| `NotABackup`, `BackupDamaged` | a file is not a backup, or a part of it is wrong | say so, change nothing |
| `BackupTooNew` | made by a newer version | tell the user to update |
| `PassphraseRequired` | a sealed backup and no passphrase | ask for one |
| `BackupNotFound` | an id that is not in `backups/` | refresh the list |

## Development

```sh
scripts/check-config.sh          # fmt, clippy -D warnings, tests, docs -D warnings
cargo test --test storage_lifecycle        # unit tests, end-to-end tests, doc tests
```

- `src/**` unit tests sit next to the code they test; `tests/lifecycle.rs` uses the public API
  only, the way an app does (install, restart, damage, refuse a foreign format, backup).
- Properties (names, hex, the cipher) are checked with `proptest`.
- Tests work in temporary folders and never touch the real config or keyring.
- `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]`: every public item is documented.

## Compatibility

- **Formats.** There has been no release yet, so the formats are not frozen and there is no
  migration code: `config.toml`, the secret envelopes and the sealed documents are read only in the
  layout this build writes. Once a first release is out, a change of layout will bump the version
  written in the file, and an older build will refuse a newer file instead of misreading it.
- **API.** The crate is part of the Wyck workspace and versioned with it (see the
  [changelog](CHANGELOG.md)).
- **Rust.** The version in the workspace `Cargo.toml` (`rust-version`).

License: Apache-2.0, like the rest of the workspace.
