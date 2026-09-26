//! The OAuth client against a local stand-in for the token endpoint.
//!
//! The endpoint takes the client secret and the code in the query string of a `GET`, so the tests
//! also check that no error message repeats them.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use secrecy::ExposeSecret;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wyck_openapi::OpenApiError;
use wyck_openapi::auth::OAuthClient;
use wyck_openapi::config::ClientCredentials;

/// A one-answer HTTP server: it records the request line and replies with `status` and `body`.
async fn token_server(status: &str, body: &str) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/apps/token", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (seen, status, body) = (requests.clone(), status.to_owned(), body.to_owned());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0u8; 8192];
            let read = stream.read(&mut buffer).await.unwrap_or(0);
            let head = String::from_utf8_lossy(&buffer[..read]).into_owned();
            seen.lock()
                .unwrap()
                .push(head.lines().next().unwrap_or("").to_owned());
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    (url, requests)
}

fn client(url: &str) -> OAuthClient {
    OAuthClient::new(ClientCredentials::new("app-id", "app-secret-XYZ"))
        .unwrap()
        .with_token_url(url)
}

#[tokio::test]
async fn a_code_is_exchanged_with_every_parameter_the_endpoint_wants() {
    let (url, requests) = token_server(
        "200 OK",
        r#"{"accessToken":"AT-1","refreshToken":"RT-1","tokenType":"bearer","expiresIn":2628000}"#,
    )
    .await;
    let tokens = client(&url)
        .exchange_code("the code", "http://localhost:8765")
        .await
        .unwrap();
    assert_eq!(tokens.access_token.expose_secret(), "AT-1");
    assert_eq!(tokens.refresh_token.expose_secret(), "RT-1");
    assert_eq!(tokens.expires_in, Some(Duration::from_secs(2_628_000)));

    let request = requests.lock().unwrap()[0].clone();
    assert!(request.starts_with("GET /apps/token?"), "{request}");
    for expected in [
        "grant_type=authorization_code",
        "code=the+code",
        "redirect_uri=http%3A%2F%2Flocalhost%3A8765",
        "client_id=app-id",
        "client_secret=app-secret-XYZ",
    ] {
        assert!(
            request.contains(expected),
            "{expected} missing from {request}"
        );
    }
}

#[tokio::test]
async fn a_refresh_sends_the_refresh_token_and_returns_the_new_pair() {
    let (url, requests) = token_server(
        "200 OK",
        r#"{"accessToken":"AT-2","refreshToken":"RT-2","expiresIn":"1000"}"#,
    )
    .await;
    let tokens = client(&url).refresh("RT-1").await.unwrap();
    assert_eq!(tokens.access_token.expose_secret(), "AT-2");
    assert_eq!(
        tokens.expires_in,
        Some(Duration::from_secs(1000)),
        "text numbers are read"
    );
    let request = requests.lock().unwrap()[0].clone();
    assert!(request.contains("grant_type=refresh_token") && request.contains("refresh_token=RT-1"));
}

#[tokio::test]
async fn an_error_in_a_normal_answer_is_reported_with_the_servers_words() {
    let (url, _) = token_server(
        "200 OK",
        r#"{"errorCode":"ACCESS_DENIED","description":"Invalid authorization code"}"#,
    )
    .await;
    let error = client(&url)
        .exchange_code("bad", "http://localhost:1")
        .await
        .unwrap_err();
    assert!(matches!(error, OpenApiError::Auth(_)));
    assert!(error.to_string().contains("ACCESS_DENIED"));
    assert!(error.to_string().contains("Invalid authorization code"));
}

#[tokio::test]
async fn an_error_status_with_an_error_body_keeps_both() {
    let (url, _) = token_server(
        "400 Bad Request",
        r#"{"errorCode":"INVALID_GRANT","description":"code expired"}"#,
    )
    .await;
    let error = client(&url)
        .exchange_code("old", "http://localhost:1")
        .await
        .unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains("INVALID_GRANT") && text.contains("400"),
        "{text}"
    );
}

#[tokio::test]
async fn an_unreadable_body_is_an_error_not_a_panic() {
    let (url, _) = token_server("502 Bad Gateway", "<html>oops</html>").await;
    let error = client(&url).refresh("x").await.unwrap_err();
    assert!(matches!(error, OpenApiError::Auth(_)));
    assert!(error.to_string().contains("502"), "{error}");
}

#[tokio::test]
async fn an_unreachable_endpoint_never_leaks_the_secret_or_the_code_in_the_error() {
    // Nothing listens here: the failure comes from the HTTP layer, whose messages normally hold
    // the whole URL, query string included.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/apps/token", listener.local_addr().unwrap());
    drop(listener);
    let error = client(&url)
        .exchange_code("SUPER-SECRET-CODE", "http://localhost:8765")
        .await
        .unwrap_err();
    let text = format!("{error} {error:?}");
    assert!(
        !text.contains("app-secret-XYZ"),
        "the secret leaked: {text}"
    );
    assert!(
        !text.contains("SUPER-SECRET-CODE"),
        "the code leaked: {text}"
    );
    // A network failure is a transport error (worth retrying), not a refusal of the sign in.
    assert!(matches!(error, OpenApiError::Transport(_)), "{error:?}");
}

#[tokio::test]
async fn a_token_endpoint_that_is_not_a_url_is_a_config_error() {
    let oauth = OAuthClient::new(ClientCredentials::new("a", "b"))
        .unwrap()
        .with_token_url("not a url");
    assert!(matches!(
        oauth.refresh("x").await,
        Err(OpenApiError::Config(_))
    ));
}
