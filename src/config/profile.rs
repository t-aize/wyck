//! A configured connection profile: the non-secret half of an account (display name, service and
//! optional Open API settings).

use serde::{Deserialize, Serialize};

/// A stable, opaque identifier for one `ProfileConfig`, generated once when the profile is
/// created and never reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// Generates a new, statistically-unique profile id (UUID v4).
    pub fn new_random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
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

/// A configured connection profile: public connection settings, never secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub id: ProfileId,
    pub display_name: String,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<i64>,
}

impl ProfileConfig {
    /// Creates a new profile with a freshly generated `ProfileId`.
    pub fn new(display_name: impl Into<String>, service: impl Into<String>) -> Self {
        Self {
            id: ProfileId::new_random(),
            display_name: display_name.into(),
            service: service.into(),
            client_id: None,
            callback_port: None,
            account_id: None,
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
        let mut profile = ProfileConfig::new("Live: FTMO 100k", "ctrader-openapi");
        profile.client_id = Some("public-client-id".to_owned());
        profile.callback_port = Some(52123);
        profile.account_id = Some(12_345_678);
        let toml_text = toml::to_string(&profile).unwrap();
        let parsed: ProfileConfig = toml::from_str(&toml_text).unwrap();
        assert_eq!(profile, parsed);
    }

    #[test]
    fn settings_a_profile_does_not_use_are_left_out_of_the_file() {
        let profile = ProfileConfig::new("Minimal profile", "some-service");
        let toml_text = toml::to_string(&profile).unwrap();
        let value: toml::Value = toml::from_str(&toml_text).unwrap();
        for key in ["client_id", "callback_port", "account_id"] {
            assert!(value.get(key).is_none(), "`{key}` in:\n{toml_text}");
        }
    }
}
