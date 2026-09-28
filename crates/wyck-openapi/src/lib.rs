//! cTrader Open API SDK: messages, client, session, OAuth and trading calculations.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # wyck-openapi
//!
//! A Rust SDK for the **cTrader Open API**, over its JSON WebSocket.
//!
//! It streams every price change, serves tick history, bars of fourteen periods, live bars and
//! the order book. This crate is a typed, tested client for that, with what it takes to use it
//! for days without babysitting: request matching, heartbeats, the documented rate limits and
//! their retries, the OAuth 2 sign in, and a [`session::Session`] that reconnects and renews its
//! tokens by itself.
//!
//! **Reads and trades.** Alongside the read-only account and market data, [`trading`] places,
//! amends and cancels orders and closes positions, and [`margin`] reads and (for its one threshold
//! setting) writes margin call configuration. A trading call moves money: simulated on a demo
//! account, real on a live one, since demo balances are simulated but persistent. See the "Trading
//! safety" section of the README, and the non-idempotency caveat on
//! [`trading::TradingClient::new_order`], before sending an order from anything that is not a
//! script you are watching.
//!
//! # Module map
//!
//! `Client -> AccountClient -> { MarketClient, AccountDataClient, TradingClient, MarginClient }`,
//! with [`session::Session`] built on the same [`Client`] for programs that stay up.
//!
//! | Module | Role |
//! |---|---|
//! | (root) | [`Client`], [`ClientBuilder`]: the connection; [`AccountClient`]: a client bound to one account, routing to the four below |
//! | [`market`] | [`market::MarketClient`]: symbols, live prices, the order book, history |
//! | [`account`] | [`account::AccountDataClient`]: balance, positions, orders, deals; [`account::book::AccountBook`]: the account kept current from the server's messages |
//! | [`trading`] | [`trading::TradingClient`]: placing, amending and cancelling orders; [`trading::contract`]: lots, volumes, pips, risk sizing and profit |
//! | [`margin`] | [`margin::MarginClient`]: expected margin, margin calls, dynamic leverage |
//! | [`session`] | [`session::Session`]: reconnects, renews tokens, restores subscriptions |
//! | [`auth`] | OAuth 2: the consent URL, tokens, refresh, the local redirect listener |
//! | [`event`] | What the server sends unasked: prices, order book, executions, notices |
//! | [`config`] | Demo or live, timeouts, application credentials |
//! | [`error`] | [`Error`] and its classification |
//! | [`prelude`] | A group import of the pieces most programs need |
//!
//! # Features
//!
//! `client` (on by default) brings the connection, the session and OAuth, with tokio and the
//! WebSocket and HTTP stacks. Without it the crate holds only the messages, the market types and
//! the trading calculations, for code that works on prices without talking to the server.
//!
//! # Stability
//!
//! The crate follows [semantic versioning](https://semver.org). Before 1.0, a minor release
//! (0.4 to 0.5) may break the API and a patch release does not. What the promise covers:
//!
//! - Everything reachable from the documented modules and the root re-exports.
//! - The server's messages, the events and the error type are `#[non_exhaustive]`: a field or a
//!   variant the server gains is added in a patch release, so build them through their
//!   constructors or from JSON, match with a wildcard arm, and prefer [`Error::kind`] to the
//!   error's variants.
//!
//! What it does not cover: items hidden from these docs (the `transport` module, the wire
//! envelope, `event::event_from`). They are public only for this crate's own tests.
//!
//! Changes are listed in `CHANGELOG.md`, next to this crate's manifest.
//!
//! # Which layer to use
//!
//! - **A script or a tool** that runs for a minute: [`Client`] directly (or [`ClientBuilder`] to
//!   connect and identify the application in one step). Connect, sign in, ask, close.
//! - **A program that stays up** (a recorder, a chart feed): [`session::Session`]. It owns the
//!   connection, and you only read its events and say what to subscribe to.
//! - **Your own supervision**: [`Client`] plus [`Event::Disconnected`]; the client never
//!   reconnects on its own, so you decide how.
//!
//! # A complete run
//!
//! ```no_run
//! use wyck_openapi::auth::{authorization_url, new_state, CallbackListener, OAuthClient, Scope};
//! use wyck_openapi::{ClientBuilder, ClientCredentials, Environment, Event};
//! use secrecy::ExposeSecret;
//! use std::time::Duration;
//!
//! # async fn demo() -> wyck_openapi::Result<()> {
//! // 1. Sign the user in (once; keep the tokens in a secret store afterwards).
//! let credentials = ClientCredentials::new("my-client-id", "my-client-secret");
//! let listener = CallbackListener::bind(8765).await?;
//! let redirect = listener.redirect_uri();
//! let state = new_state();
//! println!("open: {}", authorization_url(&credentials.client_id, &redirect, Scope::Accounts, &state));
//! let code = listener.wait(&state, Duration::from_secs(300)).await?;
//! let oauth = OAuthClient::new(credentials.clone())?;
//! let tokens = oauth.exchange_code(code.code(), &redirect).await?;
//!
//! // 2. Connect, identify the application, authorize an account.
//! let client = ClientBuilder::new(Environment::Demo)
//!     .credentials(credentials)
//!     .connect()
//!     .await?;
//! let access = tokens.access_token.expose_secret();
//! let accounts = client.accounts(access).await?;
//! let account = client.account(accounts.ctid_trader_account[0].ctid_trader_account_id);
//! account.authorize(access).await?;
//!
//! // 3. Follow a symbol's prices, through the market sub-client.
//! let market = account.market();
//! let symbols = market.symbols().await?;
//! let eurusd = symbols.iter().find(|s| s.symbol_name.as_deref() == Some("EURUSD")).unwrap();
//! let mut events = client.events();
//! market.subscribe_spots(&[eurusd.symbol_id]).await?;
//! while let Ok(event) = events.recv().await {
//!     match event {
//!         Event::Spot(spot) => println!("{:?} {:?}", spot.bid, spot.ask),
//!         Event::Disconnected(reason) => { println!("gone: {reason:?}"); break; }
//!         _ => {}
//!     }
//! }
//! # Ok(()) }
//! ```
//!
//! # Facts that shape the design
//!
//! Checked against the official documentation and `.proto` files, and against a live demo account
//! (see the section below):
//!
//! - **Endpoints**: `demo.ctraderapi.com` and `live.ctraderapi.com`, JSON on port `5036`. Demo and
//!   live are separate: one connection each, and accounts of one cannot be used on the other.
//! - **Limits**: 50 requests per second, 5 per second for history, per connection; the client stays
//!   a little under (40 and 4) and sends a request again, after the wait the server asks for
//!   (`retryAfter` is in **seconds**), when it is refused for its rate (`REQUEST_FREQUENCY_EXCEEDED`,
//!   or `BLOCKED_PAYLOAD_TYPE`, which a live run produced). Silence for more than 10 seconds drops
//!   the connection, hence the heartbeat.
//! - **Prices** are integers scaled by 100 000 ([`market::PRICE_SCALE`]).
//! - **Ticks** come newest first with their times **and prices** as differences from the tick before
//!   ([`market::decode_ticks`], confirmed on a live demo account), at most one week per request, bid
//!   and ask requested separately. There is **no volume per tick**: a bar's volume counts ticks
//!   (both bid and ask changes), and only the order book has sizes.
//! - **Bars** are a low price plus offsets, and match the bid ticks of their minute in about 96
//!   percent of minutes; the rest are 1 to 3 units wider, because the bars are built from a richer
//!   feed than the tick history.
//!
//! # What has and has not been verified against a live server
//!
//! Everything above the wire is covered by tests with a local mock server. Live runs on a demo
//! account (`crates/wyck-openapi/tests/live.rs`) confirmed the connection, both sign in steps, the symbol list, the
//! price subscription, the tick encoding, tick and bar history with paging and no lost tick at page
//! seams, the rate limit behavior described above, the account and margin calls, [`session::Session`]
//! end to end (including reconnecting after a real, not simulated, connection drop), and the whole
//! trading path: placing, amending and cancelling a pending order, amending a position's stop loss
//! and take profit, and placing and closing a market order. Still open: whether the OAuth consent
//! page echoes `state` back (checked by [`auth::AuthorizationCode::state_echoed`] during sign in,
//! but not by an automated test: it needs a real sign in
//! through a browser, which nothing here drives) and exactly how long a broker keeps ticks (a demo
//! account's retention is not documented; `crates/wyck-openapi/tests/live.rs`'s `tick_history_retention_is_reported`
//! narrows it by probing rather than asserting a fixed answer).

pub mod account;
#[cfg(feature = "client")]
pub mod auth;
#[cfg(feature = "client")]
pub mod config;
pub mod error;
pub mod event;
#[cfg(feature = "client")]
mod handle;
pub mod margin;
pub mod market;
#[cfg(feature = "client")]
pub mod prelude;
#[cfg(feature = "client")]
pub mod session;
pub mod trading;
/// The wire format and the connection machinery. Public only for this crate's own tests: not
/// part of the stable API, and it may change in any release.
#[doc(hidden)]
pub mod transport;

pub(crate) use account::types::number_enum;
#[cfg(feature = "client")]
pub use config::{ClientCredentials, ConnectionConfig, Environment};
#[allow(deprecated)]
pub use error::OpenApiError;
pub use error::{Error, ErrorKind, Result};
pub use event::{DisconnectReason, Event};
#[cfg(feature = "client")]
pub use handle::AccountClient;
#[cfg(feature = "client")]
pub use transport::connection::{Client, ClientBuilder, ConnectionState};
pub use transport::messages::{AccountsRes, CtidProfile, RefreshTokenRes, TraderAccount};
