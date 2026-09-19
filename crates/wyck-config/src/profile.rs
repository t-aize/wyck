//! A configured connection profile: the non-secret half of an account (display name,
//! which service it's for, its endpoint). The secret half (the token itself) never
//! lives here; see [`crate::secret`].

use serde::{Deserialize, Serialize};

/// A stable, opaque identifier for one [`ProfileConfig`], generated once when the
/// profile is created and never reused. Also doubles as the [`crate::secret::SecretKey`]
/// derivation input (see [`crate::secret::SecretKey::for_profile`]), so a profile and
/// its credential are always looked up by the same identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// Generates a new, statistically-unique profile id (UUID v4).
    pub fn new_random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Wraps an existing id string (e.g. one read back from config). Prefer
    /// [`Self::new_random`] when creating a brand new profile.
    pub fn from_raw(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The raw id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A configured connection profile: everything about an account EXCEPT its token.
///
/// `service` is deliberately a free-form string rather than an enum owned by this
/// crate: `wyck-config` has no knowledge of cTrader, or of any other specific
/// broker/API, on purpose. A caller in `ctrader-mcp`'s orbit might use
/// `"ctrader-remote"`/`"ctrader-local"`; a future crate for a different broker or a
/// different kind of API key entirely reuses the exact same struct with its own tag.
/// This is what makes the crate genuinely shared infrastructure rather than
/// cTrader-specific config wearing a generic name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub id: ProfileId,
    /// User-facing label shown in the TUI/GUI (e.g. `"Live: FTMO 100k"`,
    /// `"Demo: scalping"`).
    pub display_name: String,
    /// Free-form tag identifying which client/service this profile authenticates
    /// against. Not validated or interpreted by this crate.
    pub service: String,
    /// The connection endpoint, if the service is addressed by URI (e.g. an MCP
    /// endpoint). `None` for services that resolve their endpoint another way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

impl ProfileConfig {
    /// Creates a new profile with a freshly generated [`ProfileId`].
    pub fn new(
        display_name: impl Into<String>,
        service: impl Into<String>,
        endpoint: Option<String>,
    ) -> Self {
        Self {
            id: ProfileId::new_random(),
            display_name: display_name.into(),
            service: service.into(),
            endpoint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_ids_are_unique() {
        let a = ProfileId::new_random();
        let b = ProfileId::new_random();
        assert_ne!(a, b);
    }

    #[test]
    fn profile_config_round_trips_through_toml() {
        let profile = ProfileConfig::new(
            "Live: FTMO 100k",
            "ctrader-remote",
            Some("https://mcp.ctrader.com/trading/mcp".to_owned()),
        );
        let toml_text = toml::to_string(&profile).unwrap();
        let parsed: ProfileConfig = toml::from_str(&toml_text).unwrap();
        assert_eq!(profile, parsed);
    }

    #[test]
    fn endpoint_is_omitted_from_toml_when_absent() {
        let profile = ProfileConfig::new("Minimal profile", "some-service", None);
        let toml_text = toml::to_string(&profile).unwrap();
        let value: toml::Value = toml::from_str(&toml_text).unwrap();
        assert!(
            value.get("endpoint").is_none(),
            "expected no `endpoint` key in:\n{toml_text}"
        );
    }
}
