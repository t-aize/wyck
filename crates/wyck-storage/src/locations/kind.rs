//! The kinds of data that can be kept in a folder of the user's choice.

use std::fmt;

/// A kind of data that can be kept in a folder of the user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Layout, shortcuts and every document shared by the accounts.
    Settings,
    /// Drawings, alerts and watchlists, one folder per account.
    Accounts,
    /// The scripts of the indicators the user writes.
    Indicators,
    /// The copies of everything, kept for restoring.
    Backups,
    /// The log files and the reports of a panic.
    Logs,
}

impl Kind {
    pub const ALL: [Self; 5] = [
        Self::Settings,
        Self::Accounts,
        Self::Indicators,
        Self::Backups,
        Self::Logs,
    ];

    /// The name of the kind in files: the name of a folder inside a cloud folder, and the `kind`
    /// of a marker.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Accounts => "accounts",
            Self::Indicators => "indicators",
            Self::Backups => "backups",
            Self::Logs => "logs",
        }
    }

    /// The kind a [`Self::code`] names.
    #[must_use]
    pub(crate) fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// A few words for a list.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::Accounts => "Account data",
            Self::Indicators => "Indicator scripts",
            Self::Backups => "Backups",
            Self::Logs => "Logs",
        }
    }

    /// What is in it, for the line under the label.
    #[must_use]
    pub const fn help(self) -> &'static str {
        match self {
            Self::Settings => {
                "Look, colors, layout, shortcuts, favorites, ticket and indicator settings"
            }
            Self::Accounts => {
                "Drawings and their saved looks, watchlists and alerts, one folder per account number"
            }
            Self::Indicators => "The .rhai files of your own indicators",
            Self::Backups => "The copies kept for restoring, automatic ones included",
            Self::Logs => "What the app wrote about itself, and the report of a crash",
        }
    }

    /// Whether the cloud preset puts it in the cloud folder. The logs say what happened on one
    /// computer, so they stay where they are.
    #[must_use]
    pub(crate) const fn in_cloud_preset(self) -> bool {
        !matches!(self, Self::Logs)
    }

    /// Whether moving it carries its files along. The old logs stay where they were written.
    #[must_use]
    pub(crate) const fn carries_files(self) -> bool {
        !matches!(self, Self::Logs)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
