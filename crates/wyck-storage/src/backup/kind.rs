//! Why a copy of the user's data was made.

/// Why a copy was made: it decides the name of the file and what `BackupStore::prune` keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackupKind {
    /// Made on request, or dropped in the folder by hand.
    Manual,
    /// Made by [`super::store::BackupStore::auto_snapshot`].
    Automatic,
    /// What an import replaced, saved right before it was applied.
    BeforeImport,
    /// What a reset removed, saved right before it was applied.
    BeforeReset,
    /// Everything the user made, saved right before an update was installed: what to come back
    /// to if the new version does not suit.
    BeforeUpdate,
}

impl BackupKind {
    /// The start of the name of the files of this kind.
    #[must_use]
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Self::Manual => "backup",
            Self::Automatic => "auto",
            Self::BeforeImport => "before-import",
            Self::BeforeReset => "before-reset",
            Self::BeforeUpdate => "before-update",
        }
    }

    /// A few words for a list.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "Saved by you",
            Self::Automatic => "Automatic",
            Self::BeforeImport => "Before an import",
            Self::BeforeReset => "Before a reset",
            Self::BeforeUpdate => "Before an update",
        }
    }

    pub(super) fn of(id: &str) -> Self {
        [
            Self::Automatic,
            Self::BeforeImport,
            Self::BeforeReset,
            Self::BeforeUpdate,
        ]
        .into_iter()
        .find(|kind| {
            id.strip_prefix(kind.prefix())
                .is_some_and(|rest| rest.starts_with('-'))
        })
        .unwrap_or(Self::Manual)
    }
}
