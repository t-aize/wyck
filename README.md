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
cTrader or Spotware Systems**. cTrader is a trademark of its owner.

## Install

Download the archive for your system from the
[latest release](https://github.com/t-aize/wyck/releases/latest), unpack it and run `wyck`.
The files are `wyck_<version>_<platform>.tar.gz` (Linux, macOS) or `.zip` (Windows), next to a
`SHA256SUMS` file and a GitHub artifact attestation. The binaries are not signed for macOS
Gatekeeper or Windows SmartScreen, which may warn on first run. Or build it yourself, see below.

## First start

1. Create an application on the cTrader Open API portal and register
   `http://localhost:8765` as its redirect address.
2. Run `wyck`. Type the client ID and the client secret of the application, pick Demo or Live
   (`Ctrl+E`), and press Enter.
3. Approve the access in the browser. If no browser opens, open the address shown in the terminal.
4. Pick the trading account. The connection is saved: the next start goes straight to the
   dashboard.

The client secret and the tokens go to the OS keyring. When there is none (a server, an SSH
session), wyck asks for a passphrase at every start and keeps them in encrypted files instead.

`wyck --reset` forgets the saved connection. `wyck --config-dir <folder>` keeps everything in one
folder.

`wyck --preview` opens the interface with sample prices, positions and orders. It does not
connect, open the keyring or write configuration files.

## Keys

| Key | Action |
|---|---|
| `F1` to `F4` | watchlist, positions, orders, console |
| `Shift+Tab`, `F6` | switch between command input and tables |
| `/` (tables) | focus command input |
| `Tab` (input) | complete a command name |
| Up / Down (input) | command history, then restore the draft |
| Left / Right, Home / End | move the input cursor |
| `Ctrl+A` / `Ctrl+E`, `Ctrl+U` / `Ctrl+K`, `Ctrl+W` | move to start/end, erase before/after cursor, erase a word |
| `PgUp` / `PgDn` | scroll the activity log |
| `Esc` | clear input, switch focus when empty, or cancel a popup |
| `Ctrl+C` | cancel a popup or clear input, then quit when empty |
| `1` to `4`, `Tab` (tables) | switch views |
| `j` / `k`, arrows (tables) | select a row |
| `a` / `d`, `b` / `s` (watchlist) | add/remove a symbol, open buy/sell ticket |
| `x` (positions), `c` (orders) | confirm a close or cancellation |
| `o`, `q` (tables) | confirm sign out, quit |

## Commands

The command input starts focused. Commands work with or without a leading `/`. Results and
server events stay in the activity log; the Console view gives the log the full workspace.

```text
/help
/add EURUSD
/watchlist
/positions
/orders
/buy EURUSD 0.01 --sl 1.08000 --tp 1.09500
/sell EURUSD 0.01 --sl 1.09500 --tp 1.08000
/close 1042
/cancel 2084
/remove EURUSD
/refresh
/console
/clear
/logout
/quit
```

Add a symbol to the watchlist before trading it. Buy and sell open an editable ticket, close
and cancel open a confirmation. Lots must match the broker's limits and volume step. SL/TP
arguments are prices; market orders send their distance from the current ask (buy) or bid
(sell), so their final levels follow the actual fill. Pasting text never submits it.

The one-line header follows the original OpenTUI layout: symbol, bid/ask, spread and price
history, then connection, balance, Paris clock and market sessions. Session and ICT windows
use each city's time zone and local weekdays. They are time windows, not the broker's trading
schedule. Wider spreads turn red relative to recent quotes; XAUUSD also keeps its original
absolute threshold of 1 in price units. Narrow terminals omit secondary header information.

Orders pass through the risk guard in `src/trading/guard.rs`. Limits are read from the `risk`
document of the account (see below); with none set, only the price collar applies.

## Where the files go

| OS | Config folder |
|---|---|
| Linux | `~/.config/wyck` |
| macOS | `~/Library/Application Support/sh.wyck.wyck` |
| Windows | `%APPDATA%\wyck\config` |

`WYCK_CONFIG_DIR` and `WYCK_DATA_DIR` move them. `config.toml` holds the profiles,
`scopes/<demo|live>-<account>/` holds the documents of an account (`watchlist.toml`,
`risk.toml`), and `wyck.log` in the data folder is the log.

## Layout

| Path | Owns |
|---|---|
| `src/lib.rs`, `src/main.rs` | the library and the `wyck` binary |
| `src/openapi` | cTrader Open API: messages, WebSocket client, OAuth, reconnecting session, account book |
| `src/config` | profiles, documents, credential storage (keyring or encrypted files) |
| `src/trading` | lot and money math, trade plans, risk guard |
| `src/tui` | the terminal interface |
| `tests` | integration tests against mock servers, and the ignored live tests |

## Requirements

- A recent stable Rust toolchain (the minimum is `rust-version` in `Cargo.toml`, currently 1.98).
  `rust-toolchain.toml` makes rustup pick stable; run `rustup update` if yours is older.
- Linux only: `pkg-config` and the OpenSSL headers (`libssl-dev` on Debian and Ubuntu).

## Build and test

```sh
cargo run
cargo test
```

`cargo test` never touches the network: it is mock servers and unit tests.

## Test against a real demo account

Fill in `.env` from [.env.example](.env.example), then:

```sh
cargo test --test live -- --ignored --nocapture
```

## Checks

The same checks run in CI on Linux, Windows and macOS:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Dependencies are checked with [cargo-deny](https://github.com/EmbarkStudios/cargo-deny)
(`cargo install cargo-deny --locked`, then `cargo deny check`) and updated by Dependabot.

Releases are described in [RELEASING.md](RELEASING.md).

## Contributing

Pull requests are not accepted yet, but issues are welcome. Read
[CONTRIBUTING.md](CONTRIBUTING.md) first. AI tools are allowed but must be disclosed.

| File                                       | What it covers                              |
| ------------------------------------------ | ------------------------------------------- |
| [CONTRIBUTING.md](CONTRIBUTING.md)         | How to report bugs and suggest changes      |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)   | Expected behavior in the community          |
| [SECURITY.md](SECURITY.md)                 | How to report a vulnerability, privately    |
| [SUPPORT.md](SUPPORT.md)                   | Where to ask questions and what to expect   |

## License

Licensed under the [Apache License 2.0](LICENSE).
