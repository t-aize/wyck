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

## Keys

| Key | Action |
|---|---|
| `1` `2` `3`, `Tab` | watchlist, positions, orders |
| `j` `k`, arrows | move |
| `a`, `d` | add or remove a symbol (watchlist) |
| `b`, `s` | buy or sell the selected symbol at market (watchlist) |
| `x` | close the selected position |
| `c` | cancel the selected order |
| `o` | sign out and forget the connection |
| `q` | quit |

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
| `src/openapi` | cTrader Open API: messages, WebSocket client, OAuth, reconnecting session, account book |
| `src/config` | profiles, documents, credential storage (keyring or encrypted files) |
| `src/trading` | lot and money math, trade plans, risk guard |
| `src/tui` | the terminal interface |

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
cargo test openapi::tests::live -- --ignored --nocapture
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
