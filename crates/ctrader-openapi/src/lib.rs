//! # ctrader-openapi
//!
//! A Rust client for the **cTrader Open API**, over its JSON WebSocket.
//!
//! The two MCP servers cTrader offers (see the `ctrader-mcp` crate) cannot go below one minute:
//! they have no tick history, no tick stream, and no bars under M1. The Open API can. It streams
//! every price change, serves tick history, bars of fourteen periods, live bars and the order book.
//! This crate is a small, typed, tested client for that, with what it takes to use it safely:
//! request matching, heartbeats, the documented rate limits, and the OAuth 2 sign in.
//!
//! **Read only, for now.** Nothing here places or changes an order.
//!
//! # Module map
//!
//! | Module | Role |
//! |---|---|
//! | [`client`] | The connection: [`Client`], one method per call, events, state |
//! | [`history`] | Whole ranges of ticks and bars, fetched page by page |
//! | [`auth`] | OAuth 2: the consent URL, tokens, refresh |
//! | [`callback`] | The loopback web server that catches the sign in redirect |
//! | [`event`] | What the server sends unasked: prices, order book, notices |
//! | [`types`] | Periods, bars, ticks, quotes, price scale, and their decoding |
//! | [`model`] | The messages as plain data |
//! | [`wire`] | The envelope and the payload type numbers |
//! | [`config`] | Demo or live, timeouts, application credentials |
//! | [`rate_limit`] | The request limiter |
//! | [`error`] | [`OpenApiError`] and its classification |
//!
//! # A complete session
//!
//! ```no_run
//! use ctrader_openapi::auth::{authorization_url, new_state, OAuthClient, Scope};
//! use ctrader_openapi::callback::CallbackListener;
//! use ctrader_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
//! use ctrader_openapi::{Client, Event};
//! use secrecy::ExposeSecret;
//! use std::time::Duration;
//!
//! # async fn demo() -> ctrader_openapi::Result<()> {
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
//! let client = Client::connect(&ConnectionConfig::new(Environment::Demo)).await?;
//! client.authenticate_application(&credentials).await?;
//! let access = tokens.access_token.expose_secret();
//! let accounts = client.accounts(access).await?;
//! let account = accounts.ctid_trader_account[0].ctid_trader_account_id;
//! client.authorize_account(account, access).await?;
//!
//! // 3. Follow a symbol's prices.
//! let symbols = client.symbols(account, false).await?;
//! let eurusd = symbols.iter().find(|s| s.symbol_name.as_deref() == Some("EURUSD")).unwrap();
//! let mut events = client.events();
//! client.subscribe_spots(account, &[eurusd.symbol_id], true).await?;
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
//! Checked against the official documentation and `.proto` files (see `TODO.md`, section 2A):
//!
//! - **Endpoints**: `demo.ctraderapi.com` and `live.ctraderapi.com`, JSON on port `5036`. Demo and
//!   live are separate: one connection each, and accounts of one cannot be used on the other.
//! - **Limits**: 50 requests per second, 5 per second for history, per connection; the client stays
//!   a little under (40 and 4) and sends a request again, after the wait the server asks for
//!   (`retryAfter` is in **seconds**), when it is refused for its rate (`REQUEST_FREQUENCY_EXCEEDED`, or
//!   `BLOCKED_PAYLOAD_TYPE`, which a live run produced). Silence for more than 10 seconds drops the
//!   connection, hence the heartbeat.
//! - **Prices** are integers scaled by 100 000 ([`types::PRICE_SCALE`]).
//! - **Ticks** come newest first with their times **and prices** as differences from the tick before
//!   ([`types::decode_ticks`], confirmed on a live demo account), at most one week per request, bid and ask requested separately. There is **no volume per tick**: a bar's
//!   volume counts ticks, and only the order book has sizes.
//!
//! # What has and has not been verified against a live server
//!
//! Everything above the wire is covered by tests with a local mock server. A first run on a demo
//! account (`tests/live.rs`) confirmed the connection, both sign in steps, the symbol list, the price
//! subscription and the tick encoding (prices are differences too), and produced the rate limit
//! behavior described above. Still open: the range limit of bar requests per period, which end a
//! truncated bar answer holds, whether the consent page echoes `state`, and how long a broker keeps
//! ticks. `TODO.md` 2A.7 keeps the list.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod account;
mod account_api;
pub mod auth;
pub mod callback;
pub mod client;
pub mod config;
pub mod error;
pub mod event;
pub mod history;
pub mod model;
pub mod rate_limit;
pub mod types;
pub mod wire;

pub use client::{Client, ConnectionState};
pub use config::{ClientCredentials, ConnectionConfig, Environment};
pub use error::{ErrorKind, OpenApiError, Result};
pub use event::{DisconnectReason, Event};
