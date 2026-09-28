# wyck-config

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
- [Backups and export](#backups-and-export)
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
| Text to carry to another machine, unreadable without a passphrase | `sealed::seal_text`, `sealed::open_text` | a small TOML document |
| Files written without ever leaving half a file | `write_atomically`, `atomic_write` | any path |
| Names that can never leave their folder | `names::validate_name`, `names::sanitize` | none |
| "Is my config healthy?" | `WyckConfig::diagnose` | none |

## Quick start

```rust
use secrecy::SecretString;
use wyck_config::WyckConfig;

// The standard folders of the OS, credentials in the OS keyring.
let mut config = WyckConfig::open()?;

let id = config.add_profile(
    "Live: FTMO 100k",
    "ctrader-openapi",
    None,
    Some(SecretString::from("the-account-token".to_owned())),
)?;
config.set_active_profile(Some(id.clone()))?;

if let Some(token) = config.token_for(&id)? {
    // hand `token` to the client that needs it
}
```

The builder changes the folders or the credential store:

```rust
// A portable install, and a machine with no keyring: credentials in encrypted files.
let config = WyckConfig::builder()
    .portable("./wyck-data")
    .encrypted_file(SecretString::from(passphrase))
    .build()?;
```

Documents are typed, and the crate never looks inside them:

```rust
let store = config.global_documents();               // shared by every account
let account = config.scoped_documents("demo-4242");  // one folder per account

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
  indicators/            scripted indicators (owned by wyck-chart)
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
- the cost is written in the file, so it can be raised later without breaking older files, and a
  hostile file cannot ask for gigabytes (the cost is bounded before anything is allocated);
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
  `add_profile` removes a token it stored if the profile could not be saved;
  `remove_profile` deletes the credentials first and keeps the profile when one refuses to go,
  so the call can be repeated.
- **A file from a newer version is left alone.** `config.toml` with a higher `schema_version`
  is refused (`UnsupportedSchema`) instead of being read wrong and written back without what this
  version does not know.
- **Names cannot leave their folder.** Names of documents, scopes and named credentials are 1 to
  100 characters of `A-Z a-z 0-9 - _`. No dot, no separator: two different names never share a
  file.

## Backups and export

The crate gives the pieces; the app decides what goes in a backup.

```rust
// Collect the text of every document, as it is on disk (comments and all).
for name in store.list()? {
    let text = store.load_text(&name)?.unwrap();
    // ...
}
for scope in DocumentStore::list_scopes(&paths)? { /* the same, per account */ }

// Seal it with a passphrase to carry it somewhere it must not be readable.
let file = wyck_config::sealed::seal_text(&passphrase, "wyck-backup", &text)?;
let text = wyck_config::sealed::open_text(&passphrase, "wyck-backup", &file)?;

// Restore: the text is checked to be TOML, then written atomically.
store.save_text(&name, &text)?;
```

- `sealed::is_sealed(text)` tells whether a file needs a passphrase, before asking for one.
- The label (`"wyck-backup"`) is signed into the file: a sealed backup cannot be passed off as
  another kind of document.
- `open_text` tells a **wrong passphrase** (`ConfigError::WrongPassphrase`) from a **damaged or
  foreign file** (`ConfigError::Sealed`).
- Credentials are never part of a backup: they stay in the keyring or in `secrets/`.

`tests/lifecycle.rs` holds a complete example (collect, seal, open elsewhere, restore).

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
cargo run -p wyck-config --example config_doctor                    # this machine
cargo run -p wyck-config --example config_doctor -- --dir ./data    # a portable install
WYCK_PASSPHRASE=... cargo run -p wyck-config --example config_doctor -- --dir ./data
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

A secret envelope (`secrets/<key>-<fingerprint>.toml`), version 2:

```toml
version = 2
memory_kib = 19456
iterations = 2
parallelism = 1
salt = "..."        # hex, 16 bytes
nonce = "..."       # hex, 12 bytes
ciphertext = "..."  # hex
```

Version 1 envelopes (no cost written down, key not signed, file named after the readable part of
the key only) are still read, and replaced by version 2 the next time the secret is stored. A
sealed document is described in the docs of the `sealed` module.

## Errors

Every fallible call returns `wyck_config::Result<T>`. `ConfigError` says what failed and where
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

## Development

```sh
scripts/check-config.sh          # fmt, clippy -D warnings, tests, docs -D warnings
cargo test -p wyck-config        # 78 unit tests, 9 end-to-end tests, doc tests
```

- `src/**` unit tests sit next to the code they test; `tests/lifecycle.rs` uses the public API
  only, the way an app does (install, restart, damage, upgrade, backup).
- Properties (names, hex, the cipher) are checked with `proptest`.
- `tests/fixtures/` holds files as earlier versions wrote them; a test opens each one. **Never
  edit a fixture to make a test pass**: a fixture that stops opening means users' files would.
- Tests work in temporary folders and never touch the real config or keyring.
- `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]`: every public item is documented.

## Compatibility

- Files: everything an earlier release wrote is read. `config.toml` carries a `schema_version`
  and secret envelopes carry a `version`; a newer one is refused, never misread.
- API: the crate is part of the Wyck workspace and versioned with it (see the
  [changelog](CHANGELOG.md)). Its public API is what the other crates use, and a change to it is
  listed there.
- Rust: the version in the workspace `Cargo.toml` (`rust-version`).

License: Apache-2.0, like the rest of the workspace.
