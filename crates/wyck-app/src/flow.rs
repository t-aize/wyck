//! The connection flow: which screen the user is on while connecting, and what moves them on.
//!
//! This is the logic behind the seven connection screens (choose, looking for a local session,
//! session found, session not found, token form, verifying, token refused), with no UI toolkit in
//! it. A front end draws [`ConnectFlow::screen`] and calls the transition methods; the network
//! work is done by [`AppController`](crate::controller::AppController) and its result handed
//! back through [`ConnectFlow::finish_local`] or [`ConnectFlow::finish_remote`].
//!
//! # Stale results
//!
//! Connecting takes seconds and the user can press Back meanwhile. Every attempt gets a number,
//! and a result is only applied if its attempt is still the current one. A result that arrives
//! late is [`Delivery::Stale`]: for a success, the caller must then drop the session it opened
//! (see [`ConnectFlow::back`] for the same rule when leaving a found session).
//!
//! # Tokens
//!
//! [`validate_token`] cleans what the user typed or pasted and refuses what cannot be a token.
//! A token is a [`SecretString`] from that point on and never appears in a screen, a log or a
//! [`Failure`]: [`mask_token`] gives the only form that may be shown.

use secrecy::{ExposeSecret, SecretString};
use wyck_engine::{BrokerErrorKind, EngineError, EngineState};

use crate::presentation::{Badge, kind_badge};

/// Longest failure text kept for display, in characters.
const MAX_DETAIL_CHARS: usize = 240;

/// Words in a connection error that mean the server refused the credentials.
const REFUSAL_MARKERS: [&str; 4] = ["401", "403", "forbidden", "auth"];

/// Why a connection attempt failed, in the terms the screens need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The server answered and refused the credentials: a wrong, expired or revoked token.
    Refused,
    /// Nothing answered: no network, wrong address, cTrader Desktop not running.
    Unreachable,
    /// Anything else.
    Other,
}

/// A failed connection attempt, ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// What kind of failure it is.
    pub kind: FailureKind,
    /// What to tell the user, in one line. Never contains the token.
    pub detail: String,
    /// The technical text of the error, in one bounded line, for logs. Never contains the token.
    pub raw: String,
}

impl Failure {
    /// Describes `error`. `secret`, when given, is removed from the text if it appears in it.
    #[must_use]
    pub fn from_error(error: &EngineError, secret: Option<&str>) -> Self {
        let kind = match error {
            EngineError::Broker { kind, message, .. } => {
                let lower = message.to_ascii_lowercase();
                if REFUSAL_MARKERS.iter().any(|m| lower.contains(m))
                    || *kind == BrokerErrorKind::Rejected
                {
                    FailureKind::Refused
                } else if *kind == BrokerErrorKind::Connection {
                    FailureKind::Unreachable
                } else {
                    FailureKind::Other
                }
            }
            EngineError::Timeout { .. } => FailureKind::Unreachable,
            _ => FailureKind::Other,
        };
        let raw = one_line(&error.to_string(), secret);
        let detail = match kind {
            FailureKind::Refused => {
                "The server refused this token: authentication failed.".to_owned()
            }
            FailureKind::Unreachable => {
                "The server did not answer. Check your internet connection.".to_owned()
            }
            FailureKind::Other => raw.clone(),
        };
        Self { kind, detail, raw }
    }

    /// A failure with a message written by us.
    #[must_use]
    pub fn other(detail: impl Into<String>) -> Self {
        let detail = one_line(&detail.into(), None);
        Self {
            kind: FailureKind::Other,
            raw: detail.clone(),
            detail,
        }
    }
}

/// Collapses whitespace, removes `secret`, and cuts the text to [`MAX_DETAIL_CHARS`].
fn one_line(text: &str, secret: Option<&str>) -> String {
    let mut text = text.to_owned();
    if let Some(secret) = secret.filter(|s| !s.is_empty()) {
        text = text.replace(secret, "[token]");
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_DETAIL_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(MAX_DETAIL_CHARS).collect();
    cut.push_str("...");
    cut
}

/// What a token field can be wrong about before anything is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// Nothing was entered.
    #[error("Paste your API token first.")]
    Empty,
    /// The text has spaces, line breaks or control characters inside: it is not one token.
    #[error(
        "A token is a single unbroken string. Check that you copied all of it and nothing else."
    )]
    Malformed,
}

/// Turns what the user entered into a token.
///
/// Surrounding whitespace and a leading `Bearer ` (people copy the whole header) are removed.
///
/// # Errors
///
/// [`TokenError::Empty`] for nothing, [`TokenError::Malformed`] for whitespace or control
/// characters inside the text.
pub fn validate_token(raw: &str) -> Result<SecretString, TokenError> {
    let trimmed = raw.trim();
    let token = match trimmed.split_once(char::is_whitespace) {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("bearer") => rest.trim(),
        _ => trimmed,
    };
    if token.is_empty() || token.eq_ignore_ascii_case("bearer") {
        return Err(TokenError::Empty);
    }
    if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(TokenError::Malformed);
    }
    Ok(SecretString::from(token))
}

/// The only form of a token that may be shown: dots and, for a long token, its last four
/// characters so the user can tell two tokens apart.
#[must_use]
pub fn mask_token(token: &SecretString) -> String {
    const DOTS: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";
    let token = token.expose_secret();
    let count = token.chars().count();
    if count < 16 {
        return DOTS.to_owned();
    }
    let tail: String = token.chars().skip(count - 4).collect();
    format!("{DOTS}{tail}")
}

/// The `host:port` part of an endpoint URL: `http://127.0.0.1:9876/mcp/` gives `127.0.0.1:9876`.
#[must_use]
pub fn endpoint_authority(endpoint: &str) -> &str {
    let rest = endpoint.split_once("//").map_or(endpoint, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

/// A local cTrader Desktop session that answered, as the "session found" screen shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalSession {
    /// The endpoint that answered.
    pub endpoint: String,
    /// cTrader's version, when it reports one.
    pub server_version: Option<String>,
    /// The account the session is on.
    pub account_id: String,
    /// Demo, live or unknown. An unknown kind is never drawn like a demo.
    pub kind: Badge,
}

impl LocalSession {
    /// Reads the session out of the engine state after a successful connection. `None` while
    /// the state has no account.
    #[must_use]
    pub fn from_state(state: &EngineState, endpoint: &str) -> Option<Self> {
        let account = state.account.as_ref()?;
        Some(Self {
            endpoint: endpoint.to_owned(),
            server_version: account.server_version.clone(),
            account_id: account.account_id.as_str().to_owned(),
            kind: kind_badge(Some(account.kind)),
        })
    }
}

/// Which screen the user is on.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    /// The first screen: local session or token.
    Choose,
    /// Looking for cTrader Desktop.
    Searching,
    /// cTrader Desktop answered. The engine is connected; the user still has to continue.
    LocalFound(LocalSession),
    /// cTrader Desktop did not answer.
    LocalNotFound(Failure),
    /// The token form. `refused` holds the last failure when the previous attempt failed.
    Token {
        /// The failure of the previous attempt, if there was one.
        refused: Option<Failure>,
    },
    /// Checking a token.
    Verifying {
        /// The masked token, or a phrase for a saved one. Safe to show.
        hint: String,
    },
    /// Connected: the application proper.
    Connected,
}

/// A numbered connection attempt. See the module docs on stale results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt(u64);

/// Whether a result was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The result belonged to the current attempt and moved the flow on.
    Applied,
    /// The user had moved on. The result was dropped.
    Stale,
}

/// What leaving a screen requires of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Nothing.
    Nothing,
    /// The engine holds a session the user just walked away from: disconnect it.
    DropSession,
}

/// The connection flow. See the module docs.
#[derive(Debug, Clone)]
pub struct ConnectFlow {
    screen: Screen,
    attempt: u64,
    /// The attempt in flight is the automatic one at startup: on success go straight in.
    resuming: bool,
}

impl Default for ConnectFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectFlow {
    /// A flow on the first screen.
    #[must_use]
    pub fn new() -> Self {
        Self {
            screen: Screen::Choose,
            attempt: 0,
            resuming: false,
        }
    }

    /// A flow for a connection opened without the user, at startup (a saved account). It shows
    /// the matching waiting screen; on success it goes straight to [`Screen::Connected`], on
    /// failure it lands on the screen that explains it.
    ///
    /// `local` says whether the saved account is a local session. `hint` is what the verifying
    /// screen shows in place of a token.
    #[must_use]
    pub fn resume(local: bool, hint: impl Into<String>) -> (Self, Attempt) {
        let mut flow = Self::new();
        flow.resuming = true;
        let attempt = flow.next_attempt();
        flow.screen = if local {
            Screen::Searching
        } else {
            Screen::Verifying { hint: hint.into() }
        };
        (flow, attempt)
    }

    /// A flow standing on `screen`, with nothing in flight. For previews and tests.
    #[must_use]
    pub fn showing(screen: Screen) -> Self {
        Self {
            screen,
            attempt: 0,
            resuming: false,
        }
    }

    /// The screen to draw.
    #[must_use]
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// Whether a connection attempt is running.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        matches!(self.screen, Screen::Searching | Screen::Verifying { .. })
    }

    /// Whether the engine holds a session this flow is showing (found, or connected). A late
    /// success is only dropped when this is false and nothing is running.
    #[must_use]
    pub fn holds_session(&self) -> bool {
        matches!(self.screen, Screen::LocalFound(_) | Screen::Connected)
    }

    fn next_attempt(&mut self) -> Attempt {
        self.attempt += 1;
        Attempt(self.attempt)
    }

    /// Starts looking for a local session (from any screen but a running attempt).
    pub fn begin_local(&mut self) -> Attempt {
        self.resuming = false;
        self.screen = Screen::Searching;
        self.next_attempt()
    }

    /// Starts checking a token. `hint` is the masked token, see [`mask_token`].
    pub fn begin_remote(&mut self, hint: String) -> Attempt {
        self.resuming = false;
        self.screen = Screen::Verifying { hint };
        self.next_attempt()
    }

    /// Delivers the result of a local search.
    pub fn finish_local(
        &mut self,
        attempt: Attempt,
        result: Result<LocalSession, Failure>,
    ) -> Delivery {
        if attempt.0 != self.attempt || self.screen != Screen::Searching {
            return Delivery::Stale;
        }
        self.screen = match result {
            Ok(_) if self.resuming => Screen::Connected,
            Ok(session) => Screen::LocalFound(session),
            Err(failure) => Screen::LocalNotFound(failure),
        };
        self.resuming = false;
        Delivery::Applied
    }

    /// Delivers the result of a token check.
    pub fn finish_remote(&mut self, attempt: Attempt, result: Result<(), Failure>) -> Delivery {
        if attempt.0 != self.attempt || !matches!(self.screen, Screen::Verifying { .. }) {
            return Delivery::Stale;
        }
        self.screen = match result {
            Ok(()) => Screen::Connected,
            Err(failure) => Screen::Token {
                refused: Some(failure),
            },
        };
        self.resuming = false;
        Delivery::Applied
    }

    /// Goes to the token form.
    pub fn open_token(&mut self) {
        self.cancel();
        self.screen = Screen::Token { refused: None };
    }

    /// Goes to the first screen from a running search or check, abandoning it.
    fn cancel(&mut self) {
        self.attempt += 1;
        self.resuming = false;
    }

    /// The Back button. From a running check it returns to the token form, from any other
    /// waiting or form screen to the first screen. Does nothing on the first screen and once
    /// connected (use [`switch`](Self::switch)).
    pub fn back(&mut self) -> Exit {
        let (next, exit) = match &self.screen {
            Screen::Choose | Screen::Connected => return Exit::Nothing,
            Screen::Verifying { .. } => (Screen::Token { refused: None }, Exit::Nothing),
            Screen::LocalFound(_) => (Screen::Choose, Exit::DropSession),
            Screen::Searching | Screen::LocalNotFound(_) | Screen::Token { .. } => {
                (Screen::Choose, Exit::Nothing)
            }
        };
        self.cancel();
        self.screen = next;
        exit
    }

    /// "Continue to Wyck" on the found screen.
    pub fn accept_local(&mut self) {
        if matches!(self.screen, Screen::LocalFound(_)) {
            self.screen = Screen::Connected;
        }
    }

    /// Leaves the connected application for the first screen. The caller disconnects.
    pub fn switch(&mut self) -> Exit {
        if self.screen != Screen::Connected {
            return Exit::Nothing;
        }
        self.cancel();
        self.screen = Screen::Choose;
        Exit::DropSession
    }
}

#[cfg(test)]
mod tests {
    use wyck_engine::{Engine, EngineConfig};

    use super::*;

    fn session() -> LocalSession {
        LocalSession {
            endpoint: "http://127.0.0.1:9876/mcp/".to_owned(),
            server_version: Some("5.10.2".to_owned()),
            account_id: "40218".to_owned(),
            kind: Badge {
                text: "DEMO".to_owned(),
                tone: crate::presentation::Tone::Good,
            },
        }
    }

    fn refused() -> Failure {
        Failure {
            kind: FailureKind::Refused,
            detail: "401".to_owned(),
            raw: "401".to_owned(),
        }
    }

    #[test]
    fn a_local_search_that_succeeds_shows_the_session_then_continues() {
        let mut flow = ConnectFlow::new();
        let attempt = flow.begin_local();
        assert_eq!(flow.screen(), &Screen::Searching);
        assert!(flow.is_busy());
        assert_eq!(flow.finish_local(attempt, Ok(session())), Delivery::Applied);
        assert_eq!(flow.screen(), &Screen::LocalFound(session()));
        flow.accept_local();
        assert_eq!(flow.screen(), &Screen::Connected);
    }

    #[test]
    fn a_local_search_that_fails_explains_and_can_be_retried() {
        let mut flow = ConnectFlow::new();
        let attempt = flow.begin_local();
        flow.finish_local(attempt, Err(Failure::other("no answer")));
        assert!(matches!(flow.screen(), Screen::LocalNotFound(f) if f.detail == "no answer"));
        flow.begin_local();
        assert_eq!(flow.screen(), &Screen::Searching);
    }

    #[test]
    fn a_verified_token_connects() {
        let mut flow = ConnectFlow::new();
        flow.open_token();
        let attempt = flow.begin_remote("hint".to_owned());
        assert!(matches!(flow.screen(), Screen::Verifying { hint } if hint == "hint"));
        assert_eq!(flow.finish_remote(attempt, Ok(())), Delivery::Applied);
        assert_eq!(flow.screen(), &Screen::Connected);
    }

    #[test]
    fn a_refused_token_returns_to_the_form_with_the_reason() {
        let mut flow = ConnectFlow::new();
        let attempt = flow.begin_remote(String::new());
        flow.finish_remote(attempt, Err(refused()));
        assert_eq!(
            flow.screen(),
            &Screen::Token {
                refused: Some(refused())
            }
        );
    }

    #[test]
    fn back_during_a_search_drops_its_late_result() {
        let mut flow = ConnectFlow::new();
        let attempt = flow.begin_local();
        assert_eq!(flow.back(), Exit::Nothing);
        assert_eq!(flow.screen(), &Screen::Choose);
        assert_eq!(
            flow.finish_local(attempt, Ok(session())),
            Delivery::Stale,
            "the caller must drop the session it opened"
        );
        assert_eq!(flow.screen(), &Screen::Choose);
    }

    #[test]
    fn back_during_a_check_returns_to_the_form_and_drops_the_result() {
        let mut flow = ConnectFlow::new();
        let attempt = flow.begin_remote("h".to_owned());
        flow.back();
        assert_eq!(flow.screen(), &Screen::Token { refused: None });
        assert_eq!(flow.finish_remote(attempt, Ok(())), Delivery::Stale);
        assert_eq!(flow.screen(), &Screen::Token { refused: None });
    }

    #[test]
    fn a_result_of_an_older_attempt_never_overwrites_a_newer_one() {
        let mut flow = ConnectFlow::new();
        let old = flow.begin_local();
        flow.back();
        let new = flow.begin_local();
        assert_eq!(flow.finish_local(old, Ok(session())), Delivery::Stale);
        assert_eq!(flow.screen(), &Screen::Searching);
        assert_eq!(flow.finish_local(new, Ok(session())), Delivery::Applied);
    }

    #[test]
    fn a_result_of_the_wrong_kind_is_stale() {
        let mut flow = ConnectFlow::new();
        let attempt = flow.begin_local();
        assert_eq!(flow.finish_remote(attempt, Ok(())), Delivery::Stale);
        assert_eq!(flow.screen(), &Screen::Searching);
    }

    #[test]
    fn leaving_a_found_session_asks_to_drop_it() {
        let mut flow = ConnectFlow::new();
        let attempt = flow.begin_local();
        flow.finish_local(attempt, Ok(session()));
        assert_eq!(flow.back(), Exit::DropSession);
        assert_eq!(flow.screen(), &Screen::Choose);
    }

    #[test]
    fn switching_away_from_a_connection_asks_to_drop_it() {
        let mut flow = ConnectFlow::new();
        assert_eq!(flow.switch(), Exit::Nothing, "nothing to switch from");
        let attempt = flow.begin_remote(String::new());
        flow.finish_remote(attempt, Ok(()));
        assert_eq!(flow.back(), Exit::Nothing, "back does not leave the app");
        assert_eq!(flow.screen(), &Screen::Connected);
        assert_eq!(flow.switch(), Exit::DropSession);
        assert_eq!(flow.screen(), &Screen::Choose);
    }

    #[test]
    fn back_walks_out_of_each_form_screen() {
        let mut flow = ConnectFlow::new();
        assert_eq!(flow.back(), Exit::Nothing);
        flow.open_token();
        flow.back();
        assert_eq!(flow.screen(), &Screen::Choose);
        let attempt = flow.begin_local();
        flow.finish_local(attempt, Err(Failure::other("x")));
        flow.back();
        assert_eq!(flow.screen(), &Screen::Choose);
    }

    #[test]
    fn a_startup_connection_goes_straight_in_or_explains_itself() {
        let (mut flow, attempt) = ConnectFlow::resume(true, "");
        assert_eq!(flow.screen(), &Screen::Searching);
        flow.finish_local(attempt, Ok(session()));
        assert_eq!(
            flow.screen(),
            &Screen::Connected,
            "no click needed at startup"
        );

        let (mut flow, attempt) = ConnectFlow::resume(true, "");
        flow.finish_local(attempt, Err(Failure::other("closed")));
        assert!(matches!(flow.screen(), Screen::LocalNotFound(_)));

        let (mut flow, attempt) = ConnectFlow::resume(false, "your saved token");
        assert!(matches!(flow.screen(), Screen::Verifying { hint } if hint == "your saved token"));
        flow.finish_remote(attempt, Err(refused()));
        assert!(matches!(flow.screen(), Screen::Token { refused: Some(_) }));
    }

    #[test]
    fn a_search_started_by_the_user_after_a_startup_one_needs_a_click() {
        let (mut flow, _) = ConnectFlow::resume(true, "");
        flow.back();
        let attempt = flow.begin_local();
        flow.finish_local(attempt, Ok(session()));
        assert!(matches!(flow.screen(), Screen::LocalFound(_)));
    }

    #[test]
    fn a_token_is_cleaned_before_use() {
        let token = validate_token("  abc123def  ").unwrap();
        assert_eq!(token.expose_secret(), "abc123def");
        let token = validate_token("Bearer abc123def").unwrap();
        assert_eq!(token.expose_secret(), "abc123def");
        let token = validate_token("bearer   abc123def\n").unwrap();
        assert_eq!(token.expose_secret(), "abc123def");
    }

    #[test]
    fn nothing_and_broken_text_are_not_tokens() {
        assert_eq!(validate_token("").unwrap_err(), TokenError::Empty);
        assert_eq!(validate_token("   \n").unwrap_err(), TokenError::Empty);
        assert_eq!(validate_token("Bearer ").unwrap_err(), TokenError::Empty);
        assert_eq!(
            validate_token("abc def").unwrap_err(),
            TokenError::Malformed
        );
        assert_eq!(
            validate_token("abc\u{0}def").unwrap_err(),
            TokenError::Malformed
        );
    }

    #[test]
    fn a_masked_token_hides_all_but_the_last_four_of_a_long_one() {
        let long = SecretString::from("ctd_live_8f2a1c9e4b7d0f3a6c5e");
        let masked = mask_token(&long);
        assert!(masked.ends_with("6c5e"));
        assert!(!masked.contains("ctd_live"));
        assert!(!masked.contains("8f2a"));
        let short = SecretString::from("abc123");
        let masked = mask_token(&short);
        assert!(
            !masked.contains("abc"),
            "a short token shows nothing of itself"
        );
    }

    fn broker(kind: BrokerErrorKind, message: &str) -> EngineError {
        EngineError::Broker {
            kind,
            retryable: false,
            message: message.to_owned(),
        }
    }

    #[test]
    fn an_authentication_error_is_a_refusal() {
        for message in [
            "HTTP status client error (401 Unauthorized)",
            "Forbidden",
            "auth required",
        ] {
            let failure = Failure::from_error(&broker(BrokerErrorKind::Connection, message), None);
            assert_eq!(failure.kind, FailureKind::Refused, "{message}");
        }
    }

    #[test]
    fn a_dead_connection_is_unreachable_not_a_refusal() {
        let failure = Failure::from_error(
            &broker(BrokerErrorKind::Connection, "dns error: no such host"),
            None,
        );
        assert_eq!(failure.kind, FailureKind::Unreachable);
        let failure = Failure::from_error(
            &EngineError::Timeout {
                operation: "connect",
            },
            None,
        );
        assert_eq!(failure.kind, FailureKind::Unreachable);
        let failure = Failure::from_error(&EngineError::NotConnected, None);
        assert_eq!(failure.kind, FailureKind::Other);
    }

    #[test]
    fn the_user_sees_a_plain_sentence_and_the_logs_the_technical_text() {
        let failure = Failure::from_error(
            &broker(
                BrokerErrorKind::Connection,
                "TransportError { AuthRequired }",
            ),
            None,
        );
        assert_eq!(failure.kind, FailureKind::Refused);
        assert!(!failure.detail.contains("TransportError"));
        assert!(failure.raw.contains("TransportError"));
    }

    #[test]
    fn a_failure_text_is_one_bounded_line_without_the_token() {
        let error = broker(
            BrokerErrorKind::Connection,
            &format!(
                "line one\n  line two with SECRETTOKEN inside {}",
                "x".repeat(500)
            ),
        );
        let failure = Failure::from_error(&error, Some("SECRETTOKEN"));
        for text in [&failure.detail, &failure.raw] {
            assert!(!text.contains("SECRETTOKEN"));
            assert!(!text.contains('\n'));
        }
        assert!(failure.raw.contains("[token]"));
        assert!(failure.raw.chars().count() <= MAX_DETAIL_CHARS + 3);
        assert!(failure.raw.ends_with("..."));
    }

    #[test]
    fn the_address_is_the_host_and_port_of_the_endpoint() {
        assert_eq!(
            endpoint_authority("http://127.0.0.1:9876/mcp/"),
            "127.0.0.1:9876"
        );
        assert_eq!(
            endpoint_authority("https://mcp.ctrader.com/trading/mcp"),
            "mcp.ctrader.com"
        );
        assert_eq!(endpoint_authority("localhost:1"), "localhost:1");
    }

    #[test]
    fn a_session_needs_an_account() {
        let engine = Engine::start(EngineConfig {
            calendar_enabled: false,
            ..EngineConfig::default()
        })
        .unwrap();
        assert!(LocalSession::from_state(&engine.handle().state(), "e").is_none());
    }
}
