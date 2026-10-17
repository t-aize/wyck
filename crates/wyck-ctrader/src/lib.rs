//! cTrader Open API SDK: messages, client, session, OAuth and trading calculations.

//! # cTrader Open API client
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
//! amends and cancels orders and closes positions, and `margin` reads and (for its one threshold
//! setting) writes margin call configuration. A trading call moves money: simulated on a demo
//! account, real on a live one, since demo balances are simulated but persistent. See the
//! non-idempotency caveat on [`trading::TradingClient::new_order`] before sending an order from
//! anything that is not a script you are watching.
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
//! | `account` | `account::AccountDataClient`: balance, positions, orders, deals; [`wyck_trading::book::AccountBook`]: the account kept current from the server's messages |
//! | [`trading`] | [`trading::TradingClient`]: placing, amending and cancelling orders; [`wyck_trading::contract`] and [`wyck_trading::risk`]: lots, volumes, pips and risk sizing |
//! | `margin` | `margin::MarginClient`: what a volume would need |
//! | [`session`] | [`session::Session`]: reconnects, renews tokens, restores subscriptions |
//! | [`auth`] | OAuth 2: the consent URL, tokens, refresh, the local redirect listener |
//! | `event` | What the server sends unasked: prices, order book, executions, notices |
//! | [`config`] | Demo or live, timeouts, application credentials |
//! | `error` | [`Error`] and its classification |
//!
//! # Scope
//!
//! Only the workspace uses this crate, so its API changes with the app and carries no promise to
//! outside code. The public API is the documented modules and the root re-exports; the wire format
//! and the connection machinery (`transport`) are private.
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
//! use wyck_ctrader::auth::{authorization_url, new_state, CallbackListener, OAuthClient, Scope};
//! use wyck_ctrader::{ClientBuilder, ClientCredentials, Environment, Event};
//! use secrecy::ExposeSecret;
//! use std::time::Duration;
//!
//! # async fn demo() -> wyck_ctrader::Result<()> {
//! // 1. Sign the user in (once; keep the tokens in a secret store afterwards).
//! let credentials = ClientCredentials::new("my-client-id", "my-client-secret");
//! let listener = CallbackListener::bind(8765).await?;
//! let redirect = listener.redirect_uri();
//! let state = new_state();
//! println!("open: {}", authorization_url(&credentials.client_id, &redirect, Scope::Accounts, &state)?);
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
//! Checked against the official documentation and `.proto` files, and by hand against a live demo account:
//!
//! - **Endpoints**: `demo.ctraderapi.com` and `live.ctraderapi.com`, JSON on port `5036`. Demo and
//!   live are separate: one connection each, and accounts of one cannot be used on the other.
//! - **Limits**: 50 requests per second, 5 per second for history, per connection; the client stays
//!   a little under (40 and 4) and sends a request again, after the wait the server asks for
//!   (`retryAfter` is in **seconds**), when it is refused for its rate (`REQUEST_FREQUENCY_EXCEEDED`,
//!   or `BLOCKED_PAYLOAD_TYPE`, which a live run produced). Silence for more than 10 seconds drops
//!   the connection, hence the heartbeat.
//! - **Prices** are integers scaled by 100 000 ([`wyck_market::PRICE_SCALE`]).
//! - **Ticks** come newest first with their times **and prices** as differences from the tick before
//!   ([`wyck_market::decode_ticks`], confirmed on a live demo account), at most one week per request, bid
//!   and ask requested separately. There is **no volume per tick**: a bar's volume counts ticks
//!   (both bid and ask changes), and only the order book has sizes.
//! - **Bars** are a low price plus offsets, and match the bid ticks of their minute in about 96
//!   percent of minutes; the rest are 1 to 3 units wider, because the bars are built from a richer
//!   feed than the tick history.
//!
//! # What is tested, and what is not
//!
//! Everything above the wire is covered by tests with a local mock server (`cargo test -p
//! wyck-ctrader`). The examples in `crates/wyck-ctrader/examples` run the same calls against a real
//! demo account; nothing automated does. Two points are open: whether the OAuth consent page echoes
//! `state` back (the sign in refuses a redirect without it, see `auth::CallbackListener`; if the
//! real page does not echo it, `allow_missing_state` is the one switch, and it needs a real sign in
//! through a browser to find out), and how long a broker keeps ticks (a demo account's retention
//! is not documented).

mod account;
pub mod auth;
pub mod config;
mod connection;
mod error;
mod event;
mod handle;
mod margin;
pub mod market;
pub mod session;
pub mod trading;
mod transport;

pub use config::{ClientCredentials, ConnectionConfig, Environment};
pub use connection::Client;
pub use connection::builder::ClientBuilder;
pub use error::{Error, ErrorKind, Result};
pub use event::Event;
pub use handle::AccountClient;
pub use transport::messages::TraderAccount;
