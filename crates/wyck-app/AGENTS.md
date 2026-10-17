# wyck-app

What the app remembers and does between the services below it (the broker, the files, the log) and
the screens above it: sign-in, the live account and its orders, alerts, the live price hub and
history loading, the arrangement the user saved, the look, the indicator folder, updates. These are
gpui entities and globals with no view of their own; the screens in `wyck` read them and call them.

Depends on: `wyck-chart`, `wyck-core`, `wyck-ctrader`, `wyck-drawings`, `wyck-indicators`,
`wyck-market`, `wyck-platform`, `wyck-storage`, `wyck-trading`, `gpui`, `tokio`, `serde`,
`serde_json`, `toml`, `secrecy`, `thiserror`, `tracing`, `chrono`, `async-trait`,
`cargo-packager-updater`, `notify-rust`, `rodio`, `rust-embed` (`tempfile` in tests only).
Used by: `wyck` (every screen). Not `gpui-kit`: this crate draws nothing.

## Module map

The modules are public (the screens write `alerts::Condition`, `workspace::Preferences`...); what a
screen does not name is `pub(crate)`. The crate root only declares them and re-exports `app_paths`.

| Module | What it holds |
|---|---|
| `account` | `Account`, the live account entity: reads balance, positions and orders on every connection, follows execution events, sends orders (`orders`) and keeps the promises of their plans (`plans`: OCO, break-even, time stops), follows an ATR stop (`follow`), reverses a position (`reverse`), refreshes the profit (`sync`). `notice` puts a refusal or an outcome in a `Notice` the screens show as a toast, and is where the sentences of the trading guard (`Refusal`, `Warning`, `Lock`) and of an `Outcome` are written. |
| `sign_in` | `SignIn`, an entity with no view: finds a saved session or walks through cTrader's consent page and the choice of an account (`consent`), keeps the pure `SignInState` (`state`), the decisions that need no window (`rules`), opens the config (`config`). |
| `alerts` | `Alerts`, the entity that judges, saves and fires alerts. `model` is the data (`Alert`, `Source`, `Condition`, `Trigger`), `eval` the plain functions that decide, `sound` the sound and the desktop notification (on a thread of its own). |
| `market_data` | `live::LiveHub` (what to subscribe to: the union of every owner's wish) and `load` (history in chunks within the server's limits); `now_ms`. |
| `workspace` | `Workspace`, the live copy of the saved arrangement: `Preferences` (global, one module per domain: `charts`, `timeframes`, `tools`, `limits`) and `Watchlists` (per account), and `layouts`. |
| `saver` | `Saver`, the write of a document a moment after the last change, and on exit; every saved document of the crate (and the script drafts) uses it. |
| `prefs` | What a feature keeps in the workspace document: `ticket`, `panel`, `indicators`. |
| `appearance`, `chart_export`, `drawings` | The look (own document), the export panel's options and presets (own document), the live drawings every chart shares. |
| `scripts` | The indicator folder: `Service` keeps the list current and publishes it to the indicator registry; `folder::Library` reads it and does what a user does to files. |
| `locations`, `migrations`, `diagnostics` | Where the data lives and the start-up move of folders (`app_paths`); bringing saved documents to this version; where the logs are. |
| `shortcuts`, `updates` | `Chord`, a key combination as text; checking, downloading and installing signed updates. |
| `broker`, `storage`, `system` | Doors: `pub use` of `wyck-ctrader`, `wyck-storage`, `wyck-platform`. The screens reach those crates only through here. |
| `token_store` | Keeps a session's OAuth tokens in the credential store of `wyck-storage` (private to the crate). |

## Invariants and traps

- **An order goes through `Account::place_with` (or `place_batch`)**: it is validated, cleared by
  the guard of `wyck-trading` (`Account::clear`: one-click orders are judged more strictly), and not
  sent again while another is in flight or when it repeats a recent one. The connection refuses a
  new order that has no `Cleared` for it and for the account's `Mode`, so there is no way around
  it; do not add one (no constructor of `Cleared` outside `guard`).
- **Every new order carries an id of its own** (`client_order_id`, from the clock, the process and a
  count), so it can be recognized on the account whatever happens to its answer.
- **A timeout is not a refusal.** After a failed or unanswered trading call (timeout, closed
  connection) the account is read again and marked uncertain, the order is kept as `in_doubt`, and
  it is not resent: once the account is read, `settle_in_doubt` tells the user whether it is
  working, was executed, or is not on the account. Sending it again is the user's decision.
- **Demo and live are a `Mode`, fixed when the `Account` is made** from the account the user chose;
  a proof made for one is refused on a connection of the other.
- **Tokens live in the credential store**: `token_store` reads and writes them through
  `wyck-storage`. A failed read logs the error, never the token.
- **`app_paths()` is the first call at start** (`run` in `wyck` does it before anything else): its
  first call applies the folder move the user asked for. Nothing reads a document before that,
  nor before `migrations::run`.
- **Every saved document has a `schema_version`, a plan in `migrations` and a fixture.** Raising a
  version without its step loses the user's settings.
- **Everything read from a file is repaired, not trusted** (`normalized`): a value this version does
  not know becomes a default, never an error that keeps a panel from opening.
- **Saves are debounced** (`Saver`): a change is written a moment later and once more on exit or
  when the saver is dropped.
- **Updates are signed.** `updates` checks the `latest.json` of the latest GitHub release against
  the public key compiled in (`assets/update.pubkey`); development builds do not update.
- **The modules form no cycle** (`alerts -> drawings -> saver`, `workspace -> alerts, prefs`): the
  alert limits live in `alerts`, the delayed write in `saver`. Keep the arrows that way.

## Global state

- `locations`: `PATHS` and `STARTUP` (`OnceLock`): the folders of the app, resolved once, and what
  that start did, for the notices.
- gpui globals: `appearance::State`, `scripts::Service`, `updates::Service`.
- `alerts::sound`: the thread that plays sounds and shows notifications (`OnceLock`).
- `workspace::layouts`: the catalog of layouts, built once (`OnceLock`).
- `scripts` publishes to the registry of `wyck-indicators`, which charts read.

## Tests

183 unit tests next to the code: the sign-in states and rules, the alert rules (`eval`), the
guard path of an order and its notices, the live hub and the history loading (against a fake
server), the migrations (`src/fixtures/` holds the old layouts), the shortcuts, the workspace
documents, the update states. The rules are plain functions so they run without a window. Some
constructors exist only for tests (`#[cfg(test)]`). Run: `cargo test -p wyck-app`.

## Where to add X

- A saved document: its type with a `schema_version`, a name constant, a `Plan` in `migrations`
  with a fixture of the old layout, and its name in `locations::KNOWN_DOCUMENTS`.
- A setting: a field with a default in `workspace::Preferences` or in `prefs`, repaired in
  `normalized` (its rules in the module of its domain under `workspace/preferences/`); a migration step only if an existing file changes shape.
- An alert source or condition: `alerts::model::{source, condition}`, its rule in `alerts::eval`.
- A sentence for an order outcome or a refusal: `account::notice`.
- An alert sound: a file in `assets/sounds`, `alerts::sound`, and `scripts/make_sounds.py`.
- A call to the broker the screens need: add it in `wyck-ctrader`, then reach it through `broker`.

## Do not

- Do not name `wyck-ctrader`, `wyck-storage` or `wyck-platform` in a screen: go through `broker`,
  `storage`, `system`.
- Do not depend on `gpui-kit`, on a screen crate, or on `reqwest`/`keyring`: `cargo xtask deps`
  fails on them. Network is `wyck-ctrader`'s, files and secrets are `wyck-storage`'s.
- Do not resend an order after a timeout, and do not skip the guard.
- Do not put a token, a secret or an account number you cannot show in a log line or a notice.
