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

## Layout

A single crate:

- `src/app`: the application.
- `src/config`: app configuration and encrypted credential storage.
- `src/openapi`: a client for the cTrader Open API, over its JSON WebSocket. See its module
  docs (`cargo doc --open`, module `wyck::openapi`) for the full guide: quick start, signing
  in, streaming prices, history, trading, margin, errors, and the protocol coverage table.
- `examples/demo_server.rs`: a local stand-in for the Open API, see below.
- `scripts/`: sign-in and connection tooling for a real demo account, see
  [scripts/README.md](scripts/README.md).

## Requirements

- A recent stable Rust toolchain (the minimum is `rust-version` in `Cargo.toml`).
- Linux only: the X11 and xkbcommon development packages, for example on Debian and Ubuntu
  `libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev`.

## Build and test

```sh
cargo build
cargo test
```

`cargo test` never touches the network: it is entirely mock servers and unit tests.

The first build is slow because the dependency graph is large and dependencies are compiled
with optimizations even in debug builds (see `[profile.dev.package."*"]` in `Cargo.toml`).
Later builds only rebuild this crate.

## Try it without a cTrader account

The demo server is a local stand-in for the Open API with made-up prices and one demo
account. Start it, then point the app at it:

```sh
cargo run --example demo_server
WYCK_DEMO_SERVER=ws://127.0.0.1:5035 cargo run
```

## Test against a real demo account

Testing `openapi` against a real cTrader demo account, end to end, is a separate opt-in step.
See `scripts/README.md` and `.env.example` to set it up, then:

```sh
cargo test --test live -- --ignored --nocapture
```

## Checks

The same checks run in CI on Linux and Windows:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
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
