# scripts

Operational tooling for testing `wyck::openapi` against a real cTrader **demo** account: signing
in, keeping tokens fresh, and a fast sanity check before running the full live test suite. Not
usage examples of the library (see the module docs, `cargo doc --open`, for those) -- these are
things you run, not code you read to learn the API.

Each one is a normal `[[bin]]` target (see the root `Cargo.toml`), a small [clap](https://docs.rs/clap)
CLI with `--help`, and reads settings from `.env` (via [dotenvy](https://docs.rs/dotenvy)) as
much as from real flags -- a flag always wins if both are given.

## Setup

```sh
cp .env.example .env
```

Fill in `WYCK_OPENAPI_CLIENT_ID` and `WYCK_OPENAPI_CLIENT_SECRET` by hand (register an application
first: see the comment at the top of `.env.example`). Everything else in `.env` is either optional
or filled in by the scripts below.

## `sign-in`

```sh
cargo run --bin sign-in -- --write-env
```

Opens the cTrader consent page (or prints its URL, with `--no-open`), waits for you to sign in and
grant access, trades the resulting code for an access/refresh token pair, connects to list every
trading account that token covers, and with `--write-env` saves the access token, refresh token
and the chosen account's id straight into `.env`.

Ask for the trading scope (`--scope trading`) only if you intend to run the trading live test
(`place_and_close_a_minimal_market_order_on_a_demo_account`); the default, `accounts`, is enough
for everything else and cannot place an order even if asked to.

## `refresh-tokens`

```sh
cargo run --bin refresh-tokens -- --write-env
```

Trades the refresh token in `.env` for a fresh pair against the real token endpoint. Useful
before a long test session, or once the access token from `sign-in` is close to its ~30 day
lifetime. The old refresh token stops working the moment this succeeds, so run it with
`--write-env` (or copy the printed pair into `.env` yourself right away): losing the new pair
means signing in again from scratch with `sign-in`.

## `check-connection`

```sh
cargo run --bin check-connection
```

A few seconds: connects, authenticates the application, authorizes the account, and reads one
thing from each of the four sub-clients (account data, market, margin). Confirms a `.env` is
actually usable before running the much slower live test suite below -- a wrong client id, an
expired token or a wrong account id shows up here in seconds instead of partway through it.

## Running the live tests

Once `check-connection` is happy:

```sh
cargo test --test live -- --ignored --nocapture
```

See the module doc comment at the top of `tests/live.rs` for what each test covers and the one
extra opt-in (`WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`) the trading test needs on top of `--ignored`.
