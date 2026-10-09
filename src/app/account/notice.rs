//! What the user is told about their orders and about a failed request, in words.
//!
//! The account book (`domain::trading::book`) says what happened as an [`Outcome`]; this module
//! turns it into a [`Notice`] the dashboard shows as a toast. A server refusal code and a failed
//! request are put in sentences here too.

use crate::app::broker::Error as ApiError;
use crate::domain::trading::book::Outcome;

/// How serious a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Tone {
    /// Worth knowing.
    Info,
    /// Something went through.
    Success,
    /// Something went through, but not quite as asked.
    Warning,
    /// Something was refused or failed.
    Error,
}

/// Something the user can do about a notice, straight from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoticeAction {
    ClosePosition(i64),
    /// Move the stop loss of this position to its entry price.
    BreakEven(i64),
    /// Cancel this working order.
    CancelOrder(i64),
}

/// Something to tell the user about what happened to their orders.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    /// How serious it is.
    pub tone: Tone,
    /// A short headline.
    pub title: String,
    pub message: String,
    /// What to do about it, when there is something.
    pub hint: Option<String>,
    /// The exact words of the server or of the error, for a bug report.
    pub details: Option<String>,
    /// What can be done about it from the notice itself.
    pub actions: Vec<NoticeAction>,
}

impl Notice {
    /// A notice with a tone, a headline and a message, and nothing else.
    pub fn new(tone: Tone, title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tone,
            title: title.into(),
            message: message.into(),
            hint: None,
            details: None,
            actions: Vec::new(),
        }
    }

    /// Adds a button the notice offers.
    #[must_use]
    pub fn action(mut self, action: NoticeAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Adds what to do about it.
    #[must_use]
    pub fn hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
        self
    }

    /// Adds the exact words behind it.
    #[must_use]
    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// A server refusal in words.
pub fn explain(code: &str) -> String {
    match code {
        "NOT_ENOUGH_MONEY" => "not enough free margin".to_owned(),
        "TRADING_BAD_VOLUME" => "the volume is not one the broker accepts".to_owned(),
        "TRADING_BAD_STOPS" => "the stop loss or take profit is not allowed there".to_owned(),
        "TRADING_DISABLED" => "trading is disabled for this symbol or account".to_owned(),
        "MARKET_CLOSED" => "the market is closed".to_owned(),
        "PROTECTION_IS_TOO_CLOSE_TO_MARKET" => {
            "the protection is too close to the price".to_owned()
        }
        "POSITION_NOT_FOUND" => "the position is already closed".to_owned(),
        "ORDER_NOT_FOUND" => "the order no longer exists".to_owned(),
        "MAX_EXPOSURE_REACHED" => "the most this account may hold is reached".to_owned(),
        "ACCOUNT_NOT_AUTHORIZED" | "CH_ACCESS_TOKEN_INVALID" => {
            "this sign-in has no trading permission: disconnect and sign in again".to_owned()
        }
        other => other.to_lowercase().replace('_', " "),
    }
}

/// A refusal or a failure in words a trader can act on.
#[derive(Debug, Clone, PartialEq)]
pub struct Reason {
    /// What went wrong, as a sentence.
    pub message: String,
    /// What to do about it, when there is something to do.
    pub hint: Option<String>,
}

/// A text as a sentence: its first letter in capital, and a full stop at the end.
pub fn sentence(text: &str) -> String {
    let text = text.trim().trim_end_matches('.');
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

/// A server refusal, from its code and the words it gave, as a sentence with what to do.
///
/// The server's own words are the most exact when the code is a general one (`INVALID_REQUEST`),
/// so a description that names a known problem is put in words first, then the code, then the
/// description as it came.
pub fn refusal(code: &str, description: Option<&str>) -> Reason {
    let reason = |message: &str, hint: &str| Reason {
        message: message.to_owned(),
        hint: Some(hint.to_owned()),
    };
    let described = description.map(str::trim).filter(|d| !d.is_empty());
    if let Some(text) = described {
        let lower = text.to_lowercase();
        if lower.contains("precision") {
            let what = ["stop loss", "take profit", "price", "volume"]
                .into_iter()
                .find(|what| lower.contains(what))
                .unwrap_or("value");
            return reason(
                &format!("The {what} has more decimals than this symbol allows."),
                "Round it to the price step of the symbol and send again.",
            );
        }
    }
    match code {
        "NOT_ENOUGH_MONEY" => reason(
            "Not enough free margin for this order.",
            "Lower the size, or close a position to free some margin.",
        ),
        "TRADING_BAD_VOLUME" => reason(
            "The broker does not accept this volume.",
            "Check the least, the most and the step of the volume for this symbol.",
        ),
        "TRADING_BAD_STOPS" => reason(
            "The stop loss or take profit is not allowed there.",
            "A buy has its stop loss under the price and its take profit over it; a sell the other way round.",
        ),
        "PROTECTION_IS_TOO_CLOSE_TO_MARKET" => reason(
            "The stop loss or take profit is too close to the price.",
            "Move it farther away: the broker asks for a least distance.",
        ),
        "TRADING_BAD_PRICES" => reason(
            "The price of the order is not valid.",
            "A buy limit goes under the ask and a buy stop over it; a sell the other way round.",
        ),
        "TRADING_BAD_EXPIRATION_DATE" => reason(
            "The expiry is not valid.",
            "Pick a time that has not passed.",
        ),
        "TRADING_DISABLED" => reason(
            "Trading is disabled for this symbol or account.",
            "Ask the broker if it is not expected.",
        ),
        "MARKET_CLOSED" => reason("The market is closed.", "Try again when it opens."),
        "POSITION_NOT_FOUND" => reason(
            "The position is already closed.",
            "The lists of the account catch up in a moment.",
        ),
        "ORDER_NOT_FOUND" => reason(
            "The order no longer exists.",
            "It was filled, cancelled or expired.",
        ),
        "MAX_EXPOSURE_REACHED" => reason(
            "The most this account may hold is reached.",
            "Close a position before opening another.",
        ),
        "SYMBOL_NOT_FOUND" | "UNKNOWN_SYMBOL" => reason(
            "The broker does not know this symbol.",
            "Pick it again from the list.",
        ),
        "ACCOUNT_NOT_AUTHORIZED" | "CH_ACCESS_TOKEN_INVALID" | "OA_AUTH_TOKEN_EXPIRED" => reason(
            "This sign-in has no trading permission.",
            "Disconnect and sign in again.",
        ),
        "REQUEST_FREQUENCY_EXCEEDED" => reason(
            "Too many requests in a short time.",
            "Wait a moment and try again.",
        ),
        "SERVER_IS_UNDER_MAINTENANCE" => {
            reason("The server is under maintenance.", "Try again in a while.")
        }
        other => Reason {
            message: match described {
                Some(text) => sentence(text),
                None => sentence(&explain(other)),
            },
            hint: None,
        },
    }
}

/// Any failure of a request, in words a trader can act on.
pub fn describe(error: &ApiError) -> Reason {
    let reason = |message: &str, hint: &str| Reason {
        message: message.to_owned(),
        hint: Some(hint.to_owned()),
    };
    match error {
        ApiError::Server {
            code, description, ..
        } => refusal(code, description.as_deref()),
        ApiError::Timeout { .. } => reason(
            "The server did not answer in time.",
            "The request may have gone through: look at the account before sending it again.",
        ),
        ApiError::Closed => reason(
            "The connection is closed.",
            "Wait for it to come back, then try again.",
        ),
        ApiError::Transport(_) => {
            reason("The connection failed.", "Check the network and try again.")
        }
        ApiError::Auth(_) => reason("Signing in failed.", "Disconnect and sign in again."),
        ApiError::Protocol(_) => Reason {
            message: "The server sent something that could not be read.".to_owned(),
            hint: None,
        },
        other => Reason {
            message: sentence(&other.to_string()),
            hint: None,
        },
    }
}

/// The notice for what happened to an order, in words.
pub fn of(outcome: Outcome) -> Notice {
    fn text(what: &str, tail: &str) -> String {
        format!("{what}{tail}")
    }
    match outcome {
        Outcome::Filled {
            what,
            price,
            closed,
            open_position,
        } => {
            let at = price.map(|p| format!(" at {p}")).unwrap_or_default();
            let title = if closed {
                "Position closed"
            } else {
                "Order filled"
            };
            let mut notice = Notice::new(Tone::Success, title, text(&what, &at));
            if let Some(id) = open_position {
                notice = notice
                    .action(NoticeAction::ClosePosition(id))
                    .action(NoticeAction::BreakEven(id));
            }
            notice
        }
        Outcome::PartlyFilled { what } => Notice::new(Tone::Info, "Order partly filled", what),
        Outcome::Placed { what, order_id } => Notice::new(Tone::Info, "Order placed", what)
            .action(NoticeAction::CancelOrder(order_id)),
        Outcome::Changed { what } => Notice::new(Tone::Info, "Order changed", what),
        Outcome::Cancelled { what } => Notice::new(Tone::Info, "Order cancelled", what),
        Outcome::Expired { what } => Notice::new(Tone::Warning, "Order expired", what),
        Outcome::Refused { what, code } => Notice::new(
            Tone::Error,
            "Order refused",
            format!(
                "{what}{}{}",
                if what.is_empty() { "" } else { ": " },
                explain(&code)
            ),
        )
        .hint(refusal(&code, None).hint)
        .details(code),
        Outcome::BalanceChanged => {
            Notice::new(Tone::Info, "Balance changed", "A deposit or withdrawal")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_general_refusal_is_put_in_words_from_what_the_server_says() {
        let reason = refusal(
            "INVALID_REQUEST",
            Some("Relative stop loss has invalid precision"),
        );
        assert_eq!(
            reason.message,
            "The stop loss has more decimals than this symbol allows."
        );
        assert!(reason.hint.is_some());
        // A code with words of its own wins over a description that says nothing new.
        assert_eq!(
            refusal("NOT_ENOUGH_MONEY", Some("no")).message,
            "Not enough free margin for this order."
        );
        // An unknown code with a description keeps the description, as a sentence.
        assert_eq!(
            refusal("SOMETHING_NEW", Some("the thing is off")).message,
            "The thing is off."
        );
        assert_eq!(refusal("SOMETHING_NEW", None).message, "Something new.");
    }

    #[test]
    fn a_failure_that_is_not_a_refusal_says_what_to_do() {
        let timeout = describe(&ApiError::Timeout {
            operation: "an order",
        });
        assert!(timeout.hint.unwrap().contains("account"));
        let server = describe(&ApiError::server("MARKET_CLOSED", None, None, None));
        assert_eq!(server.message, "The market is closed.");
    }

    #[test]
    fn a_fill_reads_as_an_order_filled_with_buttons_for_the_position() {
        let notice = of(Outcome::Filled {
            what: "Buy 0.01 EURUSD limit".into(),
            price: Some("1.05000".into()),
            closed: false,
            open_position: Some(5),
        });
        assert_eq!(notice.title, "Order filled");
        assert_eq!(notice.message, "Buy 0.01 EURUSD limit at 1.05000");
        assert_eq!(
            notice.actions,
            vec![NoticeAction::ClosePosition(5), NoticeAction::BreakEven(5)]
        );
    }

    #[test]
    fn a_closing_fill_says_the_position_closed() {
        let notice = of(Outcome::Filled {
            what: "Sell 1 EURUSD".into(),
            price: None,
            closed: true,
            open_position: None,
        });
        assert_eq!(notice.title, "Position closed");
        assert!(notice.actions.is_empty());
    }

    #[test]
    fn a_refusal_is_explained_and_keeps_the_code_for_a_bug_report() {
        let notice = of(Outcome::Refused {
            what: "Buy 1 EURUSD".into(),
            code: "NOT_ENOUGH_MONEY".into(),
        });
        assert_eq!(notice.tone, Tone::Error);
        assert!(
            notice.message.ends_with("not enough free margin"),
            "{}",
            notice.message
        );
        assert_eq!(notice.details.as_deref(), Some("NOT_ENOUGH_MONEY"));
        assert_eq!(explain("SOMETHING_ELSE"), "something else");
    }
}
