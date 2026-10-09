//! The states of the sign-in and how they change. No window and no network: tested alone.
//!
//! One modal covers the whole sign-in. Its phase says what it shows, and a failure, when there
//! is one, is shown above the form with the phase back at [`Phase::Editing`], so the typed
//! fields are never lost.

use crate::infra::ctrader::Error;

/// A trading account the user can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountChoice {
    /// The id to authorize.
    pub id: i64,
    /// How the account is named: `Broker Live - 1234567`.
    pub label: String,
    /// Whether it is a live (real money) account.
    pub is_live: bool,
}

/// What the sign-in modal is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Looking for a saved sign-in.
    Restoring,
    /// Waiting for the user to fill in the form.
    Editing,
    /// Checking the application credentials with cTrader.
    Verifying,
    /// The browser is open on cTrader's consent page.
    WaitingBrowser {
        /// The consent page, to open again or copy.
        url: String,
    },
    /// Several accounts are covered: the user picks one.
    ChoosingAccount {
        /// The accounts that fit the chosen environment.
        accounts: Vec<AccountChoice>,
        /// The one picked now.
        selected: usize,
    },
    /// The chosen account is being authorized.
    Authorizing {
        /// Its name.
        label: String,
    },
    /// A session exists.
    Connected,
}

/// Why a sign-in step failed. Each kind has its own words and its own advice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The Client ID or the Client secret was left empty.
    MissingFields,
    /// cTrader refused the application credentials.
    CredentialsRejected,
    /// cTrader could not be reached.
    Unreachable,
    /// The local port for the redirect is taken.
    PortBusy(u16),
    /// The consent was not given in time.
    TimedOut,
    /// The consent page sent back an error or something unreadable.
    ConsentFailed(String),
    /// No account of the chosen environment is covered by the token.
    NoAccount {
        /// Whether the live environment was chosen.
        live: bool,
    },
    /// The account could not be authorized.
    AuthorizeFailed(String),
    /// A saved session could not be used any more.
    SessionExpired,
    /// Anything else, in the words of the error.
    Other(String),
}

impl Failure {
    /// Sorts an error of the application check.
    pub fn from_connect(error: &Error) -> Self {
        match error {
            Error::Server { .. } | Error::Auth(_) => Self::CredentialsRejected,
            Error::Transport(_) | Error::Timeout { .. } | Error::Closed => Self::Unreachable,
            other => Self::Other(other.to_string()),
        }
    }

    /// Sorts an error of the wait for the redirect and of the code exchange.
    pub fn from_consent(error: &Error) -> Self {
        match error {
            Error::Timeout { .. } => Self::TimedOut,
            Error::Transport(_) | Error::Closed => Self::Unreachable,
            other => Self::ConsentFailed(other.to_string()),
        }
    }

    /// The short line above the explanation.
    pub fn title(&self) -> &'static str {
        match self {
            Self::MissingFields => "Fill in both fields",
            Self::CredentialsRejected => "cTrader did not accept this application",
            Self::Unreachable => "Can't reach cTrader",
            Self::PortBusy(_) => "The sign-in port is busy",
            Self::TimedOut => "The sign-in took too long",
            Self::ConsentFailed(_) => "Couldn't finish signing in",
            Self::NoAccount { .. } => "No account to connect",
            Self::AuthorizeFailed(_) => "Couldn't connect this account",
            Self::SessionExpired => "Your session has ended",
            Self::Other(_) => "Something went wrong",
        }
    }

    /// What happened and what to do about it.
    pub fn message(&self) -> String {
        match self {
            Self::MissingFields => "Enter the Client ID and the Client secret of your application.".into(),
            Self::CredentialsRejected => {
                "Check the Client ID and the Client secret, and that the application is enabled on cTrader Connect.".into()
            }
            Self::Unreachable => {
                "Check your internet connection, then try again.".into()
            }
            Self::PortBusy(port) => format!(
                "Port {port} is used by another program or another Wyck window. Close it and try again."
            ),
            Self::TimedOut => {
                "Allow access on cTrader's page within five minutes, then try again.".into()
            }
            Self::ConsentFailed(detail) => detail.clone(),
            Self::NoAccount { live } => format!(
                "None of the accounts you allowed is a {} account. Pick one on cTrader's page, or switch the environment.",
                if *live { "live" } else { "demo" }
            ),
            Self::AuthorizeFailed(detail) | Self::Other(detail) => detail.clone(),
            Self::SessionExpired => "Sign in again to keep trading.".into(),
        }
    }
}

/// Checks the two fields of the form before anything is sent.
///
/// # Errors
///
/// [`Failure::MissingFields`] when one of them is empty.
pub fn check_fields(client_id: &str, client_secret: &str) -> Result<(), Failure> {
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        Err(Failure::MissingFields)
    } else {
        Ok(())
    }
}

/// The phase and the last failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInState {
    phase: Phase,
    failure: Option<Failure>,
    failures: u64,
}

impl SignInState {
    /// A sign-in that looks for a saved session first.
    pub fn restoring() -> Self {
        Self {
            phase: Phase::Restoring,
            failure: None,
            failures: 0,
        }
    }

    /// A sign-in that starts on the form.
    pub fn editing() -> Self {
        Self {
            phase: Phase::Editing,
            failure: None,
            failures: 0,
        }
    }

    /// What the modal shows.
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// The failure to show above the form.
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }

    /// How many failures there have been, so each new one replays the banner's entrance.
    pub fn failures(&self) -> u64 {
        self.failures
    }

    /// Whether something is in progress, so the form is locked and Escape cancels.
    pub fn is_busy(&self) -> bool {
        !matches!(self.phase, Phase::Editing | Phase::Connected)
    }

    /// The form is sent. Only the form can be sent.
    pub fn submit(&mut self) -> bool {
        if self.phase != Phase::Editing {
            return false;
        }
        self.failure = None;
        self.phase = Phase::Verifying;
        true
    }

    /// The application is verified and the browser is open.
    pub fn browser_opened(&mut self, url: String) {
        if self.phase == Phase::Verifying {
            self.phase = Phase::WaitingBrowser { url };
        }
    }

    /// The accounts of the token, already narrowed to the chosen environment.
    pub fn accounts_listed(&mut self, accounts: Vec<AccountChoice>) {
        if matches!(self.phase, Phase::WaitingBrowser { .. }) {
            self.phase = Phase::ChoosingAccount {
                accounts,
                selected: 0,
            };
        }
    }

    /// Picks the account at `index`.
    pub fn choose(&mut self, index: usize) {
        if let Phase::ChoosingAccount { accounts, selected } = &mut self.phase
            && index < accounts.len()
        {
            *selected = index;
        }
    }

    /// The account picked now.
    pub fn chosen(&self) -> Option<&AccountChoice> {
        match &self.phase {
            Phase::ChoosingAccount { accounts, selected } => accounts.get(*selected),
            _ => None,
        }
    }

    /// The account is being authorized.
    pub fn authorizing(&mut self, label: String) {
        if matches!(
            self.phase,
            Phase::ChoosingAccount { .. } | Phase::WaitingBrowser { .. }
        ) {
            self.phase = Phase::Authorizing { label };
        }
    }

    /// Back to the form with the reason shown, whatever was in progress.
    pub fn fail(&mut self, failure: Failure) {
        self.phase = Phase::Editing;
        self.failure = Some(failure);
        self.failures += 1;
    }

    /// Back to the form without a failure: the user gave up.
    pub fn cancel(&mut self) {
        if self.is_busy() {
            self.phase = Phase::Editing;
            self.failure = None;
        }
    }

    /// There was no saved session to restore.
    pub fn nothing_to_restore(&mut self) {
        if self.phase == Phase::Restoring {
            self.phase = Phase::Editing;
        }
    }

    /// A session exists.
    pub fn connected(&mut self) {
        self.phase = Phase::Connected;
        self.failure = None;
    }

    /// The session ended: the form again, with the reason.
    pub fn expired(&mut self) {
        self.fail(Failure::SessionExpired);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: i64) -> AccountChoice {
        AccountChoice {
            id,
            label: format!("Broker Demo - {id}"),
            is_live: false,
        }
    }

    #[test]
    fn a_full_sign_in_walks_the_phases_in_order() {
        let mut state = SignInState::editing();
        assert!(!state.is_busy());
        assert!(state.submit());
        assert_eq!(state.phase(), &Phase::Verifying);
        assert!(state.is_busy());
        state.browser_opened("https://example.test/consent".into());
        assert!(matches!(state.phase(), Phase::WaitingBrowser { .. }));
        state.accounts_listed(vec![account(1), account(2)]);
        state.choose(1);
        assert_eq!(state.chosen().map(|a| a.id), Some(2));
        state.authorizing("Broker Demo - 2".into());
        assert!(matches!(state.phase(), Phase::Authorizing { .. }));
        state.connected();
        assert_eq!(state.phase(), &Phase::Connected);
        assert!(!state.is_busy());
    }

    #[test]
    fn only_the_form_can_be_sent() {
        let mut state = SignInState::restoring();
        assert!(!state.submit());
        state.nothing_to_restore();
        assert!(state.submit());
        assert!(!state.submit());
    }

    #[test]
    fn a_failure_returns_to_the_form_and_counts() {
        let mut state = SignInState::editing();
        state.submit();
        state.fail(Failure::Unreachable);
        assert_eq!(state.phase(), &Phase::Editing);
        assert_eq!(state.failure(), Some(&Failure::Unreachable));
        assert_eq!(state.failures(), 1);
        // Sending again clears the banner.
        state.submit();
        assert_eq!(state.failure(), None);
    }

    #[test]
    fn cancelling_returns_to_the_form_without_a_failure() {
        let mut state = SignInState::editing();
        state.submit();
        state.browser_opened("u".into());
        state.cancel();
        assert_eq!(state.phase(), &Phase::Editing);
        assert_eq!(state.failure(), None);
        // Cancelling the form itself changes nothing.
        state.cancel();
        assert_eq!(state.phase(), &Phase::Editing);
    }

    #[test]
    fn a_pick_out_of_range_is_ignored() {
        let mut state = SignInState::editing();
        state.submit();
        state.browser_opened("u".into());
        state.accounts_listed(vec![account(1)]);
        state.choose(5);
        assert_eq!(state.chosen().map(|a| a.id), Some(1));
    }

    #[test]
    fn steps_out_of_order_are_ignored() {
        let mut state = SignInState::editing();
        state.browser_opened("u".into());
        assert_eq!(state.phase(), &Phase::Editing);
        state.accounts_listed(vec![account(1)]);
        assert_eq!(state.phase(), &Phase::Editing);
        state.authorizing("x".into());
        assert_eq!(state.phase(), &Phase::Editing);
    }

    #[test]
    fn an_ended_session_shows_the_form_with_its_reason() {
        let mut state = SignInState::editing();
        state.connected();
        state.expired();
        assert_eq!(state.phase(), &Phase::Editing);
        assert_eq!(state.failure(), Some(&Failure::SessionExpired));
    }

    #[test]
    fn fields_must_not_be_empty() {
        assert_eq!(check_fields("", "secret"), Err(Failure::MissingFields));
        assert_eq!(check_fields("id", "  "), Err(Failure::MissingFields));
        assert_eq!(check_fields(" id ", " secret "), Ok(()));
    }

    #[test]
    fn errors_are_sorted_into_the_kinds_a_person_can_act_on() {
        let refused = Error::server("CH_CLIENT_AUTH_FAILURE", None, None, None);
        assert_eq!(
            Failure::from_connect(&refused),
            Failure::CredentialsRejected
        );
        let down = Error::Transport("dns".into());
        assert_eq!(Failure::from_connect(&down), Failure::Unreachable);
        let slow = Error::Timeout {
            operation: "the redirect",
        };
        assert_eq!(Failure::from_consent(&slow), Failure::TimedOut);
        assert!(matches!(
            Failure::from_consent(&Error::Auth("denied".into())),
            Failure::ConsentFailed(_)
        ));
    }

    #[test]
    fn every_failure_has_words() {
        for failure in [
            Failure::MissingFields,
            Failure::CredentialsRejected,
            Failure::Unreachable,
            Failure::PortBusy(8765),
            Failure::TimedOut,
            Failure::ConsentFailed("x".into()),
            Failure::NoAccount { live: true },
            Failure::AuthorizeFailed("x".into()),
            Failure::SessionExpired,
            Failure::Other("x".into()),
        ] {
            assert!(!failure.title().is_empty());
            assert!(!failure.message().is_empty());
        }
    }
}
