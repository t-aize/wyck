//! The HTTP side: one conditional, size-bounded GET of the feed.

use std::future::Future;
use std::time::Duration;

use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, RETRY_AFTER};
use reqwest::{StatusCode, Url};

use crate::error::{CalendarError, Result};
use crate::parse::{Feed, parse_feed};

/// ForexFactory's public feed for the current week (no authentication).
pub const THIS_WEEK_URL: &str = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";

/// How the client reaches the feed.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// The feed URL. `http` and `https` are accepted (`http` exists for loopback test
    /// servers; the real feed is `https`). Default: [`THIS_WEEK_URL`].
    pub url: String,
    /// Total time allowed for one request, connect through last body byte. Default 15 s.
    pub timeout: Duration,
    /// Response-body cap. Default 2 MiB, over 100 times a real week.
    pub max_body_bytes: usize,
    /// `User-Agent` header. Default `wyck-calendar/<crate version>`.
    pub user_agent: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            url: THIS_WEEK_URL.to_owned(),
            timeout: Duration::from_secs(15),
            max_body_bytes: 2 * 1024 * 1024,
            user_agent: concat!("wyck-calendar/", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }
}

/// The HTTP cache validators from a successful response, replayed on the next request so
/// an unchanged feed costs a `304` instead of a full body (and is friendlier to the
/// feed's rate limiting).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validators {
    /// `ETag` response header.
    pub etag: Option<String>,
    /// `Last-Modified` response header.
    pub last_modified: Option<String>,
}

impl Validators {
    fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none()
    }
}

/// The result of one fetch.
#[derive(Debug, Clone, PartialEq)]
pub enum FetchOutcome {
    /// A fresh body was downloaded and decoded.
    Modified {
        /// The decoded feed.
        feed: Feed,
        /// Validators to send next time, when the server provided any.
        validators: Option<Validators>,
    },
    /// The server confirmed (`304`) that the previously fetched feed is current.
    NotModified,
}

/// Anything that can produce the calendar feed. [`CalendarClient`] is the real
/// implementation; the trait exists so [`crate::CalendarService`]'s scheduling, backoff
/// and stale-data behavior can be tested against a scripted source.
pub trait Fetch: Send + Sync + 'static {
    /// Fetches the feed, sending `validators` (from a previous `Modified` outcome) as a
    /// conditional request when present.
    fn fetch(
        &self,
        validators: Option<&Validators>,
    ) -> impl Future<Output = Result<FetchOutcome>> + Send;
}

/// The real [`Fetch`]: `reqwest` against the configured URL.
#[derive(Debug, Clone)]
pub struct CalendarClient {
    http: reqwest::Client,
    url: Url,
    max_body_bytes: usize,
}

impl CalendarClient {
    /// Builds a client.
    ///
    /// # Errors
    ///
    /// [`CalendarError::Config`] if the URL is invalid or not `http(s)`, or the HTTP
    /// stack cannot be initialized.
    pub fn new(config: ClientConfig) -> Result<Self> {
        let url = Url::parse(&config.url)
            .map_err(|e| CalendarError::Config(format!("bad feed URL `{}`: {e}", config.url)))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(CalendarError::Config(format!(
                "feed URL must be http(s), got `{}`",
                url.scheme()
            )));
        }
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| CalendarError::Config(format!("cannot build HTTP client: {e}")))?;
        Ok(Self {
            http,
            url,
            max_body_bytes: config.max_body_bytes,
        })
    }

    /// A client for the real feed with default settings.
    ///
    /// # Errors
    ///
    /// See [`CalendarClient::new`].
    pub fn with_defaults() -> Result<Self> {
        Self::new(ClientConfig::default())
    }

    /// One conditional GET; see [`Fetch::fetch`].
    ///
    /// # Errors
    ///
    /// Every [`CalendarError`] except `Config`.
    pub async fn fetch(&self, validators: Option<&Validators>) -> Result<FetchOutcome> {
        let mut request = self.http.get(self.url.clone());
        if let Some(v) = validators {
            if let Some(etag) = &v.etag {
                request = request.header(IF_NONE_MATCH, etag);
            }
            if let Some(modified) = &v.last_modified {
                request = request.header(IF_MODIFIED_SINCE, modified);
            }
        }

        let response = request.send().await.map_err(CalendarError::Transport)?;
        let status = response.status();

        if status == StatusCode::NOT_MODIFIED {
            if validators.is_none_or(Validators::is_empty) {
                return Err(CalendarError::Status { status: 304 });
            }
            return Ok(FetchOutcome::NotModified);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(CalendarError::RateLimited { retry_after });
        }
        if !status.is_success() {
            return Err(CalendarError::Status {
                status: status.as_u16(),
            });
        }

        let validators = Validators {
            etag: header_string(&response, ETAG),
            last_modified: header_string(&response, LAST_MODIFIED),
        };
        let body = self.read_bounded(response).await?;
        if body.trim_ascii_start().first() == Some(&b'<') {
            return Err(CalendarError::HtmlResponse);
        }
        let feed = parse_feed(&body)?;
        Ok(FetchOutcome::Modified {
            feed,
            validators: (!validators.is_empty()).then_some(validators),
        })
    }

    /// Reads the body, aborting as soon as it exceeds the cap (a hostile or wrong URL
    /// cannot make us buffer an unbounded stream).
    async fn read_bounded(&self, mut response: reqwest::Response) -> Result<Vec<u8>> {
        let limit = self.max_body_bytes;
        let too_large = || CalendarError::TooLarge { limit };

        if response
            .content_length()
            .is_some_and(|len| len > limit as u64)
        {
            return Err(too_large());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(CalendarError::Transport)? {
            if body.len() + chunk.len() > limit {
                return Err(too_large());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

impl Fetch for CalendarClient {
    fn fetch(
        &self,
        validators: Option<&Validators>,
    ) -> impl Future<Output = Result<FetchOutcome>> + Send {
        CalendarClient::fetch(self, validators)
    }
}

fn header_string(
    response: &reqwest::Response,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_urls_before_any_request() {
        for url in [
            "",
            "not a url",
            "ftp://example.invalid/x",
            "file:///etc/passwd",
        ] {
            let err = CalendarClient::new(ClientConfig {
                url: url.to_owned(),
                ..ClientConfig::default()
            })
            .unwrap_err();
            assert!(matches!(err, CalendarError::Config(_)), "{url:?}");
        }
    }

    #[test]
    fn default_config_targets_the_public_feed() {
        let config = ClientConfig::default();
        assert_eq!(config.url, THIS_WEEK_URL);
        assert!(config.user_agent.starts_with("wyck-calendar/"));
        CalendarClient::new(config).expect("default config is valid");
    }
}
