# wyck-clock

The one place the workspace reads the wall clock. Nothing else calls `SystemTime::now`,
`chrono::Local::now` or `chrono::Utc::now`: `clippy.toml` forbids them, and the single exception is
`SystemClock::now` in this crate.

Depends on: nothing (std only).
Used by: `wyck`, `wyck-app`, `wyck-ctrader`, `wyck-platform`, `wyck-storage`, `wyck-ui`,
`xtask`.

## Module map

| Item | What it is |
|---|---|
| `Clock`, `SharedClock` | The trait (`now() -> SystemTime`) and `Arc<dyn Clock>`. |
| `SystemClock` | The clock of the machine, the default. |
| `ManualClock` | A clock that moves only on `set` and `advance`, for tests. |
| `now`, `unix_ms` | The current time, from the clock of the thread, else the installed one, else the system's. |
| `with_clock` | Another clock for this thread for the length of a closure (tests). |
| `install` | Another clock for the process, once, at start. |

## Invariants and traps

- **Calendar time only.** `Instant` (timeouts, rate limits, polling) stays on the monotonic clock
  and is not routed here: it cannot be set and has no date.
- `with_clock` is per thread. Work a test hands to another thread or to the async runtime reads the
  installed or the system clock; test the pure function with the time as an argument instead.
- A clock set before the epoch reads as 0 in `unix_ms`.

## Tests

Unit tests next to the code. Run: `cargo test -p wyck-clock`.

## Do not

- Do not read the wall clock anywhere else, and do not add `#[expect(clippy::disallowed_methods)]`
  for it: call `wyck_clock::now()`.
- Do not depend on any other crate, not even of the workspace: every layer uses this one.
