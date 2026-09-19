# 0001: GUI framework

Status: accepted, 2026-09-19. Decided by the project owner. How the dependency is consumed
is in [../gpui-dependency.md](../gpui-dependency.md).

## Context

`wyck-app` is a desktop application driven mostly by the keyboard, developed and used on
Windows, that sits next to a charting platform (cTrader Desktop). What it needs from a UI
toolkit:

- a **floating panel** that stays on top of another application without taking its focus;
- **global hotkeys** that work while another window has focus;
- fast **tables** (positions, news, log) and some **custom drawing** (small charts);
- an **async model** compatible with the engine, which owns a Tokio runtime and exposes futures
  that only wait on channels, so any executor can await them;
- Windows support that is first class, and a permissive license;
- **logic testable without a window**, and a way to package and update the app.

The engine deliberately has no UI code and no UI dependency, so this choice only affects
`wyck-app`. That is what keeps it cheap to revisit.

## Decision

Use **GPUI**, the UI framework of the Zed editor, through the **gpui-kit** crate (GPUI plus the gpui-component library). The visual design is the owner's and is not part of this decision: `wyck-app` ships a shell with an empty window.

## Why

Facts checked on 2026-09-19 against the crates, the source and the documentation.

- **The floating panel is supported by the platform layer.** In `gpui_windows`, a window of
  kind `WindowKind::PopUp` gets the extended styles `WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, and
  with `focus: false` it is shown with `SW_SHOWNOACTIVATE`. That is an always-on-top panel that
  does not appear in the taskbar and does not steal focus, without native code of our own.
- **Windows is a supported target.** The README says Windows needs no feature flags: windowing
  is Win32, text is DirectWrite. Zed ships Windows builds for x86_64 and ARM64.
- **The component library covers the tables.** `gpui-component` (Apache-2.0, 0.6.4 on
  2026-09-18) lists 60+ components, including a data table with virtual scrolling and
  resizable columns, a virtual list, a dock layout with panels and tabs, and charts. Its README
  says it powers a shipped commercial desktop application.
- **The async model fits.** GPUI has its own executor. The engine's futures only wait on
  `tokio::sync` channels, so `cx.spawn` can await `watch_state().changed()` directly. Zed also
  has a `gpui_tokio` crate that bridges a Tokio runtime (`init_from_handle` accepts an existing
  one), kept in reserve if a future needs a real Tokio context.
- **License.** Apache-2.0, same as this repository.
- **Activity.** Zed released v1.20.2 on 2026-09-17, and GPUI received 15 commits in the five days to 2026-09-18.

## Options not chosen

None of these was prototyped. The choice was made on the facts above and on the owner's
preference, so treat the rejections as "not chosen", not "disqualified".

| Option | Version and date (crates.io) | Why not |
|---|---|---|
| egui | 0.36.2, 2026-09-08, MIT or Apache-2.0 | The strongest alternative: very active, immediate mode, mature plotting. Kept as the fallback (below). Judgment, not tested: an immediate-mode model is a different fit for docked, keyboard-driven panels. |
| Slint | 1.18.0, 2026-09-16 | Licensed GPL-3.0, or its own royalty-free or commercial terms: not a plain permissive license for an Apache-2.0 project. Its own markup language for the UI. |
| Iced | 0.14.0, 2025-12-07, MIT | Release cadence is slower (nine months since the last release). Not evaluated for tables and hotkeys. |
| Tauri | 2.11.5 stable, 2026-07-01 (3.0 in alpha) | Best chart ecosystem, because the UI is web. Costs a second language and a webview runtime, and puts an IPC boundary between the UI and the engine that the in-process design avoids. |
| Dioxus | 0.7.10, 2026-07-30 (0.8 in alpha) | Same trade as Tauri for desktop (a web renderer), on a younger stack. |

## Risks

GPUI is pre-1.0 and its README warns that breaking changes are frequent. Specific risks, with
what is known:

1. **API churn.** Mitigated by pinning an exact version and upgrading on purpose
   ([../gpui-dependency.md](../gpui-dependency.md)).
2. **Windows issues reported upstream** (open GitHub issues found on 2026-09-19, not
   reproduced by us). Each matters to a hotkey-driven app:
   - #62404: a synthetic Alt key press is injected into the global input queue on every window
     activation;
   - #61469: a window can stop presenting frames for 5 to 15 seconds under sustained keyboard
     input, which is what a held key looks like;
   - #61508: a transparent `PopUp` window still shows a border;
   - #63471: a display change re-shows hidden windows.
3. **No global hotkeys in GPUI.** The usual crate is `global-hotkey` 0.8.0 (Tauri, 2026-05-01).
   On Windows it needs a Win32 event loop on the thread that creates the manager, and on Linux
   it supports X11 only. Checked on Windows: registered on GPUI's main thread, the shortcuts are delivered by GPUI's own message loop, so no dedicated thread is needed.
4. **Charts.** The component library advertises charts, but a candlestick chart with pan and
   zoom is unproven.
5. **Packaging, code signing and auto-update** are not addressed yet (`TODO.md` 6.5).
6. **Supply chain.** The practical way to depend on GPUI today is a snapshot published by the
   `gpui-component` maintainer, not by Zed (see the dependency doc).

## Validation and fallback

The risks are retired by a validation spike in `crates/wyck-app` (`TODO.md` 4.1): a GPUI window
that connects the engine to a demo account, follows `watch_state`, and reacts to a global
hotkey. The window has no visuals of its own, since the owner brings the design. It passes when
all of these hold on the Windows development machine.

Done on 2026-09-19:

- [x] The engine's state updates reach the UI without blocking either executor, and without
  `gpui_tokio`.
- [x] A global hotkey fires while another program has the focus, on GPUI's own message loop, with
  no extra thread and no `unsafe`.
- [x] A binary that uses GPUI links and runs with `--locked`, and the window renders (Direct3D
  11.1).

Still to check:

- [ ] A `PopUp` panel stays on top of cTrader Desktop and never takes its focus.
- [ ] Holding a key does not freeze presentation (#61469), and window activation does not inject
  Alt (#62404).
- [ ] A table of a few hundred rows scrolls smoothly, and a simple chart can be drawn.

**Fall back to egui** if the focus-free panel cannot be made reliable without native code out of
proportion to the app, or if the frame stall reproduces and has no workaround. Because the engine
is UI-agnostic, that switch costs `wyck-app` only.

## Consequences

- `wyck-app` depends on `gpui-kit` (behind its `gui` feature), `wyck-engine` and `wyck-config`, nothing else
  from the workspace.
- CI must be able to build GPUI: system packages on Linux, or a Windows-only job for
  `wyck-app` at first.
- Charts and any custom drawing are ours to write on top of GPUI's primitives.
