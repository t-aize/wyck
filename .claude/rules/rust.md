---
paths:
  - "**/*.rs"
---

# Rust code

- Items are `pub(crate)` unless another crate uses them. A new public item of `wyck-domain` `chart` is
  re-exported flat in its `lib.rs` under a name nobody else has.
- `#[expect(lint, reason = "...")]`, not `#[allow]`.
- Concrete first, then an enum, then `dyn`, then a generic. No trait with one implementation unless a
  test double needs it; no builder under about four optional parameters; no macro under three uses.
- Errors: `thiserror` enums in libraries, `anyhow` only in `xtask` and the `editor` module of `wyck-ui`.
- English and plain ASCII in code and comments. A `//!` is 1 to 3 lines, a `///` says a unit, an
  encoding, a panic or a reason and never repeats the name.
- Format with `cargo fmt --all`; a file of production code stays under about 600 lines, or says why
  in its module doc.
