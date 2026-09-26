//! What the user arranges and wants back at the next start: the layout of the charts and their
//! timeframes and types, the timeframes they favor, the time zone, and (for the account in use)
//! favorite symbols and watchlists.
//!
//! The data lives in TOML documents managed by [`wyck_config::DocumentStore`]; this module owns
//! their shape. Two rules keep old files working: every field has a default, so a file from an
//! older version (or a hand-edited one) still loads, and everything read is checked and repaired
//! by [`Preferences::normalized`], so a value this version does not know (a timeframe that was
//! removed, a layout that no longer exists) becomes a sane default instead of an error.
//!
//! - `preferences` is global, shared by every account.
//! - `watchlists` is per account, because symbols are named by the broker behind it.
//!
//! [`Workspace`] is the live copy, a gpui entity: views read it, change it through
//! [`Workspace::edit_preferences`] and [`Workspace::edit_watchlists`], and are told when it
//! changed. Each change is saved a moment later (see [`saver`]) and once more on exit.

mod saver;

pub use self::saver::Saver;
use wyck_config::DocumentStore;
pub use wyck_state::*;

const PREFERENCES: &str = "preferences";
const WATCHLISTS: &str = "watchlists";

/// The two places documents are kept.
#[derive(Clone)]
pub struct Documents {
    /// Shared by every account.
    pub global: DocumentStore,
    /// Private to the account in use.
    pub account: DocumentStore,
}

// ---- the live copy ----

pub struct Workspace {
    preferences: Preferences,
    watchlists: Watchlists,
    preferences_saver: Saver<Preferences>,
    watchlists_saver: Saver<Watchlists>,
}

impl Workspace {
    /// Reads both documents (repairing them), and arranges for the last changes to be written
    /// when the app quits.
    pub fn new(documents: &Documents, cx: &mut gpui::Context<Self>) -> Self {
        let preferences = documents
            .global
            .load_or_default::<Preferences>(PREFERENCES)
            .normalized();
        let watchlists = documents
            .account
            .load_or_default::<Watchlists>(WATCHLISTS)
            .normalized();
        cx.on_app_quit(|this, _cx| {
            this.preferences_saver.flush();
            this.watchlists_saver.flush();
            async {}
        })
        .detach();
        Self {
            preferences,
            watchlists,
            preferences_saver: Saver::new(documents.global.clone(), PREFERENCES),
            watchlists_saver: Saver::new(documents.account.clone(), WATCHLISTS),
        }
    }

    /// Writes what waits to be saved now, for what is about to read the files (a backup).
    pub fn flush(&self) {
        self.preferences_saver.flush();
        self.watchlists_saver.flush();
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn watchlists(&self) -> &Watchlists {
        &self.watchlists
    }

    /// Changes the preferences, tells the views, and saves. Nothing happens if `change` leaves
    /// them as they were.
    pub fn edit_preferences<R>(
        &mut self,
        cx: &mut gpui::Context<Self>,
        change: impl FnOnce(&mut Preferences) -> R,
    ) -> R {
        let before = self.preferences.clone();
        let result = change(&mut self.preferences);
        if self.preferences != before {
            self.preferences_saver.schedule(self.preferences.clone());
            cx.notify();
        }
        result
    }

    /// Changes the watchlists, tells the views, and saves.
    pub fn edit_watchlists<R>(
        &mut self,
        cx: &mut gpui::Context<Self>,
        change: impl FnOnce(&mut Watchlists) -> R,
    ) -> R {
        let before = self.watchlists.clone();
        let result = change(&mut self.watchlists);
        if self.watchlists != before {
            self.watchlists_saver.schedule(self.watchlists.clone());
            cx.notify();
        }
        result
    }
}
