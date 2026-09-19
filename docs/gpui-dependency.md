# Depending on GPUI

How `wyck-app` consumes GPUI, why, and how to upgrade it. The choice of GPUI itself is in
[decisions/0001-gui-framework.md](decisions/0001-gui-framework.md). Everything below was
checked on 2026-09-19 with `rustc 1.98.1` on Windows 11, unless it says otherwise.

## Policy

- Depend on **`gpui-kit` at an exact version** (`=0.6.4` today), and only through the `gui`
  feature of `wyck-app`. `gpui-kit` is the entry point its authors intend: it pins the matching
  GPUI snapshot and re-exports GPUI, `gpui-base`, `gpui-component` and the default assets, and
  provides `gpui_kit::application()`, `gpui_kit::init` and the platform code. Its default
  features (`component`, `assets`) are what the app uses; the Tree-sitter language features are
  opt-in and stay off. One exact version fixes the whole UI stack.
- Import GPUI through `gpui_kit::` (`gpui_kit::Context`, `gpui_kit::component::...`), never as a
  separate `gpui` dependency. There is then one copy of GPUI in the graph, and after any change
  `cargo tree -d | grep -E '^gpui'` must print nothing.
- Commit `Cargo.lock` (already done) and build with `--locked` in CI.
- Declare the version once, in `[workspace.dependencies]` of the root `Cargo.toml`, with a
  comment pointing here.
- Never use a floating requirement (`0.6`, `*`, a git branch). GPUI has breaking changes between
  versions and its own README says so.
- Do not use `gpui_tokio`. The engine's futures only wait on `tokio::sync` channels, so `cx.spawn`
  awaits `watch_state().changed()` directly. Checked: the shell follows the engine that way.

## The four ways to get GPUI

| Way | State on 2026-09-19 | Verdict |
|---|---|---|
| `gpui` on crates.io | 0.2.2, published by Zed on 2025-10-22, 11 months old. Predates the split of the platform code into `gpui_platform`, `gpui_windows` and others, which Zed's `main` has since done. | Not used. Too old for the component library, and it lags Zed's `main`. |
| Git dependency on `zed-industries/zed` at a pinned commit | Official source and exactly reproducible with `--locked`. Zed's `[patch.crates-io]` section (forks of `async-task`, `calloop`, `async-process`, `notify` and others) belongs to Zed's workspace, and Cargo ignores patches declared by a dependency, so a consumer has to copy the ones it needs. Pulls the whole Zed repository. | Fallback. This is what the `zeron` project does, through its own fork with a pinned revision. |
| `gpui-pre` and its sibling crates on crates.io | 0.3.5 on 2026-09-14, described as "a snapshot of zed@d89e9c2", Apache-2.0. Published by the `gpui-component` maintainer (Jason Lee, `huacnlee`), **not by Zed Industries**, even though the repository field points at Zed's. Six versions since 2026-09-03. | Used, through `gpui-kit`. |
| `gpui-kit` 0.6.4 and `gpui-component` 0.6.4 | 2026-09-18, Apache-2.0, same maintainer. `gpui-kit` bundles GPUI, the component library and default assets behind one dependency (default features: `component`, `assets`; Tree-sitter languages are opt-in). `gpui-component` depends on `gpui-pre` 0.3.5. | `gpui-kit` used. |

The trade being made: a crates.io snapshot buys a normal, lockable, offline-friendly dependency
and a component library tested against it, at the price of relying on one community maintainer
to keep publishing. If that stops, move to the git dependency on Zed at the commit of the last
snapshot (`d89e9c2` today), and copy the `[patch]` entries the build needs.

One caution on sources: an automated summary of the crates.io page called `gpui-pre` "official".
The publisher and the name say otherwise, so treat it as a community snapshot.

## What was proven

Two checks, both on `rustc 1.98.1` (the workspace `rust-version` is 1.98) and Windows 11.

**A throwaway project** outside the repository, with `gpui-component = "=0.6.4"`, `wyck-engine`
and `wyck-config`:

- Resolution: 682 packages for GPUI alone, 745 with the engine.
- `cargo check`: 1 min 19 s from cold for GPUI alone, 28 s more with the engine.
- **No dependency conflict with the engine.** A single `reqwest` (0.13.5) with `native-tls`
  (SChannel on Windows) serves the engine, and GPUI's dependencies add no second TLS stack. That
  matters: the root `Cargo.toml` says the TLS backend was verified against `mcp.ctrader.com` and
  must not be swapped without re-verifying a real connection.
- Only ordinary version duplicates appear (`windows` 0.58 and 0.62, `base64`, `rand`, ...).
- Neither GPUI nor the `gpui-component` workspace declares a `rust-version`. GPUI's README
  asks for the latest stable Rust, and both use edition 2024.

**The `wyck-app` shell** (`cargo run -p wyck-app`, `gpui-kit =0.6.4`), against a demo account:

- It links and runs. A cold debug build of the whole workspace takes about 2 minutes. No CMake.
- **The window opens and renders.** The log shows the Windows platform layer using **Direct3D
  11.1** on the machine's GPU and DirectWrite text (an earlier note said the renderer was not
  established).
- **The async bridge works without `gpui_tokio`.** A `cx.spawn` task awaiting the engine's
  `watch_state().changed()` followed the session from Disconnected through Connecting and Loading
  account to Ready, and the account figures reached the shared model.
- **Global hotkeys work on GPUI's message loop, with no thread and no `unsafe`.** Registered on
  the main thread with `global-hotkey`, `ctrl+alt+b` and `ctrl+alt+s` were delivered while another
  program had the focus, and each produced a dry-run order (nothing sent). A hotkey handler that
  only pushes to an unbounded channel is enough.

**Not proven yet**: a `PopUp` window's focus and always-on-top behavior in practice, a held key
(GPUI issue #61469), the phantom Alt on activation (#62404), a table of hundreds of rows, a chart,
packaging, and the Linux build. The checklist is in `TODO.md` 4.1.

## Build requirements

Windows (from Zed's documentation, where the build is heavier than ours):

- Rust through `rustup`, and the MSVC C++ build tools with the "Desktop development with C++"
  workload.
- The Windows 10 or 11 SDK, version 2104 (10.0.20348.0) or later.
- Long paths enabled (`git config --system core.longpaths true`, plus the registry setting) for
  the deeply nested dependency paths.
- Do not set the `RUSTFLAGS` environment variable: it overrides `.cargo/config.toml` and breaks
  the build. Put flags in a config file instead.
- Zed needs CMake for one of its own dependencies (`wasmtime`); the GPUI graph here did not.
- If a Vulkan error appears at run time, update the GPU driver (Zed's advice). On Windows GPUI
  uses Direct3D 11, so this concerns Linux.

Linux, if the app is ever built there. Zed's `script/linux` installs, on Debian and Ubuntu
among others: `libwayland-dev`, `libxkbcommon-x11-dev`, `libx11-xcb-dev`, `libfontconfig-dev`,
`libvulkan1`, `libasound2-dev`, `libglib2.0-dev`, `libssl-dev`, `clang`, `cmake`, `lld`. At least
one of the `wayland` or `x11` features is mandatory on Linux.

## Continuous integration

`wyck-app` has a `gui` feature (on by default) that brings GPUI in. Without it the crate is the
application layer alone and builds anywhere Rust does. CI therefore treats the two platforms
differently:

- **Windows** builds everything, GPUI included, with `--locked`. It is the development and target
  platform.
- **Ubuntu** builds the workspace without `wyck-app` (`--exclude wyck-app`), and builds and tests
  `wyck-app` with `--no-default-features`. GPUI needs system packages on Linux (the list above)
  that the job does not install; add them, and drop the exclusion, when there is a reason to
  build the window on Linux.

The GPUI build is the slow part of the workflow, so the job's time limit is raised accordingly.

## Upgrading

Upgrades are deliberate, one commit each:

1. Read the `gpui-component` release notes and Zed's GPUI changes for the range.
2. Bump the exact versions together in `[workspace.dependencies]`, then `cargo update -p
   gpui-component --precise <version>`.
3. `cargo tree -d | grep -E '^gpui'` must print nothing.
4. Run the whole check list of `TODO.md` section 2, then the window spike and its checklist.
5. Record what changed for us in the crate's changelog.

Automated dependency updates (`TODO.md` section 8) must ignore `gpui-component`, `gpui-pre` and
the `gpui-pre-*` crates.
