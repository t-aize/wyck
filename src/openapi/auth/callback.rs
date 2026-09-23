//! Catching the redirect of the consent page on `localhost`.
//!
//! When the user grants access, the browser is sent to the application's redirect URI with the
//! authorization code in the query. A desktop application has no web server, so it opens a tiny one
//! for the occasion: a [`CallbackListener`] bound to the loopback address, which lives until the
//! code arrives (or a timeout passes) and then closes.
//!
//! ```text
//! let listener = CallbackListener::bind(8765).await?;
//! let uri = listener.redirect_uri();                       // http://localhost:8765
//! let state = auth::new_state();
//! open_in_browser(&auth::authorization_url(id, &uri, Scope::Accounts, &state));
//! let code = listener.wait(&state, Duration::from_secs(300)).await?;
//! let tokens = oauth.exchange_code(code.code(), &uri).await?;   // within a minute
//! ```
//!
//! The redirect URI must be registered for the application exactly as given, port included, so the
//! port is a setting and not a random choice (port `0` is available for tests). The listener only
//! binds `127.0.0.1`: nothing outside the machine can reach it.
//!
//! # What it accepts
//!
//! It answers the first request that carries a `code` (or an `error`) and ignores others (the
//! browser's request for `/favicon.ico`, a port scan) with a 404, then keeps waiting. A request
//! whose `state` does not match the expected one is rejected the same way: it did not come from the
//! consent page this call opened. If the redirect carries no `state` at all it is accepted and
//! [`AuthorizationCode::state_echoed`] is `false`, because the server's documentation does not say
//! that it echoes the parameter; a caller that wants to be strict can refuse such a code.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

use crate::openapi::error::{OpenApiError, Result};

/// The biggest request head read: a redirect is a few hundred bytes.
const MAX_REQUEST_BYTES: usize = 8 * 1024;

/// How long one connection may take to send its request.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The page shown in the browser once the code was received.
const SUCCESS_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Signed in</title></head>\
<body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\">\
<h2>Signed in</h2><p>You can close this window and go back to the application.</p></body></html>";

/// The page shown when the user refused, or the request was not accepted.
const FAILURE_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Not signed in</title></head>\
<body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\">\
<h2>Not signed in</h2><p>Go back to the application and try again.</p></body></html>";

/// The authorization code the redirect brought. It is a secret for the minute it lives.
#[derive(Debug, Clone)]
pub struct AuthorizationCode {
    code: SecretString,
    state_echoed: bool,
}

impl AuthorizationCode {
    /// The code, to trade at the token endpoint at once.
    #[must_use]
    pub fn code(&self) -> &str {
        self.code.expose_secret()
    }

    /// Whether the redirect carried the `state` back, and it matched. `false` when the redirect had
    /// no `state` at all.
    #[must_use]
    pub fn state_echoed(&self) -> bool {
        self.state_echoed
    }
}

/// A one-shot web server on the loopback address. See the [module docs](self).
#[derive(Debug)]
pub struct CallbackListener {
    listener: TcpListener,
    port: u16,
}

impl CallbackListener {
    /// Starts listening on `127.0.0.1:port`. With port `0` the system picks a free port (see
    /// [`CallbackListener::port`]).
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Transport`] when the port cannot be used, usually because another program
    /// holds it: the text says so, since the user can act on it.
    pub async fn bind(port: u16) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| {
            warn!(port, error = %e, "cannot listen for the sign in redirect");
            OpenApiError::Transport(format!(
                "cannot listen on port {port} for the sign in redirect ({e}); is it in use?"
            ))
        })?;
        let port = listener
            .local_addr()
            .map_err(|e| OpenApiError::Transport(e.to_string()))?
            .port();
        debug!(port, "listening for the OAuth sign in redirect");
        Ok(Self { listener, port })
    }

    /// The port being listened on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The redirect URI to register and to pass to the consent page. It uses the name `localhost`,
    /// the form the portal documents for desktop applications.
    #[must_use]
    pub fn redirect_uri(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// Waits for the redirect, up to `timeout`, and returns the code.
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Timeout`] when nothing valid arrives in time, and [`OpenApiError::Auth`]
    /// when the user refused (the redirect carried an `error`).
    pub async fn wait(self, expected_state: &str, timeout: Duration) -> Result<AuthorizationCode> {
        tokio::time::timeout(timeout, self.accept_until_code(expected_state))
            .await
            .map_err(|_| OpenApiError::Timeout {
                operation: "the sign in in the browser",
            })?
    }

    async fn accept_until_code(&self, expected_state: &str) -> Result<AuthorizationCode> {
        loop {
            let Ok((mut stream, _)) = self.listener.accept().await else {
                continue;
            };
            let head = match tokio::time::timeout(READ_TIMEOUT, read_head(&mut stream)).await {
                Ok(Some(head)) => head,
                _ => {
                    respond(&mut stream, "400 Bad Request", FAILURE_PAGE).await;
                    continue;
                }
            };
            match parse_redirect(&head, expected_state) {
                Redirect::Code(code) => {
                    if !code.state_echoed() {
                        warn!("the redirect carried no state to check; accepted the code anyway");
                    }
                    info!(
                        state_echoed = code.state_echoed(),
                        "the sign in redirect carried a code"
                    );
                    respond(&mut stream, "200 OK", SUCCESS_PAGE).await;
                    return Ok(code);
                }
                Redirect::Denied(reason) => {
                    warn!(%reason, "the sign in redirect denied access");
                    respond(&mut stream, "200 OK", FAILURE_PAGE).await;
                    return Err(OpenApiError::Auth(reason));
                }
                Redirect::Ignore => {
                    debug!(
                        "ignored a request to the callback listener that carried no usable redirect"
                    );
                    respond(&mut stream, "404 Not Found", FAILURE_PAGE).await;
                }
            }
        }
    }
}

/// Reads the request line and headers, or `None` when the connection sends nothing usable.
async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") || buffer.len() >= MAX_REQUEST_BYTES {
            break;
        }
    }
    (!buffer.is_empty()).then(|| String::from_utf8_lossy(&buffer).into_owned())
}

async fn respond(stream: &mut TcpStream, status: &str, page: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    // The browser may have gone; nothing to do about it.
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// What one request to the listener means.
#[derive(Debug)]
enum Redirect {
    /// The consent page's answer, with a code.
    Code(AuthorizationCode),
    /// The user refused, or the server reported an error.
    Denied(String),
    /// Anything else.
    Ignore,
}

/// Reads a request head as a redirect. Pure, so the rules are tested without a socket.
fn parse_redirect(head: &str, expected_state: &str) -> Redirect {
    let Some(line) = head.lines().next() else {
        return Redirect::Ignore;
    };
    let mut parts = line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return Redirect::Ignore;
    };
    let Ok(url) = reqwest::Url::parse(&format!("http://localhost{target}")) else {
        return Redirect::Ignore;
    };
    let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();

    // A state that is present must match; one that is absent is allowed (see the module docs).
    let state_echoed = match query.get("state") {
        Some(state) if state == expected_state => true,
        Some(_) => return Redirect::Ignore,
        None => false,
    };
    if let Some(error) = query.get("error") {
        let detail = query
            .get("error_description")
            .map_or(String::new(), |d| format!(": {d}"));
        return Redirect::Denied(format!("access was not granted ({error}{detail})"));
    }
    match query.get("code").filter(|c| !c.is_empty()) {
        Some(code) => Redirect::Code(AuthorizationCode {
            code: SecretString::from(code.clone()),
            state_echoed,
        }),
        None => Redirect::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.unwrap();
        answer
    }

    #[test]
    fn a_redirect_with_a_matching_state_gives_the_code() {
        match parse_redirect("GET /?code=abc123&state=s1 HTTP/1.1\r\n\r\n", "s1") {
            Redirect::Code(c) => {
                assert_eq!(c.code(), "abc123");
                assert!(c.state_echoed());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_redirect_without_a_state_is_accepted_and_flagged() {
        match parse_redirect("GET /?code=abc HTTP/1.1\r\n\r\n", "s1") {
            Redirect::Code(c) => assert!(!c.state_echoed()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_wrong_state_is_not_accepted() {
        assert!(matches!(
            parse_redirect("GET /?code=abc&state=other HTTP/1.1\r\n\r\n", "s1"),
            Redirect::Ignore
        ));
    }

    #[test]
    fn a_refusal_carries_its_reason() {
        match parse_redirect(
            "GET /?error=access_denied&error_description=No&state=s1 HTTP/1.1\r\n\r\n",
            "s1",
        ) {
            Redirect::Denied(text) => {
                assert!(text.contains("access_denied") && text.contains("No"))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn everything_else_is_ignored() {
        for head in [
            "GET /favicon.ico HTTP/1.1\r\n\r\n",
            "POST /?code=abc HTTP/1.1\r\n\r\n",
            "GET /?code= HTTP/1.1\r\n\r\n",
            "",
            "garbage",
        ] {
            assert!(
                matches!(parse_redirect(head, "s1"), Redirect::Ignore),
                "{head:?}"
            );
        }
    }

    #[test]
    fn a_code_with_url_encoding_is_decoded() {
        match parse_redirect("GET /?code=a%2Fb%3D&state=s HTTP/1.1\r\n\r\n", "s") {
            Redirect::Code(c) => assert_eq!(c.code(), "a/b="),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn the_listener_answers_the_redirect_and_returns_the_code() {
        let listener = CallbackListener::bind(0).await.unwrap();
        let port = listener.port();
        assert_eq!(listener.redirect_uri(), format!("http://localhost:{port}"));
        let waiting = tokio::spawn(listener.wait("st", Duration::from_secs(5)));

        // Noise first: the favicon request must not end the wait.
        let noise = get(port, "/favicon.ico").await;
        assert!(noise.starts_with("HTTP/1.1 404"), "{noise}");
        let answer = get(port, "/?code=THECODE&state=st").await;
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.contains("Signed in"));

        let code = waiting.await.unwrap().unwrap();
        assert_eq!(code.code(), "THECODE");
    }

    #[tokio::test]
    async fn a_forged_state_does_not_end_the_wait() {
        let listener = CallbackListener::bind(0).await.unwrap();
        let port = listener.port();
        let waiting = tokio::spawn(listener.wait("real", Duration::from_secs(5)));
        let forged = get(port, "/?code=EVIL&state=fake").await;
        assert!(forged.starts_with("HTTP/1.1 404"));
        get(port, "/?code=GOOD&state=real").await;
        assert_eq!(waiting.await.unwrap().unwrap().code(), "GOOD");
    }

    #[tokio::test]
    async fn a_refusal_in_the_browser_is_an_error() {
        let listener = CallbackListener::bind(0).await.unwrap();
        let port = listener.port();
        let waiting = tokio::spawn(listener.wait("st", Duration::from_secs(5)));
        get(port, "/?error=access_denied&state=st").await;
        let error = waiting.await.unwrap().unwrap_err();
        assert!(matches!(error, OpenApiError::Auth(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_arriving_is_a_timeout() {
        let listener = CallbackListener::bind(0).await.unwrap();
        let error = listener
            .wait("st", Duration::from_secs(60))
            .await
            .unwrap_err();
        assert!(matches!(error, OpenApiError::Timeout { .. }));
    }

    #[tokio::test]
    async fn a_busy_port_is_reported_with_the_port_number() {
        let first = CallbackListener::bind(0).await.unwrap();
        let error = CallbackListener::bind(first.port()).await.unwrap_err();
        assert!(error.to_string().contains(&first.port().to_string()));
    }
}
