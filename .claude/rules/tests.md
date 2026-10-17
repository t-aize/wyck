---
paths:
  - "**/tests.rs"
  - "**/tests/**"
---

# Tests

- Tests sit next to the code (`#[cfg(test)] mod tests`, or a `tests.rs` beside the module) and run
  with `cargo test`; no nextest.
- A test name says the behavior, as a sentence: `a_refusal_keeps_its_code_and_drops_the_order`.
- A test does not set `wyck_core::text_scale`: other tests read it.
- Do not add a test for a getter, a constant or a derive. Do not count draw commands one by one: a
  new command would break the test without changing what the user sees.
- Tests may use `unwrap`, `expect`, `panic` and indexing; production code may not.
