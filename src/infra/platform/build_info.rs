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

    pub const fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

/// The version of the package.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_mode_follows_the_compilation_profile() {
        let expected = if cfg!(debug_assertions) {
            BuildMode::Development
        } else {
            BuildMode::Production
        };
        assert_eq!(BuildMode::CURRENT, expected);
        assert_eq!(BuildMode::CURRENT.is_production(), !cfg!(debug_assertions));
    }

    #[test]
    fn the_version_is_valid_semver() {
        assert!(cargo_packager_updater::semver::Version::parse(VERSION).is_ok());
    }
}
