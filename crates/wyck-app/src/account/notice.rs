//! What the user is told about their orders and about a failed request, in words.
//!
//! The account book (`wyck_trading::book`) says what happened as an [`Outcome`]; this module
//! turns it into a [`Notice`] the dashboard shows as a toast. A server refusal code and a failed
//! request are put in sentences here too.

use crate::broker::Error as ApiError;
use wyck_trading::book::{OrderLine, Outcome};
use wyck_trading::contract::format_lots;
use wyck_trading::enums::OrderType;
use wyck_trading::guard::{Lock, Refusal, Warning};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub enum NoticeAction {
    ClosePosition(i64),
    /// Move the stop loss of this position to its entry price.
    BreakEven(i64),
    CancelOrder(i64),
}

/// Something to tell the user about what happened to their orders.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub(crate) fn new(tone: Tone, title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tone,
            title: title.into(),
            message: message.into(),
            hint: None,
            details: None,
            actions: Vec::new(),
        }
    }

    /// An order that failed its own checks (a missing volume, a stop on the wrong side).
    pub(crate) fn incomplete(error: &impl std::fmt::Display) -> Self {
        Self::new(Tone::Error, "The order is not complete", error.to_string())
    }

    /// An order a hard safety limit refused.
    pub(crate) fn blocked(reason: impl Into<String>) -> Self {
        Self::new(Tone::Warning, "Blocked by your safety settings", reason)
            .hint(Some("Change the limits in Settings, Safety.".to_owned()))
    }

    /// An order dropped because the same one was sent a moment ago.
    pub(crate) fn repeated() -> Self {
        Self::new(
            Tone::Info,
            "Order not sent again",
            "The same order was sent a moment ago.",
        )
    }

    /// Adds a button the notice offers.
    #[must_use]
    pub(crate) fn action(mut self, action: NoticeAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Adds what to do about it.
    #[must_use]
    pub(crate) fn hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
        self
    }

    /// Adds the exact words behind it.
    #[must_use]
    pub(crate) fn details(mut self, details: impl Into<String>) -> Self {
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    /// What went wrong, as a sentence.
    pub(crate) message: String,
    /// What to do about it, when there is something to do.
    pub(crate) hint: Option<String>,
}

/// A text as a sentence: its first letter in capital, and a full stop at the end.
pub(crate) fn sentence(text: &str) -> String {
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
        other @ ApiError::Config(_) => Reason {
            message: sentence(&other.to_string()),
            hint: None,
        },
    }
}

/// An order in a few words: `Buy 0.01 EURUSD limit at 1.05000`. Empty when there is none.
fn phrase(line: Option<&OrderLine>) -> String {
    let Some(line) = line else {
        return String::new();
    };
    let side = if line.buy { "Buy" } else { "Sell" };
    let kind = line.kind.map_or("order", OrderType::label);
    let price = line
        .price
        .as_ref()
        .map(|p| format!(" at {p}"))
        .unwrap_or_default();
    format!(
        "{side} {} {} {kind}{price}",
        format_lots(line.lots),
        line.symbol
    )
}

/// Why nothing new can be sent, as a sentence for a banner. `money` writes an amount with its
/// currency; `now` is the time the cooldown is counted from.
pub(crate) fn lock_text(lock: &Lock, money: &dyn Fn(f64) -> String, now: i64) -> String {
    match *lock {
        Lock::KillSwitch => "Kill switch is on: no new orders can be sent.".to_owned(),
        Lock::DailyLoss { loss, limit } => format!(
            "Daily loss limit reached ({} of {}): no new orders today.",
            money(loss),
            money(limit)
        ),
        Lock::Cooldown { until } => {
            let left = ((until - now).max(0) + 59_999) / 60_000;
            format!("Cooling down after a loss: {left} min left.")
        }
    }
}

/// Why the limits refuse an order, as a sentence.
pub(crate) fn refusal_text(refusal: &Refusal, money: &dyn Fn(f64) -> String, now: i64) -> String {
    match *refusal {
        Refusal::Locked(lock) => lock_text(&lock, money, now),
        Refusal::OrderTooBig { lots, limit } => format!(
            "The order is {} lots: the limit per order is {}.",
            format_lots(lots),
            format_lots(limit)
        ),
        Refusal::OneClickTooBig { limit } => format!(
            "One-click orders are limited to {} lots. Use the confirmation for a larger one.",
            format_lots(limit)
        ),
        Refusal::SymbolTooBig { lots, limit } => format!(
            "That would make {} lots open on this symbol: the limit is {}.",
            format_lots(lots),
            format_lots(limit)
        ),
        Refusal::TooManyPositions { open, limit } => {
            format!("{open} positions are open: the limit is {limit}.")
        }
        Refusal::TooManyOrdersToday { sent, limit } => {
            format!("{sent} orders were sent today: the limit is {limit}.")
        }
        Refusal::StopLossRequired => "A stop loss is required by your safety settings.".to_owned(),
        Refusal::PriceTooFar {
            away_pct,
            limit_pct,
        } => format!(
            "The price is {away_pct:.1}% from the market: the limit is {limit_pct:.1}%. Check the decimal point."
        ),
    }
}

/// What is worth a second look before an order is sent, as a sentence.
pub(crate) fn warning_text(warning: &Warning) -> String {
    match *warning {
        Warning::SpreadTooWide {
            spread_pips,
            limit_pips,
        } => format!(
            "The spread is {spread_pips:.1} pips, wider than your limit of {limit_pips:.1}."
        ),
    }
}

/// The notice for what happened to an order, in words.
pub(crate) fn of(outcome: Outcome) -> Notice {
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
            let mut notice = Notice::new(
                Tone::Success,
                title,
                format!("{}{at}", phrase(what.as_ref())),
            );
            if let Some(id) = open_position {
                notice = notice
                    .action(NoticeAction::ClosePosition(id))
                    .action(NoticeAction::BreakEven(id));
            }
            notice
        }
        Outcome::PartlyFilled { what } => {
            Notice::new(Tone::Info, "Order partly filled", phrase(what.as_ref()))
        }
        Outcome::Placed { what, order_id } => {
            Notice::new(Tone::Info, "Order placed", phrase(what.as_ref()))
                .action(NoticeAction::CancelOrder(order_id))
        }
        Outcome::Changed { what } => {
            Notice::new(Tone::Info, "Order changed", phrase(what.as_ref()))
        }
        Outcome::Cancelled { what } => {
            Notice::new(Tone::Info, "Order cancelled", phrase(what.as_ref()))
        }
        Outcome::Expired { what } => {
            Notice::new(Tone::Warning, "Order expired", phrase(what.as_ref()))
        }
        Outcome::Refused { what, code } => {
            let what = phrase(what.as_ref());
            let code = code.unwrap_or_else(|| "refused by the server".to_owned());
            Notice::new(
                Tone::Error,
                "Order refused",
                format!(
                    "{what}{}{}",
                    if what.is_empty() { "" } else { ": " },
                    explain(&code)
                ),
            )
            .hint(refusal(&code, None).hint)
            .details(code)
        }
        Outcome::BalanceChanged => {
            Notice::new(Tone::Info, "Balance changed", "A deposit or withdrawal")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(buy: bool, lots: f64, kind: Option<OrderType>) -> OrderLine {
        OrderLine {
            buy,
            lots,
            symbol: "EURUSD".to_owned(),
            kind,
            price: None,
        }
    }

    fn money(amount: f64) -> String {
        format!("{amount:.2}")
    }

    #[test]
    fn an_order_is_told_by_its_side_size_symbol_and_kind() {
        assert_eq!(
            phrase(Some(&line(true, 0.01, Some(OrderType::Limit)))),
            "Buy 0.01 EURUSD limit"
        );
        assert_eq!(phrase(Some(&line(false, 1.0, None))), "Sell 1 EURUSD order");
        assert_eq!(phrase(None), "");
    }

    #[test]
    fn a_server_answer_with_no_code_says_it_refused() {
        let notice = of(Outcome::Refused {
            what: None,
            code: None,
        });
        assert_eq!(notice.details.as_deref(), Some("refused by the server"));
    }

    #[test]
    fn a_lock_counts_the_minutes_left_rounding_up() {
        let cooldown = Lock::Cooldown { until: 90_000 };
        assert_eq!(
            lock_text(&cooldown, &money, 0),
            "Cooling down after a loss: 2 min left."
        );
        assert_eq!(
            lock_text(
                &Lock::DailyLoss {
                    loss: 250.0,
                    limit: 200.0
                },
                &money,
                0
            ),
            "Daily loss limit reached (250.00 of 200.00): no new orders today."
        );
    }

    #[test]
    fn every_refusal_and_warning_has_its_words() {
        let words = |refusal| refusal_text(&refusal, &money, 0);
        assert_eq!(
            words(Refusal::OrderTooBig {
                lots: 2.0,
                limit: 1.0
            }),
            "The order is 2 lots: the limit per order is 1."
        );
        assert_eq!(
            words(Refusal::TooManyPositions { open: 3, limit: 3 }),
            "3 positions are open: the limit is 3."
        );
        assert_eq!(
            words(Refusal::PriceTooFar {
                away_pct: 12.34,
                limit_pct: 5.0
            }),
            "The price is 12.3% from the market: the limit is 5.0%. Check the decimal point."
        );
        assert_eq!(
            words(Refusal::Locked(Lock::KillSwitch)),
            "Kill switch is on: no new orders can be sent."
        );
        assert_eq!(
            warning_text(&Warning::SpreadTooWide {
                spread_pips: 3.0,
                limit_pips: 1.0
            }),
            "The spread is 3.0 pips, wider than your limit of 1.0."
        );
    }

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
            what: Some(line(true, 0.01, Some(OrderType::Limit))),
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
            what: Some(line(false, 1.0, None)),
            price: None,
            closed: true,
            open_position: None,
        });
        assert_eq!(notice.title, "Position closed");
        assert_eq!(notice.actions, [] as [NoticeAction; 0]);
    }

    #[test]
    fn a_refusal_is_explained_and_keeps_the_code_for_a_bug_report() {
        let notice = of(Outcome::Refused {
            what: Some(line(true, 1.0, None)),
            code: Some("NOT_ENOUGH_MONEY".into()),
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
