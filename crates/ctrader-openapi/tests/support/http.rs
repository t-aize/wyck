//! A stand-in for the OAuth token endpoint: plain HTTP on localhost, answers taken in order.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// The requests a stand-in endpoint has seen (their request lines) and where to reach it.
pub struct TokenServer {
    /// The address to give to `OAuthClient::with_token_url`.
    pub url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl TokenServer {
    /// The request lines received so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// Starts an endpoint that answers request number `i` with `answers[i]` (status line text and
/// body), and repeats the last answer for any request after the list.
pub async fn token_server_sequence(answers: Vec<(&str, String)>) -> TokenServer {
    assert!(!answers.is_empty());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/apps/token", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let answers: Vec<(String, String)> = answers
        .into_iter()
        .map(|(status, body)| (status.to_owned(), body))
        .collect();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0u8; 8192];
            let read = stream.read(&mut buffer).await.unwrap_or(0);
            let head = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let index = {
                let mut seen = seen.lock().unwrap();
                seen.push(head.lines().next().unwrap_or("").to_owned());
                seen.len() - 1
            };
            let (status, body) = &answers[index.min(answers.len() - 1)];
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    TokenServer { url, requests }
}

/// The body of a good token answer.
pub fn tokens_body(access: &str, refresh: &str, expires_in: i64) -> String {
    format!(
        r#"{{"accessToken":"{access}","refreshToken":"{refresh}","tokenType":"bearer","expiresIn":{expires_in}}}"#
    )
}
