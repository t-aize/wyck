# wyck-app

The application layer between `wyck-engine` and a front end, plus a GPUI shell that `cargo run`
launches. It has **no visuals**: the window is empty and dark, and your own views plug in at one
place.

```text
your views  <---  shell (GPUI: window, plumbing)  <---  application layer  <---  wyck-engine
```

## Run

```sh
WYCK_SERVICE=remote WYCK_TOKEN=<demo token> WYCK_SYMBOL=EURUSD cargo run -p wyck-app
```

`cargo run` opens the window, connects, and registers three global shortcuts that work while
another program has the focus:

| Keys (default) | Effect |
|---|---|
| `ctrl+alt+b` | Plan and submit a **dry-run** buy with the default risk and stop |
| `ctrl+alt+s` | The same for a sell |
| `ctrl+alt+p` | Calls your `on_toggle_panel` hook (nothing by default) |

Without `WYCK_SERVICE`, `WYCK_ENDPOINT` and `WYCK_TOKEN` the active profile of `wyck-config` is
used (its token comes from the OS keyring). All variables are in `src/settings.rs`. Logs go to a
daily file under the data directory (`%APPDATA%\wyck\...\logs` on Windows), and to stderr in a
debug build.

**Orders sent from a shortcut are dry runs.** Nothing reaches the broker, and the controller
refuses to run at all while the engine is armed. Real orders need an arming flow, which does not
exist yet (`TODO.md` 6.1).

## Plug in your views

The shell decides nothing about looks. `src/main.rs` calls `shell::run` with a function that
builds the root view:

```rust
shell::run(args, Hooks::default(), |shell, _window, cx| cx.new(|cx| MyView::new(shell, cx)));
```

A view holds the `Shell`: `shell.model` is an entity to observe (it holds the latest engine state,
the activity log, the notices and the banners), and `shell.order(side, cx)` and `shell.dismiss(id,
cx)` are the actions. Draw what `presentation` computes: it returns strings and a `Tone`
(neutral, good, warn, bad), and never a color, so the palette is yours.

## Layers

Every module below the shell is plain Rust, tested without a window.

| Module | Role |
|---|---|
| `settings` | What the application is configured with, from the environment |
| `startup` | From settings to a connection request (profile or environment) |
| `controller` | The use cases: start the engine, connect, send a hotkey order |
| `presentation` | Engine state to formatted rows, badges and tones. An account of unknown kind is never drawn like a demo |
| `model` | The data a front end's windows share |
| `messages` | Errors and outcomes to user-facing notices, in one place |
| `hotkeys` | Global shortcuts: parsing, registration, key-repeat debouncing |
| `logging`, `session_marker` | Log files, and detection of a session that did not end cleanly |
| `shell` (feature `gui`) | The GPUI window and plumbing |

## Without GPUI

The `gui` feature is on by default. `cargo build -p wyck-app --no-default-features` builds the
application layer alone, with no windowing toolkit in the build. GPUI is pinned to an exact
version, see [../../docs/gpui-dependency.md](../../docs/gpui-dependency.md).

## Not done yet

- Persisted engine configuration and settings screen (`TODO.md` 4.4).
- Arming, and real orders from the keyboard (`TODO.md` 6.1).
- A profile creation flow: profiles are made with `wyck-config` for now.
- Views, of course.
