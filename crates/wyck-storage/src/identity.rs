//! The names a build of the app keeps its state under, so a development build and the installed
//! app never share a folder or a credential.
//!
//! A debug build is [`BuildMode::Development`] and uses `wyck-dev` where the installed app uses
//! `wyck`: another config folder and another entry group in the OS keyring. Nothing a developer
//! does while testing can reach the real preferences, the saved copies or the sign-in of the app
//! that is used to trade.
//!
//! Two environment variables bring back the sharing on purpose: [`crate::CONFIG_DIR_ENV`]
//! moves the folders and `KEYRING_SERVICE_ENV` names the keyring group, for example to try a
//! development build on the data of the installed one.

use std::ffi::OsString;

use secrecy::SecretString;

use wyck_platform::build_info::BuildMode;

/// The environment variable that names the keyring group credentials are kept under, whatever the
/// build mode says. An empty value counts as not set.
pub(crate) const KEYRING_SERVICE_ENV: &str = "WYCK_KEYRING_SERVICE";

/// The environment variable that holds a passphrase: when it is set, credentials are kept in
/// encrypted files under the data folder, locked by it, instead of the OS keyring. For a machine
/// with no keyring (Linux without a Secret Service). An empty value counts as not set.
pub(crate) const PASSPHRASE_ENV: &str = "WYCK_PASSPHRASE";

/// The application name of the installed app, used in its folder names and its keyring service.
const PRODUCTION_NAME: &str = "wyck";
const DEVELOPMENT_NAME: &str = "wyck-dev";

/// The application name a build of this mode keeps its state under.
pub(crate) const fn application_name(mode: BuildMode) -> &'static str {
    match mode {
        BuildMode::Production => PRODUCTION_NAME,
        BuildMode::Development => DEVELOPMENT_NAME,
    }
}

/// The keyring service for `mode`: [`KEYRING_SERVICE_ENV`] if `var` gives one, the application name
/// of the mode otherwise.
pub(crate) fn keyring_service_with(
    mode: BuildMode,
    var: impl Fn(&str) -> Option<OsString>,
) -> String {
    var(KEYRING_SERVICE_ENV)
        .and_then(|value| value.into_string().ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| application_name(mode).to_owned())
}

/// The passphrase [`PASSPHRASE_ENV`] gives, if `var` has one.
pub(crate) fn passphrase_with(var: impl Fn(&str) -> Option<OsString>) -> Option<SecretString> {
    var(PASSPHRASE_ENV)
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
        .map(SecretString::from)
}

/// The passphrase of the real environment: see `passphrase_with`.
pub fn passphrase() -> Option<SecretString> {
    passphrase_with(|name| std::env::var_os(name))
}

/// The keyring service of this build, read from the real environment: see `keyring_service_with`.
pub fn keyring_service() -> String {
    keyring_service_with(BuildMode::CURRENT, |name| std::env::var_os(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn a_passphrase_in_the_environment_selects_the_encrypted_files() {
        use secrecy::ExposeSecret;
        let given = passphrase_with(env(&[(PASSPHRASE_ENV, "open sesame")])).unwrap();
        assert_eq!(given.expose_secret(), "open sesame");
        assert!(passphrase_with(env(&[(PASSPHRASE_ENV, "")])).is_none());
        assert!(passphrase_with(env(&[])).is_none());
    }

    #[test]
    fn the_keyring_service_follows_the_mode() {
        assert_eq!(
            keyring_service_with(BuildMode::Production, env(&[])),
            "wyck"
        );
        assert_eq!(
            keyring_service_with(BuildMode::Development, env(&[])),
            "wyck-dev"
        );
    }

    #[test]
    fn the_environment_overrides_the_keyring_service() {
        let var = env(&[(KEYRING_SERVICE_ENV, " wyck ")]);
        assert_eq!(keyring_service_with(BuildMode::Development, var), "wyck");
    }

    #[test]
    fn an_empty_or_blank_override_is_ignored() {
        for value in ["", "   "] {
            let pairs = [(KEYRING_SERVICE_ENV, value)];
            assert_eq!(
                keyring_service_with(BuildMode::Development, env(&pairs)),
                "wyck-dev"
            );
        }
    }
}
