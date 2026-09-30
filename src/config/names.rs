//! The rule for names that become part of a path: documents, scopes and named credentials.

use crate::config::error::{ConfigError, Result};

/// The most characters a name can have.
pub const MAX_NAME_LEN: usize = 100;

fn why_not(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("it is empty")
    } else if name.len() > MAX_NAME_LEN {
        Some("it is too long")
    } else if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Some("only letters, digits, `-` and `_` are allowed")
    } else {
        None
    }
}

/// Whether `name` follows the rule of this module.
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    why_not(name).is_none()
}

/// Checks `name` against the rule of this module.
pub fn validate_name(name: &str) -> Result<()> {
    match why_not(name) {
        None => Ok(()),
        Some(reason) => Err(ConfigError::InvalidName {
            name: name.to_owned(),
            reason,
        }),
    }
}

/// `name` made into a valid one: every character the rule does not allow becomes `_`, and an
/// empty or overlong name becomes `_` or is cut.
#[must_use]
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(MAX_NAME_LEN)
        .collect();
    if cleaned.is_empty() {
        "_".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn the_rule_accepts_plain_names_and_refuses_everything_that_can_travel() {
        for good in ["a", "layout", "demo-1", "Live_2", &"x".repeat(MAX_NAME_LEN)] {
            assert!(is_valid_name(good), "{good}");
        }
        for bad in [
            "",
            ".",
            "..",
            "a.b",
            "a/b",
            "a\\b",
            "a b",
            "\u{e9}t\u{e9}",
            "a\0b",
            &"x".repeat(MAX_NAME_LEN + 1),
        ] {
            assert!(!is_valid_name(bad), "{bad:?}");
            assert!(matches!(
                validate_name(bad),
                Err(ConfigError::InvalidName { .. })
            ));
        }
    }

    proptest! {
        #[test]
        fn sanitizing_always_gives_a_valid_name(name in ".{0,300}") {
            prop_assert!(is_valid_name(&sanitize(&name)));
        }

        #[test]
        fn a_valid_name_is_its_own_sanitized_form(name in "[A-Za-z0-9_-]{1,100}") {
            prop_assert!(is_valid_name(&name));
            prop_assert_eq!(sanitize(&name), name);
        }

        #[test]
        fn sanitizing_twice_is_sanitizing_once(name in ".{0,200}") {
            let once = sanitize(&name);
            prop_assert_eq!(sanitize(&once), once);
        }
    }
}
