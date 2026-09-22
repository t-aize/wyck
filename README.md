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

Licensed under the [Apache License 2.0](LICENSE).
