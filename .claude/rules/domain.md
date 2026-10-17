---
paths:
  - "crates/wyck-core/**"
  - "crates/wyck-market/**"
  - "crates/wyck-trading/**"
  - "crates/wyck-drawings/**"
  - "crates/wyck-indicators/**"
  - "crates/wyck-chart/**"
---

# Domain crates

These crates are pure: no window, no file, no socket, no `gpui`, no `tokio`, no `reqwest`, no
`keyring`. `cargo xtask deps` fails on such a dependency.

- A price is the server's raw integer until the last step; a float is for the screen.
- Everything read from a file or from the wire goes through `normalized`; a value this version does
  not know becomes a default, never an error.
- A saved name or code (`Tool`, `ChartKind`, a serde field) is never renamed without an alias and a
  migration.
- Work per frame is bounded by the screen, not by the data (`wyck-chart`).
- Money maths and the order guard (`wyck-trading`) are not weakened to make a test pass.
