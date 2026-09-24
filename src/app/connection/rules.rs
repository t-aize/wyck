//! The decisions of the connection flow that need no window: which saved profile to reopen, which
//! environment it is for, what an account is called, where its documents live, what permission
//! the sign-in asks for. Kept apart so they are tested on their own.

use wyck::config::ProfileConfig;
use wyck::openapi::Environment;
use wyck::openapi::auth::Scope;
use wyck::openapi::transport::messages::TraderAccount;

/// The permission the sign-in asks for. Trading includes reading, and the app places orders.
pub const SIGN_IN_SCOPE: Scope = Scope::Trading;

/// What every service tag of a profile made by this flow starts with.
const SERVICE_PREFIX: &str = "ctrader-openapi-";

/// The service tag of a profile made by this flow: the environment is part of it.
pub fn service_tag(environment: Environment) -> &'static str {
    match environment {
        Environment::Live => "ctrader-openapi-live",
        Environment::Demo => "ctrader-openapi-demo",
    }
}

/// The environment of a profile made by this flow, from its service tag; `None` for a profile
/// made by something else.
pub fn environment_of(service: &str) -> Option<Environment> {
    match service.strip_prefix(SERVICE_PREFIX)? {
        "live" => Some(Environment::Live),
        "demo" => Some(Environment::Demo),
        _ => None,
    }
}

/// The profile to reopen at start: the active one when it is an Open API profile, else the first
/// Open API profile saved.
pub fn saved_profile<'a>(
    active: Option<&'a ProfileConfig>,
    all: &'a [ProfileConfig],
) -> Option<&'a ProfileConfig> {
    active
        .filter(|p| environment_of(&p.service).is_some())
        .or_else(|| all.iter().find(|p| environment_of(&p.service).is_some()))
}

/// Where the documents of an account are kept: its environment and number, so signing out and
/// in again finds them.
pub fn document_scope(environment: Environment, account_id: i64) -> String {
    let prefix = match environment {
        Environment::Live => "live",
        Environment::Demo => "demo",
    };
    format!("{prefix}-{account_id}")
}

/// The name of a connected account: `Broker Live - 1234567`.
pub fn account_label(account: &TraderAccount) -> String {
    let broker = account.broker_title_short.as_deref().unwrap_or("cTrader");
    let login = account.trader_login.map_or_else(
        || account.ctid_trader_account_id.to_string(),
        |login| login.to_string(),
    );
    let kind = if account.is_live.unwrap_or(false) {
        "Live"
    } else {
        "Demo"
    };
    format!("{broker} {kind} - {login}")
}

/// The redirect address of the local listener, as the user registers it with cTrader.
pub fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck::config::ProfileId;

    fn profile(service: &str) -> ProfileConfig {
        ProfileConfig {
            id: ProfileId::new_random(),
            display_name: service.to_owned(),
            service: service.to_owned(),
            endpoint: None,
            client_id: Some("client".into()),
            callback_port: Some(8765),
            account_id: Some(7),
        }
    }

    #[test]
    fn service_tags_carry_the_environment_both_ways() {
        for environment in [Environment::Live, Environment::Demo] {
            assert_eq!(environment_of(service_tag(environment)), Some(environment));
        }
        assert_eq!(environment_of("ctrader-openapi-staging"), None);
        assert_eq!(environment_of("mcp-server"), None);
        assert_eq!(environment_of("live"), None);
    }

    #[test]
    fn the_active_open_api_profile_is_reopened_first() {
        let other = profile("mcp-server");
        let demo = profile("ctrader-openapi-demo");
        let live = profile("ctrader-openapi-live");
        let all = vec![other.clone(), demo.clone(), live.clone()];
        assert_eq!(
            saved_profile(Some(&live), &all).map(|p| &p.id),
            Some(&live.id)
        );
        // An active profile of another kind is passed over for the first Open API one.
        assert_eq!(
            saved_profile(Some(&other), &all).map(|p| &p.id),
            Some(&demo.id)
        );
        assert_eq!(saved_profile(None, &all).map(|p| &p.id), Some(&demo.id));
        assert!(saved_profile(None, &[other]).is_none());
        assert!(saved_profile(None, &[]).is_none());
    }

    #[test]
    fn documents_are_kept_per_environment_and_account() {
        assert_eq!(document_scope(Environment::Live, 42), "live-42");
        assert_eq!(document_scope(Environment::Demo, 42), "demo-42");
    }

    #[test]
    fn accounts_are_named_by_broker_kind_and_login() {
        let mut account = TraderAccount {
            ctid_trader_account_id: 99,
            is_live: Some(true),
            trader_login: Some(1234567),
            broker_title_short: Some("Pepperstone".into()),
        };
        assert_eq!(account_label(&account), "Pepperstone Live - 1234567");
        account.is_live = None;
        account.trader_login = None;
        account.broker_title_short = None;
        assert_eq!(account_label(&account), "cTrader Demo - 99");
    }

    #[test]
    fn the_sign_in_asks_for_trading() {
        assert_eq!(SIGN_IN_SCOPE.as_str(), "trading");
        assert_eq!(redirect_uri(8765), "http://localhost:8765");
    }
}
