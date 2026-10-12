# infra

Files, network, secrets, the OS.

- Imports: `domain` and `infra`. Never `app` or `ui`, never gpui or gpui-kit.
- `ctrader` is the Open API client. A new message: wire type in `transport/messages.rs` or the
  area's `requests.rs`, a method on the area's client, a test against the mock server in
  `tests/ctrader/support`.
- `storage` owns every path the app writes. Documents are TOML with a schema version; writes are
  atomic. A layout that changes goes through `storage::migrate` (steps in order, the old text kept
  as `.v<n>.bak`, a file from a newer version set aside, never rewritten).
- Five kinds of data can live in a folder the user chose (`storage::locations::Kind`); the choice
  is `locations.toml` in the config folder, moves are made at the next start by
  `storage::relocate`, and a moved folder carries a marker of the build and the kind. Read a
  folder through `AppPaths::dir(kind)`, never from `config_dir()`, so a moved kind is followed.
  Credentials and `config.toml` never move.
- The log files and the panic reports are written by `platform::logging` into
  `AppPaths::logs_dir`, never into the settings folder. A new `tracing` event follows the rule
  below: no secret, token or code in a field or a message.
- A debug build never shares a folder or a keyring service with the installed app: names come
  from `storage::identity` and the build mode. A new place the app writes to, or a new
  credential namespace, must take its name from there, never from a literal `"wyck"`.
- Never log or format a secret, a token or an authorization code.
- Errors are `thiserror` enums that never carry a secret value.
