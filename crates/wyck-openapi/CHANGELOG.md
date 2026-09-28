# Changelog

Changes to the `wyck-openapi` SDK. The crate follows [semantic versioning](https://semver.org):
before 1.0, a minor release (0.4 to 0.5) may break the API and a patch release does not.

When the public API changes, raise the version in `Cargo.toml` (and in the root
`[workspace.dependencies]` entry) in the same change, and add an entry here:

- a breaking change: the next minor version;
- a new item, a new field on a `#[non_exhaustive]` struct, a new variant on a `#[non_exhaustive]`
  enum, or a fix: the next patch version.

Items hidden from the docs (the `transport` module, `event::event_from`) are not covered.

## 0.4.0

The first version with its own number; earlier versions followed the Wyck app.

### Added

- The messages and market types of the former `wyck-openapi-model` crate, now ordinary modules
  of this crate.
- `trading::contract`: lots and volumes, pips, volume steps, risk sizing, profit and account
  totals (from the former `wyck-trading` crate).
- `account::book::AccountBook`: the account's positions and orders kept current from the
  server's messages (from the former `wyck-trading` crate).
- The `client` feature, on by default. Without it the crate has no network or async runtime
  dependency.
- `MarginCall::new`.
- `AccountsRes`, `CtidProfile`, `RefreshTokenRes` and `TraderAccount` at the crate root.
- Examples: `accounts`, `prices`, `history`, `demo_order`.

### Changed

- `OpenApiError` is now `Error`.
- `#[non_exhaustive]` on the server's answers and events, on `Event`, `DisconnectReason`,
  `ConnectionState`, `SessionEvent`, `SessionState`, `Tone`, `TicketProblem`,
  `ConnectionConfig` and `SessionConfig`. The wire fragments (`WireTick`, `WireTrendbar`) and the
  events the trackers take as input (`SpotEvent`, `DepthEvent`, `DepthQuote`) stay open to
  struct expressions.

### Removed

- The `handle` module (use `AccountClient` from the root), and the `session::backoff` and
  `session::token_store` modules (use `session::Backoff`, `session::TokenStore` and
  `session::MemoryTokenStore`).
- `event::error_of` and `event::order_error_of` from the public API.
- The `transport` module and `event::event_from` from the documented API.
