//! What the user is told when something goes wrong, in one place.
//!
//! The engine reports errors as typed values ([`EngineError`], with a coarse
//! [`ErrorKind`](wyck_engine::ErrorKind) and an [`is_retryable`](EngineError::is_retryable)
//! flag). Every screen would otherwise invent its own wording. [`describe_error`] is the only
//! translation, so a given failure reads the same everywhere and the wording is reviewed once.
//!
//! A message never contains a secret: the engine's errors do not carry one, and this module
//! only quotes what the engine says.

use wyck_engine::{BrokerErrorKind, EngineError, OrderOutcome};

use crate::presentation::{Tone, outcome_text, outcome_tone};
use crate::startup::StartupError;

/// How serious a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Something happened, nothing to do.
    Info,
    /// Something worked.
    Success,
    /// Look at this.
    Warning,
    /// Something failed.
    Error,
}

impl From<Tone> for Level {
    fn from(tone: Tone) -> Self {
        match tone {
            Tone::Neutral => Self::Info,
            Tone::Good => Self::Success,
            Tone::Warn => Self::Warning,
            Tone::Bad => Self::Error,
        }
    }
}

/// A message for the user: a headline, the specifics, and what to try.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// How serious it is.
    pub level: Level,
    /// One line.
    pub title: String,
    /// The specifics, when they help.
    pub detail: Option<String>,
    /// What the user can do, when there is something.
    pub hint: Option<&'static str>,
}

impl Notice {
    /// An informational notice.
    #[must_use]
    pub fn info(title: impl Into<String>) -> Self {
        Self {
            level: Level::Info,
            title: title.into(),
            detail: None,
            hint: None,
        }
    }

    /// A warning.
    #[must_use]
    pub fn warning(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level: Level::Warning,
            title: title.into(),
            detail: Some(detail.into()),
            hint: None,
        }
    }
}

fn error(title: &str, detail: String, hint: Option<&'static str>) -> Notice {
    Notice {
        level: Level::Error,
        title: title.to_owned(),
        detail: Some(detail),
        hint,
    }
}

/// Translates an engine error into a notice.
#[must_use]
pub fn describe_error(e: &EngineError) -> Notice {
    let detail = e.to_string();
    match e {
        EngineError::Config(_) => error(
            "Invalid configuration",
            detail,
            Some("Fix the setting named in the message and restart."),
        ),
        EngineError::NotConnected => error(
            "Not connected",
            detail,
            Some("Connect to an account first."),
        ),
        EngineError::NotReady { .. } => error(
            "The connection is not ready",
            detail,
            Some("Wait for the session to be Ready. Trading is off until then."),
        ),
        EngineError::NotArmed => error(
            "Trading is not armed",
            detail,
            Some("Orders are only planned in dry-run mode."),
        ),
        EngineError::ArmRefused(_) => error(
            "Arming refused",
            detail,
            Some("Check the account and its kind."),
        ),
        EngineError::TradingUnavailable(_) => error(
            "This connection cannot place orders",
            detail,
            Some("Use a connection with trading rights."),
        ),
        EngineError::Invalid(_) => error(
            "The order cannot be built",
            detail,
            Some("Check the symbol, the size and the stop."),
        ),
        EngineError::Busy(_) => error(
            "An order is already in progress",
            detail,
            Some("Wait a moment and try again."),
        ),
        EngineError::ConfirmationRejected(_) => error(
            "Confirmation expired or already used",
            detail,
            Some("Start again to get a fresh confirmation."),
        ),
        EngineError::Broker {
            kind, retryable, ..
        } => {
            let (title, hint) = match kind {
                BrokerErrorKind::Connection => (
                    "Cannot reach the server",
                    "Check the network. The session reconnects by itself.",
                ),
                BrokerErrorKind::Rejected => (
                    "The server refused the request",
                    "Read the reason. Retrying the same request will not help.",
                ),
                BrokerErrorKind::Unavailable => (
                    "The server is temporarily unavailable",
                    "Try again shortly.",
                ),
                BrokerErrorKind::Protocol => (
                    "The server answered something unexpected",
                    "Check the platform: the request may or may not have been applied.",
                ),
                _ => (
                    "The server reported an error",
                    "Check the platform if it concerned an order.",
                ),
            };
            let mut notice = error(title, detail, Some(hint));
            if *retryable && notice.level == Level::Error {
                notice.level = Level::Warning;
            }
            notice
        }
        EngineError::Timeout { .. } => error(
            "The server did not answer in time",
            detail,
            Some("If this was an order, check the platform: it may have gone through."),
        ),
        EngineError::ShuttingDown => Notice::info("The application is closing"),
        _ => error("Unexpected error", detail, None),
    }
}

/// Translates a startup problem into a banner. `None` for a first run without an account: the
/// first screen is the explanation.
#[must_use]
pub fn describe_startup(error: &StartupError) -> Option<Notice> {
    match error {
        StartupError::NoProfile => None,
        StartupError::Config(reason) => Some(Notice {
            level: Level::Error,
            title: "The saved account could not be read".to_owned(),
            detail: Some(reason.clone()),
            hint: Some(
                "Connect again below. Check the configuration file and the credential store.",
            ),
        }),
        StartupError::Engine(e) => Some(describe_error(e)),
    }
}

/// Translates an order outcome into a notice.
#[must_use]
pub fn describe_outcome(outcome: &OrderOutcome) -> Notice {
    let tone = outcome_tone(outcome);
    let hint = match outcome {
        OrderOutcome::Unknown { .. } => {
            Some("Look at the positions in the platform before doing anything else.")
        }
        OrderOutcome::DryRun { .. } => Some("The engine is in dry-run mode: no order was sent."),
        _ => None,
    };
    Notice {
        level: tone.into(),
        title: outcome_text(outcome),
        detail: None,
        hint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broker(kind: BrokerErrorKind, retryable: bool) -> EngineError {
        EngineError::Broker {
            kind,
            retryable,
            message: "boom".to_owned(),
        }
    }

    #[test]
    fn a_first_run_is_not_a_problem_but_a_broken_configuration_is() {
        assert!(describe_startup(&StartupError::NoProfile).is_none());
        let notice = describe_startup(&StartupError::Config("disk".into())).unwrap();
        assert_eq!(notice.level, Level::Error);
        assert_eq!(notice.detail.as_deref(), Some("disk"));
    }

    #[test]
    fn every_engine_error_has_a_title_and_a_level() {
        let errors = [
            EngineError::Config("x".into()),
            EngineError::NotConnected,
            EngineError::NotReady {
                state: "Connecting".into(),
            },
            EngineError::NotArmed,
            EngineError::ArmRefused("x".into()),
            EngineError::TradingUnavailable("x".into()),
            EngineError::Invalid("x".into()),
            EngineError::Busy("x".into()),
            EngineError::ConfirmationRejected("x".into()),
            broker(BrokerErrorKind::Connection, true),
            broker(BrokerErrorKind::Rejected, false),
            broker(BrokerErrorKind::Unavailable, true),
            broker(BrokerErrorKind::Protocol, false),
            broker(BrokerErrorKind::Unknown, false),
            EngineError::Timeout {
                operation: "positions",
            },
            EngineError::ShuttingDown,
            EngineError::Internal("x".into()),
        ];
        for e in &errors {
            let n = describe_error(e);
            assert!(!n.title.trim().is_empty(), "{e:?}");
        }
    }

    #[test]
    fn a_retryable_broker_failure_is_a_warning_and_a_refusal_is_an_error() {
        assert_eq!(
            describe_error(&broker(BrokerErrorKind::Connection, true)).level,
            Level::Warning
        );
        assert_eq!(
            describe_error(&broker(BrokerErrorKind::Rejected, false)).level,
            Level::Error
        );
    }

    #[test]
    fn a_timeout_warns_that_an_order_may_have_gone_through() {
        let n = describe_error(&EngineError::Timeout {
            operation: "create_order",
        });
        assert!(n.hint.unwrap().contains("may have gone through"));
    }

    #[test]
    fn the_engine_detail_is_kept_and_nothing_is_invented() {
        let n = describe_error(&EngineError::Invalid("volume below the minimum".into()));
        assert!(n.detail.unwrap().contains("volume below the minimum"));
    }

    #[test]
    fn tones_map_to_levels() {
        assert_eq!(Level::from(Tone::Bad), Level::Error);
        assert_eq!(Level::from(Tone::Good), Level::Success);
        assert_eq!(Level::from(Tone::Warn), Level::Warning);
        assert_eq!(Level::from(Tone::Neutral), Level::Info);
    }
}
