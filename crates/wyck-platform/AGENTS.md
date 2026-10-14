# wyck-platform

What the operating system gives the app and nothing else: the async runtime tokio-based code needs,
the facts of the build, and the log files. It knows no screen, no account and no folder of the
user: the folder of the logs is handed to it.

Depends on: `tokio`, `tracing`, `tracing-appender`, `tracing-subscriber`, `chrono`, `thiserror`
(`semver` and `tempfile` in tests only). No other crate of the workspace.

## Module map

| Module | What it holds |
|---|---|
| `build_info` | `BuildMode` (`Development` or `Production`, fixed by `debug_assertions`) and `VERSION` (the version of the workspace). |
| `logging` | `init` (stderr and one log file per day), `flush`, `install_panic_hook` (a report file per panic), `reports` (the report files of a folder). |
| `runtime` | `handle` and `spawn`: one multi-thread tokio runtime on a thread of its own, for the code that needs tokio (the cTrader client, the OAuth flow) in an app whose executor is gpui's. Also re-exports `RecvError`, `JoinError` and `try_join`, so that no screen names tokio. |

## Invariants and traps

- `BuildMode::CURRENT` follows the Cargo profile and cannot change at run time. It is what keeps a
  development build and the installed app apart (`wyck-storage` names the folders and the keyring
  service after it).
- Neither a log line nor a panic report may carry a secret. The code that logs formats ids and
  messages, never a token or a passphrase.
- Log writes go through a channel to a worker thread, so a slow disk never freezes a chart; when
  the channel is full lines are dropped. `flush` writes out what is waiting: call it when the app
  quits.
- A panic report is written by the thread that panics, not by the worker, so it exists even when
  the process dies a moment later.
- `runtime::handle` ends the process (a message on stderr, exit code 1) if the system refuses to
  start the runtime: nothing in the app works without it. That is the only `process::exit` of the
  workspace, and its lint expectation says why.

## Global state (on purpose)

- `runtime`: the runtime handle in a `OnceLock`. The `Runtime` itself is leaked on purpose, so its
  threads live as long as the process.
- `logging`: the `WorkerGuard` of the log file in a `Mutex` (dropping it stops the worker), a
  counter of reports written (it names them), and the global `tracing` subscriber, which `init`
  sets once.

## Tests

Unit tests next to the code; the log tests write to a temporary folder. Run:
`cargo test -p wyck-platform`.

## Where to add X

- Another fact about the build: `build_info`.
- A change to what is logged or kept: `logging`, keeping `KEPT_LOG_DAYS` and `KEPT_REPORTS` the
  only knobs.

## Do not

- Do not depend on another crate of the workspace: this is the bottom of the infrastructure.
- Do not read or write the user's files here (settings, documents, secrets): that is `wyck-storage`.
- Do not use `gpui`, `reqwest` or `keyring`: `cargo xtask deps` fails on them.
