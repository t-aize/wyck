# wyck

[![CI](https://github.com/t-aize/wyck/actions/workflows/ci.yml/badge.svg)](https://github.com/t-aize/wyck/actions/workflows/ci.yml)
[![Audit](https://github.com/t-aize/wyck/actions/workflows/audit.yml/badge.svg)](https://github.com/t-aize/wyck/actions/workflows/audit.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A terminal trading panel for cTrader, built for speed, not for staring at it.

> **Status:** early development. Everything can change, including the layout of this file.

## Disclaimer

**This is not financial advice.** Trading leveraged products such as forex, CFDs and crypto
carries a high risk of losing money, possibly more than you deposited. This software is
experimental and may contain bugs that place, modify or close orders you did not intend.
Test with a demo account first. You are solely responsible for your trades and for any loss.

The software is provided "as is", without warranty of any kind, as stated in the
[Apache License 2.0](LICENSE). The authors are not liable for any loss or damage arising from
its use.

wyck is an independent project. It is **not affiliated with, endorsed by, or sponsored by
cTrader or Spotware Systems**. cTrader is a trademark of its owner. Company, coin and country
marks bundled in `assets/marks` are trademarks of their owners, shown only to identify the
instrument being traded, and are used under the license of each set (see the `LICENSE.txt`
next to it).

## Install

Download the files for your system from the [latest Wyck release](https://github.com/t-aize/wyck/releases/latest):

| System | File |
|---|---|
| Windows x86_64 | `Wyck_<version>_windows-x86_64-setup.exe` |
| macOS Intel | `Wyck_<version>_macos-x86_64.dmg` |
| macOS Apple Silicon | `Wyck_<version>_macos-aarch64.dmg` |
| Linux x86_64 | `Wyck_<version>_linux-x86_64.AppImage` or `.deb` |

The first releases are not signed with Authenticode or Apple Developer ID. Windows SmartScreen
and macOS Gatekeeper can therefore show a warning. Check `SHA256SUMS` and the GitHub artifact
attestation before running a download. The updater uses a separate committed public key and
refuses an update whose cargo-packager signature is invalid.

Production builds check the latest stable release once at startup. Open Settings with `Ctrl+,`,
then select About to check again or install an available update. The AppImage, macOS bundle and
Windows installer can update in place. A `.deb` installation opens the release page so the system
package remains managed by the package manager.

## Layout

One Cargo package, `wyck`: `src/lib.rs` holds the app, `src/main.rs` only starts it. Modules, by
role (they are being regrouped into four layers, see `docs/ROADMAP.md`):

| Module | Owns |
|---|---|
| `src/openapi` | cTrader Open API client: messages, WebSocket client, OAuth, reconnecting session, contract math and account book |
| `src/infra/storage` | Native settings, documents, backups, indicator files and credential storage |
| `src/chart_core` | Chart data, calculations, studies, drawings and scene commands |
| `src/ui/kit` | Widget kit, theme and sizes shared by every screen |
| the other `src/*` modules | GPUI screens and the app's own services (`src/services`) |

Every control, menu, dialog and color comes from the widget kit. The Open API guide is in the
documentation of the `openapi` module (`cargo doc --open`).

## Your own indicators

Indicators can be written as scripts (in [Rhai](https://rhai.rs)) and kept in a folder that the
app reads on its own. The header has a button for the folder and for a full editor.

## Accessibility

- **Keyboard.** Tab reaches the controls. In the account panel, the current row is the one stop
  of Tab: the arrows, Home, End, Page Up and Page Down move among the rows, Enter or Space does
  what a click does, and the menu key (or Shift+F10) opens the menu of the row. Modal windows
  keep Tab inside themselves and give the focus back when they close.
- **Screen readers.** The app exposes roles and names through AccessKit (Windows UI Automation,
  macOS, and AT-SPI on Linux): tables with rows, column headers and cells, tabs, dialogs, menus,
  toggles and notices. This has been built from the AccessKit documentation and covered by unit
  tests of what can be tested without a screen, but not yet tried with NVDA, VoiceOver or Orca.
  Reports from people who use them are the most useful thing you can send.
- **Size.** The interface scale in Settings (80 to 160 percent) also scales the text drawn on the
  charts.
- **Colors.** Every theme that comes with the app meets WCAG 2.2 contrast (4.5:1 for text, 3:1
  for the accent and the chart). The editor of your own themes shows what falls short and can fix
  it; it never refuses a color.

## Requirements

- A recent stable Rust toolchain (the minimum is `rust-version` in `Cargo.toml`, currently 1.98).
  `rust-toolchain.toml` makes rustup pick stable and install it if needed; run `rustup update` if
  your stable is older than the minimum.
- Linux only: the development packages of fontconfig, Wayland, OpenSSL, X11 and xkbcommon, for example on Debian and Ubuntu
  `pkg-config libfontconfig-dev libwayland-dev libssl-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libasound2-dev`.
  The last one is for the alert sounds.

## Build and test

```sh
cargo build
cargo test --workspace
```

`cargo test` never touches the network: it is entirely mock servers and unit tests.

The first build is slow because the dependency graph is large and dependencies are compiled
with optimizations even in debug builds (see `[profile.dev.package."*"]` in `Cargo.toml`).
Later builds only rebuild the changed crates.

## Test against a real demo account

Testing the cTrader client against a real cTrader demo account, end to end, is a separate opt-in step.
Fill in `.env` from [.env.example](.env.example), then:

```sh
cargo test --test openapi_live -- --ignored --nocapture
```

## Checks

The same checks run in CI on Linux, Windows and macOS:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

Dependencies are checked with [cargo-deny](https://github.com/EmbarkStudios/cargo-deny)
(`cargo install cargo-deny --locked`, then `cargo deny check`) and updated by Dependabot.

Release builds and signing are described in [RELEASING.md](RELEASING.md).

## Contributing

Pull requests are not accepted yet, but issues are welcome. This will change later. Read
[CONTRIBUTING.md](CONTRIBUTING.md) first. AI tools are allowed but must be disclosed.

| File                                       | What it covers                              |
| ------------------------------------------ | ------------------------------------------- |
| [CONTRIBUTING.md](CONTRIBUTING.md)         | How to report bugs and suggest changes      |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)   | Expected behavior in the community          |
| [SECURITY.md](SECURITY.md)                 | How to report a vulnerability, privately    |
| [SUPPORT.md](SUPPORT.md)                   | Where to ask questions and what to expect   |

## License

Licensed under the [Apache License 2.0](LICENSE).
