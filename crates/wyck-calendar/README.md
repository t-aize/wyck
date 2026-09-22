# wyck-calendar

A Rust client for Forex Factory's weekly economic calendar export. It fetches the
JSON feed, parses events, filters them by currency and impact, and keeps the last
successful result in a background service. It reports warnings around scheduled
events; it never blocks an order.

Contents: [Quick start](#quick-start) | [API](#api) | [Configuration](#configuration) |
[Feed limits](#feed-limits) | [Errors](#errors) | [Tests](#tests) |
[Production assessment](#production-assessment) | [Sources](#sources)

## Quick start

The public feed needs no credentials. Start the service inside a Tokio runtime:

```rust
use wyck_calendar::{CalendarService, EventFilter, Freshness, Impact, upcoming};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let calendar = CalendarService::spawn_default()?;
let mut updates = calendar.subscribe();
updates.changed().await?;

let state = calendar.state();
if state.freshness() == Freshness::Fresh {
    let filter = EventFilter::new().with_min_impact(Impact::High);
    let now = time::OffsetDateTime::now_utc();
    for event in filter.apply(upcoming(&state.events, now, 10)) {
        println!("{}: {}", event.time, event.title);
    }
}
# Ok(())
# }
```

`state()` is a cheap snapshot. Keep the handle alive while the service runs. Drop
the last handle to stop it. `subscribe()` returns a Tokio watch receiver for
changes; `refresh()` requests an earlier fetch, subject to the rate limit and
retry delay.

## API

| Need | Use |
|---|---|
| Fetch one response | `CalendarClient::fetch` and `FetchOutcome` |
| Parse supplied JSON | `parse_feed` and `Feed` |
| Run automatic refresh | `CalendarService`, `CalendarHandle`, `CalendarState` |
| Filter events | `EventFilter`, `between`, `upcoming` |
| Show release warnings | `imminent_events` and `AlertPolicy` |
| Interpret a forecast or previous value | `CalendarEvent::forecast_reading`, `previous_reading` |
| Select currencies from traded symbols | `currencies_from_symbols` |

Events are sorted by scheduled instant. The parser skips malformed records and
counts them in `Feed::skipped`. A document with records but no valid events is an
error. An empty JSON array is accepted as an empty calendar. Forecast and
previous values remain display strings; interpreting them as numbers is optional.

The default alert policy reports high-impact events from 30 minutes before their
scheduled time until 15 minutes after it. Pass a sorted event slice and an
explicit `now` to `imminent_events`. Use a trusted clock and inspect
`CalendarState::freshness()` and `fetched_at` before presenting an alert as
current. Event times are scheduled estimates and can change.

## Configuration

`ClientConfig` defaults to the HTTPS weekly JSON export, a 15-second request
timeout, a 2 MiB response cap, and a versioned user agent. Its URL can be
changed, including to HTTP for a local test server. Use only trusted HTTPS URLs
in a deployed application.
Redirects are rejected, so the client cannot silently follow a feed URL to a
different host or to plain HTTP.

`ServiceConfig` defaults to a 30-minute refresh, a five-minute minimum gap
between attempts, and retry delays from five to 30 minutes. A `429` response
can impose a longer delay through `Retry-After`. The service keeps successful
events after a failed refresh and exposes the error in `last_error`.

The limit applies to one service instance. Several processes behind the same
public IP do not share a request budget. Coordinate them or use one shared
fetcher. A restart makes a new request.

## Feed limits

The source is a weekly export, not a documented, versioned API. It does not
provide a service-level guarantee. The JSON export includes the event title,
country or currency, scheduled date, impact, forecast, and previous value. It
does not supply the released actual value. The website can show more fields
than this export.

The crate cannot guarantee that the feed is complete, timely, or available.
Scheduled times are approximate. An empty list of upcoming events does not
prove that no market-moving news is coming, particularly near a week boundary.
Never use this source alone as an order-safety gate.

The default delays reflect observed throttling, not a published rate-limit
contract. HTTP `429`, an HTML block page, network failure, and schema changes
are all possible. Monitor `freshness`, `fetched_at`, `last_error`, and
`skipped_records` in production.

## Errors

`CalendarError` distinguishes transport failures, HTTP status, rate limiting,
oversize responses, HTML responses, malformed JSON, missing valid events, and
invalid client configuration. `is_transient()` indicates whether retrying later
may help. `CalendarService` logs fetch failures and retains the previous feed;
its `Freshness` value distinguishes loading, unavailable, fresh, and stale data.

## Tests

```sh
cargo test -p wyck-calendar
cargo clippy -p wyck-calendar --all-targets -- -D warnings
```

Unit tests cover parsing, filtering, time windows, and refresh scheduling.
`tests/feed_http.rs` uses a local HTTP server for conditional requests, error
responses, response-size limits, and stale-cache behavior. These tests do not
contact Forex Factory. The fixture captures one historical weekly response;
it does not validate the current live feed or its availability.

## Production assessment

The crate is suitable for informational calendar views and non-blocking
warnings when the caller treats stale or unavailable data explicitly. It is
not a dependable source for mandatory trade restrictions or release-driven
execution. Before deploying, confirm the feed's usage terms for the intended
product, monitor its behavior, and decide how old a cached feed may be before
the UI stops treating it as current. A dependency vulnerability scan and a
live feed check should be part of the release process.

## Sources

- [Forex Factory calendar and weekly exports](https://www.forexfactory.com/calendar)
- [Weekly JSON export](https://nfs.faireconomy.media/ff_calendar_thisweek.json)
