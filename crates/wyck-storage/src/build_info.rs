//! Compile-time information about this build of the application.

/// Whether this binary is meant for development or for end users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Development,
    Production,
}

impl BuildMode {
    /// The mode fixed by the Cargo profile. It cannot be changed at runtime.
    pub const CURRENT: Self = if cfg!(debug_assertions) {
        Self::Development
    } else {
        Self::Production
    };

    pub const fn label(self) -> &'static str {
        match self {
            Self::Development => "Development",
            Self::Production => "Production",
        }
    }

    /// The name of the mode in files, such as the marker of a moved folder.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    pub const fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
