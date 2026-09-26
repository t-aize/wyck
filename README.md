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
marks bundled in `crates/wyck/assets/marks` are trademarks of their owners, shown only to identify the
instrument being traded, and are used under the license of each set (see the `LICENSE.txt`
next to it).

## Layout

The root manifest only configures the workspace. `cargo run` starts the desktop crate `wyck`.
The workspace crates are:

| Crate | Owns |
|---|---|
| `wyck-openapi-model` | cTrader messages, wire types and API errors |
| `wyck-openapi` | WebSocket client, OAuth and reconnecting session |
| `wyck-config` | Native settings, documents and credential storage |
| `wyck-chart` | Chart data, calculations, studies, drawings and scene commands |
| `wyck-trading` | Trading calculations, books and saved panel preferences |
| `wyck-state` | Saved workspace preferences and layouts |
| `wyck` | GPUI desktop application and bundled assets |

Each crate's modules sit directly under its `src` directory. `crates/wyck/src/app` owns GPUI
views and connects the crates to the desktop. Its shared controls are in
`crates/wyck/src/app/ui` and native app services are in `crates/wyck/src/app/services`.
The Open API guide is in the `wyck-openapi` crate documentation.

## Your own indicators

Indicators can be written as scripts (in [Rhai](https://rhai.rs)) and kept in a folder that the
app reads on its own. The header has a button for the folder and for a full editor.

## Requirements

- A recent stable Rust toolchain (the minimum is `rust-version` in `Cargo.toml`).
- Linux only: the development packages of fontconfig, Wayland, OpenSSL, X11 and xkbcommon, for example on Debian and Ubuntu
  `pkg-config libfontconfig-dev libwayland-dev libssl-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev`.

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

Testing `openapi` against a real cTrader demo account, end to end, is a separate opt-in step.
Fill in `.env` from [.env.example](.env.example), then:

```sh
cargo test -p wyck-openapi --test live -- --ignored --nocapture
```

## Checks

The same checks run in CI on Linux and Windows:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

Dependencies are checked with [cargo-deny](https://github.com/EmbarkStudios/cargo-deny)
(`cargo install cargo-deny --locked`, then `cargo deny check`) and updated by Dependabot.

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
