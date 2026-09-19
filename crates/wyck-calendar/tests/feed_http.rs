//! HTTP-level tests against an in-process mock of the feed (real sockets on
//! `127.0.0.1:0`, no live network) — the same recipe as `ctrader-mcp`'s integration
//! tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::net::TcpListener;
use wyck_calendar::{
    CalendarClient, CalendarError, CalendarService, ClientConfig, FetchOutcome, Freshness,
    ServiceConfig,
};

const FIXTURE: &str = include_str!("fixtures/ff_calendar_thisweek.json");
const ETAG: &str = "\"week-1\"";

async fn serve(router: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    format!("http://{addr}")
}

fn client(base: &str, path: &str) -> CalendarClient {
    CalendarClient::new(ClientConfig {
        url: format!("{base}{path}"),
        timeout: Duration::from_secs(5),
        max_body_bytes: 64 * 1024,
        ..ClientConfig::default()
    })
    .expect("valid config")
}

/// Serves the fixture with an `ETag`, and honors `If-None-Match` with a `304`.
async fn feed(headers: HeaderMap) -> impl IntoResponse {
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(ETAG)
    {
        return (StatusCode::NOT_MODIFIED, HeaderMap::new(), String::new());
    }
    let mut out = HeaderMap::new();
    out.insert(header::ETAG, HeaderValue::from_static(ETAG));
    out.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    (StatusCode::OK, out, FIXTURE.to_owned())
}

fn router() -> Router {
    Router::new()
        .route("/ok", get(feed))
        .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
        .route("/missing", get(|| async { StatusCode::NOT_FOUND }))
        .route(
            "/limited",
            get(|| async {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(header::RETRY_AFTER, "120")],
                )
            }),
        )
        .route(
            "/limited-no-hint",
            get(|| async { StatusCode::TOO_MANY_REQUESTS }),
        )
        .route("/html", get(|| async { "<html>maintenance</html>" }))
        .route("/drift", get(|| async { r#"[{"foo":1},{"bar":2}]"# }))
        .route("/huge", get(|| async { "[".repeat(200 * 1024) }))
        .route("/empty", get(|| async { "[]" }))
}

#[tokio::test]
async fn fetches_and_decodes_the_feed() {
    let base = serve(router()).await;
    let outcome = client(&base, "/ok").fetch(None).await.unwrap();

    let FetchOutcome::Modified { feed, validators } = outcome else {
        panic!("expected a fresh download");
    };
    assert_eq!(feed.skipped, 0);
    assert!(feed.events.len() > 50);
    assert_eq!(validators.unwrap().etag.as_deref(), Some(ETAG));
}

#[tokio::test]
async fn replays_validators_and_gets_not_modified() {
    let base = serve(router()).await;
    let c = client(&base, "/ok");

    let FetchOutcome::Modified { validators, .. } = c.fetch(None).await.unwrap() else {
        panic!("first fetch must download");
    };
    let second = c.fetch(validators.as_ref()).await.unwrap();
    assert_eq!(second, FetchOutcome::NotModified);
}

#[tokio::test]
async fn an_empty_week_is_a_valid_empty_calendar() {
    let base = serve(router()).await;
    let FetchOutcome::Modified { feed, .. } = client(&base, "/empty").fetch(None).await.unwrap()
    else {
        panic!("expected download");
    };
    assert!(feed.events.is_empty());
}

#[tokio::test]
async fn http_failures_map_to_typed_errors() {
    let base = serve(router()).await;

    let err = client(&base, "/boom").fetch(None).await.unwrap_err();
    assert!(matches!(err, CalendarError::Status { status: 500 }));
    assert!(err.is_transient());

    let err = client(&base, "/missing").fetch(None).await.unwrap_err();
    assert!(matches!(err, CalendarError::Status { status: 404 }));
    assert!(!err.is_transient());

    let err = client(&base, "/limited").fetch(None).await.unwrap_err();
    assert!(matches!(
        err,
        CalendarError::RateLimited { retry_after: Some(d) } if d == Duration::from_secs(120)
    ));
    assert!(err.is_transient());

    let err = client(&base, "/limited-no-hint")
        .fetch(None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        CalendarError::RateLimited { retry_after: None }
    ));
}

#[tokio::test]
async fn bad_bodies_are_typed_errors_not_panics() {
    let base = serve(router()).await;

    let err = client(&base, "/html").fetch(None).await.unwrap_err();
    assert!(matches!(err, CalendarError::HtmlResponse), "{err}");
    assert!(err.is_transient());

    let err = client(&base, "/drift").fetch(None).await.unwrap_err();
    assert!(
        matches!(err, CalendarError::NoValidEvents { skipped: 2 }),
        "{err}"
    );

    let err = client(&base, "/huge").fetch(None).await.unwrap_err();
    assert!(
        matches!(err, CalendarError::TooLarge { limit: 65536 }),
        "{err}"
    );
}

#[tokio::test]
async fn an_unreachable_host_is_a_transport_error() {
    // Bind then drop to obtain a port that is (almost certainly) closed.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let err = client(&format!("http://{addr}"), "/ok")
        .fetch(None)
        .await
        .unwrap_err();
    assert!(matches!(err, CalendarError::Transport(_)), "{err}");
    assert!(err.is_transient());
}

#[tokio::test]
async fn the_service_serves_stale_data_when_the_feed_later_breaks() {
    // First request succeeds; every later one is a 500.
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    let router = Router::new().route(
        "/flaky",
        get(move || {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            async move {
                if n == 0 {
                    (StatusCode::OK, FIXTURE.to_owned()).into_response()
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }),
    );
    let base = serve(router).await;

    let handle = CalendarService::spawn(
        client(&base, "/flaky"),
        ServiceConfig {
            refresh_interval: Duration::from_millis(50),
            min_refresh_interval: Duration::from_millis(10),
            retry_initial: Duration::from_millis(20),
            retry_max: Duration::from_millis(40),
        },
    );
    let mut updates = handle.subscribe();

    let good = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            updates.changed().await.unwrap();
            let state = updates.borrow_and_update().clone();
            if state.freshness() == Freshness::Fresh {
                return state;
            }
        }
    })
    .await
    .expect("first fetch publishes");
    let good_len = good.events.len();
    assert!(good_len > 50);

    let stale = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            updates.changed().await.unwrap();
            let state = updates.borrow_and_update().clone();
            if state.freshness() == Freshness::Stale {
                return state;
            }
        }
    })
    .await
    .expect("failure is published");

    assert_eq!(stale.events.len(), good_len, "events survive a broken feed");
    assert!(stale.last_error.unwrap().contains("500"));
    assert!(stale.fetched_at.is_some());
}
