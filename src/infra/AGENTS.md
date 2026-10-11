# infra

Files, network, secrets, the OS..

- Imports: `domain` and `infra`. Never `app` or `ui`, never gpui or gpui-kit.
- `ctrader` is the Open API client. A new message: wire type in `transport/messages.rs` or the
  area's `requests.rs`, a method on the area's client, a test against the mock server in
  `tests/ctrader/support`..
- `storage` owns every path the app writes. Documents are TOML with a schema version; writes are
  atomic..
- Never log or format a secret, a token or an authorization code.
- Errors are `thiserror` enums that never carry a secret value.
