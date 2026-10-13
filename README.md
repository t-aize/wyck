<p align="center">
  <img src="assets/logo.png" alt="Wyck" width="120">
</p>

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

An update replaces the program only; your settings, drawings and alerts stay where they are. Just
before it installs, Wyck keeps a copy of everything you made (Settings > Data and backup, "Before
an update", the last three are kept). At the next start, a file saved by an older version is
brought to the layout of the new one, and the old text stays next to it as `<name>.toml.v<n>.bak`.
A file saved by a newer version (after going back to an older Wyck) is never read wrongly or
rewritten: it is set aside as `<name>.toml.from-v<n>` and the older version starts that document
afresh.

## Layout

One Cargo package, `wyck`: `src/lib.rs` declares the modules, `src/main.rs` only starts the app.
The code is in four layers, and a lower layer never imports a higher one (a test checks it):

| Layer | Owns |
|---|---|
| `src/domain` | Pure logic: market data, trading maths, indicators, drawings, chart models |
| `src/infra` | The cTrader Open API client, settings and credential storage, the async runtime |
| `src/app` | State and use cases: sign-in, account, alerts, preferences, market data |
| `src/ui` | GPUI screens: the widget kit (`ui/kit`), the window and dashboard (`ui/shell`), the features |

Every control, menu, dialog and color comes from the widget kit. The Open API client is described
in the rustdoc of `infra::ctrader` (`cargo doc --open`).

## Your own indicators

Indicators can be written as scripts (in [Rhai](https://rhai.rs)) and kept in a folder that the
app reads on its own. The header has a button for the folder and for a full editor.

## Where your data is kept

Settings > Folders shows where each kind of data lives and lets you move it, one kind at a time:

| Kind | What is in it |
|---|---|
| Settings and themes | look, themes, colors, layout, shortcuts, favorites, ticket and indicator settings |
| Account data | drawings and their saved looks, watchlists and alerts, one folder per account number |
| Indicator scripts | the `.rhai` files you write |
| Backups | the copies kept for restoring, automatic ones included |
| Logs | the log of each day and the report of a crash |

**Keeping several computers in step.** "Use OneDrive" (or Dropbox, Google Drive, iCloud Drive, or
"Choose another folder...") puts the first four kinds in a folder called `Wyck` inside the folder
your service keeps in step (`Wyck (dev)` for a development build). Install Wyck on the other
computer, open Settings > Folders and pick the same folder: it uses what is there. Your logs stay
local.

What happens when you move a kind:

- Nothing moves while Wyck runs. The request waits, and the next start makes it: the files are
  copied one by one, read back and compared, and only then is the new place remembered. A failure
  leaves everything where it was.
- The old folder is kept beside where it was, renamed `<name>.moved-<date>`, until you delete it
  from the same page. Nothing is lost if you change your mind.
- A folder that already holds data of Wyck (the first computer's) is used as it is, and nothing
  from this computer is copied over it; what this computer had stays in the renamed old folder.
- A folder that holds other files is never mixed with: Wyck makes a folder named after the kind
  inside it.
- If the folder cannot be reached at start (the service has not mounted it), the standard folder
  is used for that run, you are told, and the choice is kept for when it is back.

What is never moved: the keys that sign in to your broker (the system keyring), and the list of
accounts you signed in with, which is tied to those keys on this computer. A synced folder holds
nothing that signs in. The data of an account is found again by its account number, so signing in
on the second computer brings back the same drawings and alerts.

Things to know about a synced folder: use Wyck on one computer at a time, because a service cannot
merge two changes of the same file (the last one saved wins, and the other is kept as a copy such
as `preferences-LAPTOP.toml`, which Wyck does not read; Settings > Folders lists these copies).
The indicators folder you pick in Settings > Indicators is remembered by each computer on its own,
never in the settings, since a path means something on one computer only.

**Development and installed app.** Each has its own standard folders and its own file of choices.
A moved folder carries a small marker, `wyck-location.toml`, saying which build made it and for
which kind of data. A development build refuses a folder marked by the installed app and the other
way round, and a folder of one kind is never taken for another. `WYCK_CONFIG_DIR` moves the folder
the choices are read from, as for everything else.

## Keyboard shortcuts

Every shortcut can be changed in Settings > Shortcuts (Ctrl+, opens the settings; Cmd+, on a
Mac). Click a key and press the new combination, add a second key to an action, leave an action
with none, or put the default back. A combination that another action of the same place already
uses is refused with the name of that action. The hints in the tooltips and menus follow what you
set.

## Accessibility

- **Keyboard.** Tab reaches the controls, including the buttons of the account panel. Modal
  windows keep Tab inside themselves and give the focus back when they close.
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
cargo test
```

`cargo test` never touches the network: it is entirely mock servers and unit tests.

The first build is slow because the dependency graph is large and dependencies are compiled
with optimizations even in debug builds (see `[profile.dev.package."*"]` in `Cargo.toml`).
Later builds only recompile what changed in this package.

## Development build and installed app

A debug build (`cargo run`) keeps its state apart from the installed app, so testing cannot touch
your real settings, backups or sign-in. The page Settings > About shows which folder and which
keyring service the running build uses.

| | Installed app | Debug build |
|---|---|---|
| Settings folder (Windows) | `%APPDATA%\wyck\wyck\config` | `%APPDATA%\wyck\wyck-dev\config` |
| Settings folder (macOS) | `~/Library/Application Support/sh.wyck.wyck` | `~/Library/Application Support/sh.wyck.wyck-dev` |
| Settings folder (Linux) | `~/.config/wyck` | `~/.config/wyck-dev` |
| Keyring service | `wyck` | `wyck-dev` |
| Log folder (Windows) | `%LOCALAPPDATA%\wyck\wyck\data\logs` | `%LOCALAPPDATA%\wyck\wyck-dev\data\logs` |
| Log folder (macOS) | `~/Library/Logs/sh.wyck.wyck` | `~/Library/Logs/sh.wyck.wyck-dev` |
| Log folder (Linux) | `~/.local/state/wyck/logs` | `~/.local/state/wyck-dev/logs` |

A debug build starts empty and needs its own cTrader sign-in. Three environment variables change
that:

- `WYCK_CONFIG_DIR` moves the settings folder (and the data folder with it).
- `WYCK_DATA_DIR` moves the data folder alone.
- `WYCK_KEYRING_SERVICE` names the keyring service.

On a machine with no keyring (Linux without a Secret Service), Wyck cannot keep the sign-in.
Set `WYCK_PASSPHRASE` before starting it and the credentials are kept in encrypted files in the
data folder, locked by that passphrase. The same passphrase must be set at every start.

To run a debug build on the data of the installed app (the real thing, so take care), point it at
the same places:

```powershell
$env:WYCK_CONFIG_DIR = "$env:APPDATA\wyck\wyck\config"
$env:WYCK_KEYRING_SERVICE = "wyck"
cargo run
```

To start the debug build from a copy of the installed settings instead, copy the folder once
(close the app first) and leave the sign-in to be done again:

```powershell
Copy-Item -Recurse "$env:APPDATA\wyck\wyck\config" "$env:APPDATA\wyck\wyck-dev\config"
```

## Logs and crashes

The installed app has no console, so it writes what it does to a file: `wyck.YYYY-MM-DD.log` in
the log folder above, one per day, the last 14 kept. It records the start, the sign-in and
connection steps, saved copies, updates and warnings, never a token, a secret or an
authorization code. Settings > About > "Open the log folder" opens it.

When the app panics it also writes `panic-YYYYMMDD-HHMMSS-<pid>-<n>.txt` there (the message, the
place in the code, the thread, the version and a backtrace); the last 10 are kept. Attach the
end of the log and that report to a bug report, after reading them for anything private.

`RUST_LOG` changes how much is written, to the file and to stderr alike (for example
`RUST_LOG=wyck=debug`). Without it the file gets `info` for Wyck and `warn` for the rest, and
stderr gets `debug` for Wyck. `WYCK_DATA_DIR` moves the log folder with the rest of the data
(to a `logs` folder inside it).

## Test against a real demo account

Testing the cTrader client against a real cTrader demo account, end to end, is a separate opt-in step.
Fill in `.env` from [.env.example](.env.example), then:

```sh
cargo test --test ctrader live -- --ignored --nocapture
```

## Checks

The same checks run in CI on Linux, Windows and macOS:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

`cargo test` also keeps the writing of the repository plain (no typographic dashes, quotes or
emoji, Rust sources in ASCII: `tests/writing_style.rs`), and clippy refuses `unwrap` and `expect`
outside tests (`clippy.toml`).

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
