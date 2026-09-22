# wyck

A terminal trading panel for cTrader, built for speed, not for staring at it.

A single crate, `src/config` and `src/openapi`:

- `config`: app configuration and encrypted credential storage.
- `openapi`: a client for the cTrader Open API, over its JSON WebSocket. See its module docs
  (`cargo doc --open`, module `wyck::openapi`) for the full guide: quick start, signing in,
  streaming prices, history, trading, margin, errors, and the protocol coverage table.

```sh
cargo build
cargo test
```

`cargo test` never touches the network: it is entirely mock servers and unit tests. Testing
`openapi` against a real cTrader demo account, end to end, is a separate opt-in step; see
`scripts/README.md` and `.env.example` to set it up, then:

```sh
cargo test --test live -- --ignored --nocapture
```

Licensed under the [Apache License 2.0](LICENSE).
