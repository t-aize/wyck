//! Opening the application on a given screen, to look at it without going through the flow.
//!
//! In a debug build, `WYCK_PREVIEW=<name>` makes the window start on that screen with sample
//! data and skips the automatic connection, so every state of the connection flow can be seen
//! without a cTrader Desktop to find or a token to refuse. A release build ignores the
//! variable. The names are in [`NAMES`].

use crate::flow::{Failure, FailureKind, LocalSession, Screen};
use crate::presentation::{Badge, Tone};

/// The names [`parse`] understands, in the order of the flow.
pub const NAMES: [&str; 8] = [
    "choose",
    "searching",
    "found",
    "notfound",
    "token",
    "verifying",
    "refused",
    "unreachable",
];

/// The screen called `name`, with sample data, or `None` for an unknown name.
#[must_use]
pub fn parse(name: &str) -> Option<Screen> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "choose" => Screen::Choose,
        "searching" => Screen::Searching,
        "found" => Screen::LocalFound(LocalSession {
            endpoint: "http://127.0.0.1:9876/mcp/".to_owned(),
            server_version: Some("5.10.2".to_owned()),
            account_id: "40218".to_owned(),
            kind: Badge {
                text: "DEMO".to_owned(),
                tone: Tone::Good,
            },
        }),
        "notfound" => Screen::LocalNotFound(Failure {
            kind: FailureKind::Unreachable,
            detail: "The server did not answer. Check your internet connection.".to_owned(),
            raw: String::new(),
        }),
        "token" => Screen::Token { refused: None },
        "verifying" => Screen::Verifying {
            hint: "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}83f4".to_owned(),
        },
        "refused" => Screen::Token {
            refused: Some(Failure {
                kind: FailureKind::Refused,
                detail: "The server refused this token: authentication failed.".to_owned(),
                raw: String::new(),
            }),
        },
        "unreachable" => Screen::Token {
            refused: Some(Failure {
                kind: FailureKind::Unreachable,
                detail: "The server did not answer. Check your internet connection.".to_owned(),
                raw: String::new(),
            }),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_is_a_screen() {
        for name in NAMES {
            assert!(parse(name).is_some(), "{name}");
        }
    }

    #[test]
    fn names_are_not_case_sensitive_and_unknown_ones_are_refused() {
        assert_eq!(parse(" Found "), parse("found"));
        assert!(parse("dashboard").is_none());
    }
}
