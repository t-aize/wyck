# infra

Files, network, secrets, the OS.

- Imports: `domain` and `infra`. Never `app` or `ui`, never gpui or gpui-kit.
- `ctrader` is the Open API client. A new message: wire type in `transport/messages.rs` or the
  area's `requests.rs`, a method on the area's client, a test against the mock server in
  `tests/ctrader/support`.
- `storage` owns every path the app writes. Documents are TOML with a schema version; writes are
  atomic.
- A debug build never shares a folder or a keyring service with the installed app: names come
  from `storage::identity` and the build mode. A new place the app writes to, or a new
  credential namespace, must take its name from there, never from a literal `"wyck"`.
- Never log or format a secret, a token or an authorization code.
- Errors are `thiserror` enums that never carry a secret value.
