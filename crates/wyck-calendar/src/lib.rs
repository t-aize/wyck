//! # wyck-calendar
//!
//! The economic calendar for `wyck`: a typed client for ForexFactory's public weekly
//! feed, a tolerant parser, impact / currency / watchlist filtering, a self-refreshing
//! cached service, and non-blocking "big release incoming" warnings.
//!
//! The crate is UI-agnostic on purpose (no UI toolkit, no terminal) so any front end
//! (a desktop GUI, a headless engine) can consume the same types. It knows nothing
//! about cTrader either; the one bridge to the trading side is
//! [`currencies_from_symbols`], which turns the symbol names a session trades into the
//! currency set for [`EventFilter::currencies`].
//!
//! ## Layers
//!
//! | Layer | Items | Does I/O? |
//! |---|---|---|
//! | Model | [`CalendarEvent`], [`Impact`], [`Scope`], [`Currency`], [`Reading`] | no |
//! | Decode | [`parse_feed`] -> [`Feed`] | no |
//! | Query | [`EventFilter`], [`between`], [`upcoming`], [`imminent_events`] | no |
//! | Fetch | [`CalendarClient`] (one conditional GET), [`Fetch`] trait | yes |
//! | Service | [`CalendarService`] -> [`CalendarHandle`] -> [`CalendarState`] | yes (background) |
//!
//! Everything below the *Fetch* layer is pure and synchronous, so it can be called from
//! a render loop freely.
//!
//! ## The feed, and what this crate does about its quirks
//!
//! Source: `https://nfs.faireconomy.media/ff_calendar_thisweek.json`, which needs no
//! authentication and serves one JSON array per week:
//!
//! ```json
//! {"title":"CPI m/m","country":"CAD","date":"2026-09-14T08:30:00-04:00",
//!  "impact":"High","forecast":"-0.1%","previous":"0.5%"}
//! ```
//!
//! - `country` is a currency code **or `"All"`** -> [`Scope::Global`].
//! - `forecast` / `previous` are free-form display strings, often `""` -> [`Option`];
//!   numeric shapes (`0.1%`, `8.3K`, `-1.00T`) are interpreted on demand by
//!   [`Reading::parse`], and non-numeric ones (`3.65|1.3` auction yield | bid-to-cover,
//!   `3-0-6` vote splits) degrade to [`Reading::Compound`] / [`Reading::Text`] rather
//!   than failing.
//! - `date` carries an explicit UTC offset (US Eastern, DST-aware) ->
//!   [`time::OffsetDateTime`]; instants compare correctly across offsets.
//! - The feed **rate-limits** aggressive clients (`429`) -> the service polls every 30
//!   minutes by default, sends `ETag`/`Last-Modified` validators, and backs off.
//! - **Schema drift** is contained: a malformed record is skipped and counted
//!   ([`Feed::skipped`]); a document where *nothing* decodes is an error, so a good
//!   cache is never replaced by an empty calendar.
//!
//! ## Feed behavior (verified against the live host, September 2026)
//!
//! | Fact | Consequence here |
//! |---|---|
//! | **Rate limit about 2 requests / 5 min / IP.** Beyond it: `429` with `Retry-After: 300` and an HTML body. A `304` may well count too. | [`ServiceConfig`] defaults: poll every 30 min, never two attempts closer than 5 min, retries floored at 5 min, `429` honored via [`DEFAULT_RATE_LIMIT_HOLD`]. Manual refresh cannot bypass any of it. |
//! | Sits behind Cloudflare: `Cache-Control: public, max-age=60`, a (weak) `ETag`, `Last-Modified`; conditional GETs return `304`. | [`Validators`] are replayed on every refresh. |
//! | **Only the current week exists** (Sunday to Saturday). `ff_calendar_nextweek.json` / `ff_calendar_lastweek.json` are `404`. | The calendar is empty of next-week events until the source rolls over; a UI should not present "nothing upcoming" on a Friday evening as "no news". |
//! | Sibling formats exist (`.xml`, `.csv`); the JSON has exactly six fields and **no `actual`** value. | This crate models JSON only; there is no "actual vs forecast" surprise data to compute. |
//! | Block pages are HTML (`Request Denied: you've exceeded the limit for Calendar Export requests`). | Detected as [`CalendarError::HtmlResponse`] (transient, 5-minute hold) instead of a JSON decode error. |
//! | Undocumented and unofficial: no SLA, no published terms for this host. | Everything degrades to "stale data + `last_error`"; nothing panics. Do not build order-blocking logic on it. |
//!
//! Practical corollary: every app restart is a request. Restarting the app repeatedly
//! within minutes will trip the limiter (the service then simply waits out the
//! `Retry-After`).
//!
//! ## Example: run the service and filter
//!
//! ```no_run
//! use wyck_calendar::{
//!     AlertPolicy, CalendarService, EventFilter, Impact, currencies_from_symbols,
//!     imminent_events, upcoming,
//! };
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let calendar = CalendarService::spawn_default()?;
//! let mut updates = calendar.subscribe();
//!
//! // "Currencies I actually trade", from the session's symbol list.
//! let traded = currencies_from_symbols(["EURUSD", "GBPJPY", "XAUUSD"]);
//! let filter = EventFilter::new()
//!     .with_min_impact(Impact::Medium)
//!     .with_currencies(traded.iter().copied())
//!     .watch("fomc");
//!
//! while updates.changed().await.is_ok() {
//!     let state = calendar.state();
//!     let now = time::OffsetDateTime::now_utc(); // or broker server time
//!
//!     for event in filter.apply(upcoming(&state.events, now, 20)) {
//!         println!("{} {} {}", event.time, event.scope, event.title);
//!     }
//!     for warning in imminent_events(&state.events, now, &AlertPolicy::default()) {
//!         println!("heads up: {:?}", warning.timing);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Errors
//!
//! Fetch problems are values ([`CalendarError`]), never panics, and the service turns
//! them into [`CalendarState::last_error`] while keeping the last good data: see
//! [`Freshness`]. Failures are logged through `tracing`; this crate installs no
//! subscriber.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod alert;
mod client;
mod currency;
mod error;
mod event;
mod filter;
mod parse;
mod reading;
mod service;

pub use alert::{AlertPolicy, EventWarning, Timing, imminent_events};
pub use client::{CalendarClient, ClientConfig, Fetch, FetchOutcome, THIS_WEEK_URL, Validators};
pub use currency::{Currency, ParseCurrencyError, currencies_from_symbol, currencies_from_symbols};
pub use error::{CalendarError, Result};
pub use event::{CalendarEvent, EventId, Impact, Scope, between, upcoming};
pub use filter::EventFilter;
pub use parse::{Feed, parse_feed};
pub use reading::{Reading, Unit};
pub use service::{
    CalendarHandle, CalendarService, CalendarState, DEFAULT_RATE_LIMIT_HOLD, Freshness,
    ServiceConfig,
};
