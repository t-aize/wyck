# wyck

The program. `src/main.rs` starts the logs, brings the saved data in force, opens the window and runs
the event loop; everything the app does is in the library crates it calls. This is also where the
packaging lives (the `[package.metadata.packager]` of `Cargo.toml`, the icons in `assets/`, and
`build.rs`, which gives the Windows executable its icon).

Depends on: `wyck-app`, `wyck-clock`, `wyck-core`, `wyck-platform`, `wyck-storage`, `wyck-ui`,
`gpui`, `chrono`, `tracing`; `winresource` at build time on Windows.
Used by: nobody; it is the end of the dependency graph.

## What `main` does, in order

1. `start_logging`: the first thing, so that whatever goes wrong afterwards is on disk. The libraries
   only emit events; the subscriber and the panic hook are installed here and nowhere else.
2. `wyck_ui::kit::application()` and `init`: the component library; the flush of the log on quit.
3. `updates::init`, then `prepare_saved_data`: what waited for this start (a reset or an import the
   user asked for) is applied **before anything reads the documents it replaces**, the documents of
   an older version are migrated, the saved look and the scripts are put in force, and a daily
   automatic copy is made off the interface thread.
4. The editor and the keymap are initialised, the bundled font is loaded, and the window opens at
   most of the screen, never smaller than `MIN_WINDOW`. If it cannot open, the app logs why and quits.
5. The update check, the notes about the folders of the data, and the window is activated.

## Invariants and traps

- **The order in `main` is the contract.** Pending resets and imports, then migrations, then
  anything that reads documents; the look before the first frame.
- **`build.rs` and the icons are packaging, not code.** `cargo packager` reads `Cargo.toml`
  (`product-name`, `identifier`, `icons`, `formats`) from this folder; the release is built by
  `.github/workflows/release.yml`.
- **Windows release builds are a window app** (`windows_subsystem = "windows"`): there is no console,
  so a failure must reach the log file, not `println!`.
- **No `[]` on a slice or a vector, no `unwrap`**: the workspace lints apply here like anywhere.

## Tests

None: the program only wires the crates. The guards that read its sources (`KeyBinding::new` only
in the keymap) are in `crates/wyck-ui/tests/guards/`, which also scan `wyck/src`. To see the app
run: `cargo run -p wyck` (a debug build prints its logs in the console and writes them in the logs
folder of the app).

## Where to add X

- Something to do at start: a function in `main.rs`, called in the right place of `main`; if it
  needs a document, it comes after `prepare_saved_data`.
- A new screen: a module of `wyck-ui` (see its `AGENTS.md`), hosted in `wyck_ui::shell`.

## Do not

- Do not put logic here: the program only wires the crates. A rule, a screen or a use case has a
  crate.
- Do not name `gpui-kit` here: `wyck_ui::kit` re-exports what the program needs.
- Do not read a document before `prepare_saved_data` has run.
