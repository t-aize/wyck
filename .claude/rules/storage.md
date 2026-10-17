---
paths:
  - "crates/wyck-storage/**"
  - "crates/wyck-app/src/migrations.rs"
  - "crates/wyck-app/src/fixtures/**"
---

# Files, secrets and migrations

- Read a folder through `AppPaths`, never from a path you built: the user can move any kind of data.
- Every write goes through `atomic_write`; a failed write leaves the old file.
- A name that comes from outside goes through `names` (no path separators, no dots, no spaces).
- A secret is a `SecretString`, is never logged, and is never written to `config.toml`.
- Raising a `SCHEMA_VERSION` needs a step in the plan of `wyck-app/src/migrations.rs` and a fixture
  of the old file; a test fails otherwise.
- A file from a newer version is set aside whole, never read and written back without what this
  version does not know.
