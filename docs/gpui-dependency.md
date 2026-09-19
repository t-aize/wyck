# Depending on GPUI

How `wyck-app` consumes GPUI, why, and how to upgrade it. The choice of GPUI itself is in
[decisions/0001-gui-framework.md](decisions/0001-gui-framework.md). Everything below was
checked on 2026-09-19 with `rustc 1.98.1` on Windows 11, unless it says otherwise.

## Policy

- Depend on **`gpui-component` at an exact version** (`=0.6.4` today). It pins the GPUI
  snapshot it was built and tested against, so one exact version fixes the whole UI stack.
- If the app needs GPUI types directly, add `gpui-pre` under the name `gpui`, at the exact
  version `gpui-component` uses, and keep the two in lockstep:
  `gpui = { package = "gpui-pre", version = "=0.3.5" }`. Two copies of GPUI in the graph give
  incompatible types, so after any change run `cargo tree -d | grep -E '^gpui'` and expect nothing.
- Commit `Cargo.lock` (already done) and build with `--locked` in CI.
- Declare the versions once, in `[workspace.dependencies]` of the root `Cargo.toml`, with a
  comment pointing here.
- Never use a floating requirement (`0.6`, `*`, a git branch). GPUI has breaking changes between
  versions and its own README says so.
- Do not use `gpui_tokio` until a real need appears. The engine's futures only wait on
  channels, so `cx.spawn` can await them (to be confirmed by the spike).

## The four ways to get GPUI

| Way | State on 2026-09-19 | Verdict |
|---|---|---|
| `gpui` on crates.io | 0.2.2, published by Zed on 2025-10-22, 11 months old. Predates the split of the platform code into `gpui_platform`, `gpui_windows` and others, which Zed's `main` has since done. | Not used. Too old for the component library, and it lags Zed's `main`. |
| Git dependency on `zed-industries/zed` at a pinned commit | Official source and exactly reproducible with `--locked`. Zed's `[patch.crates-io]` section (forks of `async-task`, `calloop`, `async-process`, `notify` and others) belongs to Zed's workspace, and Cargo ignores patches declared by a dependency, so a consumer has to copy the ones it needs. Pulls the whole Zed repository. | Fallback. This is what the `zeron` project does, through its own fork with a pinned revision. |
| `gpui-pre` and its sibling crates on crates.io | 0.3.5 on 2026-09-14, described as "a snapshot of zed@d89e9c2", Apache-2.0. Published by the `gpui-component` maintainer (Jason Lee, `huacnlee`), **not by Zed Industries**, even though the repository field points at Zed's. Six versions since 2026-09-03. | Used, through `gpui-component`. |
| `gpui-component` 0.6.4 and `gpui-kit` 0.6.4 | 2026-09-18, Apache-2.0, same maintainer. `gpui-component` depends on `gpui-pre` 0.3.5. `gpui-kit` bundles GPUI, the component library and default assets behind one dependency and also carries Tree-sitter language features for 40 or more languages. | `gpui-component` used. `gpui-kit` not used: the app has no code editor. |

The trade being made: a crates.io snapshot buys a normal, lockable, offline-friendly dependency
and a component library tested against it, at the price of relying on one community maintainer
to keep publishing. If that stops, move to the git dependency on Zed at the commit of the last
snapshot (`d89e9c2` today), and copy the `[patch]` entries the build needs.

One caution on sources: an automated summary of the crates.io page called `gpui-pre` "official".
The publisher and the name say otherwise, so treat it as a community snapshot.

## What the spike proved

A throwaway project outside the repository, `gpui-component = "=0.6.4"` with `wyck-engine` and
`wyck-config` as path dependencies, on `rustc 1.98.1` (the workspace `rust-version` is 1.98):

- Resolution: 682 packages for GPUI alone, 745 with the engine.
- `cargo check`: 1 min 19 s from cold for GPUI alone, 28 s more with the engine. `cargo build
  --locked` (debug) links in 1 min 47 s. No CMake needed for these.
- **No dependency conflict with the engine.** A single `reqwest` (0.13.5) with `native-tls`
  (SChannel on Windows) serves the engine, and GPUI's dependencies add no second TLS stack. That
  matters: the root `Cargo.toml` says the TLS backend was verified against `mcp.ctrader.com` and
  must not be swapped without re-verifying a real connection.
- Only ordinary version duplicates appear (`windows` 0.58 and 0.62, `base64`, `rand`, ...).
- Neither GPUI nor the `gpui-component` workspace declares a `rust-version`. GPUI's README
  asks for the latest stable Rust, and both use edition 2024.

**Not proven**, because the spike's binary did not call GPUI (so the linker dropped it): creating
a window, rendering, and everything in the spike checklist of the decision record.

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
- If a Vulkan error appears at run time, update the GPU driver (Zed's advice). Which renderer
  GPUI uses on Windows today was not established.

Linux, if the app is ever built there. Zed's `script/linux` installs, on Debian and Ubuntu
among others: `libwayland-dev`, `libxkbcommon-x11-dev`, `libx11-xcb-dev`, `libfontconfig-dev`,
`libvulkan1`, `libasound2-dev`, `libglib2.0-dev`, `libssl-dev`, `clang`, `cmake`, `lld`. At least
one of the `wayland` or `x11` features is mandatory on Linux.

## Continuous integration

CI runs on ubuntu and windows and builds the whole workspace. Adding `wyck-app` will break the
ubuntu job until the packages above are installed. Decision: **build `wyck-app` on Windows only
at first** (`cargo build --workspace --exclude wyck-app` on ubuntu), because Windows is the
development and target platform. Add Linux, with the packages installed, when there is a reason.
The Windows job must keep `--locked`.

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
