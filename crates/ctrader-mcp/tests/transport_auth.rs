//! Regression coverage for the double-Bearer-prefix auth bug: `ConnectionConfig::
//! with_bearer_token` used to store `"Bearer <token>"`, which `rmcp`'s streamable-HTTP
//! transport then prefixed with `Bearer ` AGAIN (via `reqwest::RequestBuilder::
//! bearer_auth`), sending `Authorization: Bearer Bearer <token>`, which cTrader's
//! remote MCP endpoint rejected with `AuthRequired(invalid_token)`.
//!
//! These tests talk to a raw `axum` router with no MCP semantics at all (mirroring
//! `rmcp`'s own `tests/test_streamable_http_get_stream_auth_challenge.rs`), since the
//! bug is entirely at the HTTP header level, below where MCP framing applies.

mod support;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Response, StatusCode};
use axum::routing::post;
use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::error::CTraderError;
use ctrader_mcp::retry::RetryPolicy;
use ctrader_mcp::transport::McpSession;

#[tokio::test]
async fn bearer_token_is_sent_without_a_doubled_bearer_prefix() {
    let captured_auth_header: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured = captured_auth_header.clone();

    let router = Router::new().route(
        "/mcp",
        post(move |headers: HeaderMap| {
            let captured = captured.clone();
            async move {
                *captured.lock().unwrap() = headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                // Not a real MCP server, and that's fine: the assertion below is on
                // what the server RECEIVED, not on whether `connect()` succeeds.
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap()
            }
        }),
    );
    let (url, handle) = support::spawn_raw_http_server(router).await;

    let config = ConnectionConfig::new(url)
        .with_bearer_token("secret-token")
        .with_retry_policy(RetryPolicy::none());
    let _ = McpSession::connect(&config).await;

    assert_eq!(
        captured_auth_header.lock().unwrap().as_deref(),
        Some("Bearer secret-token"),
        "must be exactly \"Bearer secret-token\": a doubled prefix (\"Bearer Bearer \
         secret-token\") is the regression this test guards against"
    );

    handle.abort();
}

#[tokio::test]
async fn a_401_auth_challenge_surfaces_as_a_connect_error() {
    let router = Router::new().route(
        "/mcp",
        post(|| async {
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(
                    "www-authenticate",
                    "Bearer realm=\"mcp\", error=\"invalid_token\"",
                )
                .body(Body::empty())
                .unwrap()
        }),
    );
    let (url, handle) = support::spawn_raw_http_server(router).await;

    let config = ConnectionConfig::new(url)
        .with_bearer_token("whatever-token")
        .with_retry_policy(RetryPolicy::none());
    match McpSession::connect(&config).await {
        Err(CTraderError::Connect { .. }) => {}
        Err(other) => panic!("expected CTraderError::Connect, got {other:?}"),
        Ok(_) => panic!("expected connect() to fail against a 401-only server"),
    }

    handle.abort();
}
