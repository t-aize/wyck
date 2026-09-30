use std::time::Duration;

use secrecy::ExposeSecret;
use tokio::sync::mpsc::UnboundedSender;

use super::Msg;
use crate::config::{CLIENT_SECRET, ConfigError, ProfileConfig, ProfileId, WyckConfig};
use crate::openapi::auth::{
    CallbackListener, OAuthClient, Scope, TokenSet, authorization_url, new_state,
};
use crate::openapi::{Client, ClientBuilder, ClientCredentials, Environment, TraderAccount};
use crate::session_tokens::{to_stored, to_token_set};

pub const CALLBACK_PORT: u16 = 8765;
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
const SERVICE_PREFIX: &str = "ctrader-openapi-";

pub fn service_tag(environment: Environment) -> &'static str {
    match environment {
        Environment::Live => "ctrader-openapi-live",
        Environment::Demo => "ctrader-openapi-demo",
    }
}

pub fn environment_of(service: &str) -> Option<Environment> {
    match service.strip_prefix(SERVICE_PREFIX)? {
        "live" => Some(Environment::Live),
        "demo" => Some(Environment::Demo),
        _ => None,
    }
}

fn saved_profile<'a>(
    active: Option<&'a ProfileConfig>,
    all: &'a [ProfileConfig],
) -> Option<&'a ProfileConfig> {
    active
        .filter(|p| environment_of(&p.service).is_some())
        .or_else(|| all.iter().find(|p| environment_of(&p.service).is_some()))
}

pub fn document_scope(environment: Environment, account_id: i64) -> String {
    match environment {
        Environment::Live => format!("live-{account_id}"),
        Environment::Demo => format!("demo-{account_id}"),
    }
}

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

pub struct Saved {
    pub profile_id: ProfileId,
    pub label: String,
    pub environment: Environment,
    pub credentials: ClientCredentials,
    pub account_id: i64,
    pub tokens: TokenSet,
}

pub fn saved_connection(config: &WyckConfig) -> Result<Option<Saved>, ConfigError> {
    let Some(profile) = saved_profile(config.active_profile(), config.profiles()) else {
        return Ok(None);
    };
    let (Some(environment), Some(client_id), Some(account_id)) = (
        environment_of(&profile.service),
        profile.client_id.clone(),
        profile.account_id,
    ) else {
        return Ok(None);
    };
    let Some(secret) = config.profile_secret(&profile.id, CLIENT_SECRET)? else {
        return Ok(None);
    };
    let Some(tokens) = config.openapi_token_storage(&profile.id).load()? else {
        return Ok(None);
    };
    Ok(Some(Saved {
        profile_id: profile.id.clone(),
        label: profile.display_name.clone(),
        environment,
        credentials: ClientCredentials::new(client_id, secret.expose_secret()),
        account_id,
        tokens: to_token_set(tokens),
    }))
}

pub fn forget_connections(config: &mut WyckConfig) -> Result<(), ConfigError> {
    let stale: Vec<ProfileId> = config
        .profiles()
        .iter()
        .filter(|p| environment_of(&p.service).is_some())
        .map(|p| p.id.clone())
        .collect();
    for id in stale {
        config.remove_profile(&id)?;
    }
    Ok(())
}

pub fn save_connection(
    config: &mut WyckConfig,
    credentials: &ClientCredentials,
    environment: Environment,
    account: &TraderAccount,
    tokens: &TokenSet,
) -> Result<Saved, ConfigError> {
    forget_connections(config)?;
    let label = account_label(account);
    let id = config.add_profile(label.clone(), service_tag(environment))?;
    config.set_openapi_profile(
        &id,
        credentials.client_id.clone(),
        CALLBACK_PORT,
        account.ctid_trader_account_id,
    )?;
    config.set_profile_secret(&id, CLIENT_SECRET, &credentials.client_secret)?;
    config.openapi_token_storage(&id).save(&to_stored(tokens))?;
    config.set_active_profile(Some(id.clone()))?;
    Ok(Saved {
        profile_id: id,
        label,
        environment,
        credentials: credentials.clone(),
        account_id: account.ctid_trader_account_id,
        tokens: tokens.clone(),
    })
}

pub struct SignedIn {
    pub credentials: ClientCredentials,
    pub environment: Environment,
    pub client: Client,
    pub tokens: TokenSet,
    pub accounts: Vec<TraderAccount>,
}

pub async fn sign_in(
    credentials: ClientCredentials,
    environment: Environment,
    tx: UnboundedSender<Msg>,
) -> Result<SignedIn, String> {
    let listener = CallbackListener::bind(CALLBACK_PORT)
        .await
        .map_err(|e| format!("could not listen on port {CALLBACK_PORT}: {e}"))?;
    let client = ClientBuilder::new(environment)
        .credentials(credentials.clone())
        .connect()
        .await
        .map_err(|e| format!("cTrader rejected the client id or secret: {e}"))?;

    let redirect = listener.redirect_uri();
    let state = new_state();
    let url = authorization_url(&credentials.client_id, &redirect, Scope::Trading, &state);
    let _ = tx.send(Msg::Url(url.clone()));
    let _ = open::that_detached(&url);

    let code = listener
        .wait(&state, SIGN_IN_TIMEOUT)
        .await
        .map_err(|e| e.to_string())?;
    let tokens = OAuthClient::new(credentials.clone())
        .map_err(|e| e.to_string())?
        .exchange_code(code.code(), &redirect)
        .await
        .map_err(|e| e.to_string())?;
    let accounts = client
        .accounts(tokens.access_token.expose_secret())
        .await
        .map_err(|e| e.to_string())?
        .ctid_trader_account;
    if accounts.is_empty() {
        return Err(
            "no trading account is covered by this token: pick one on the cTrader \
                    consent page, then sign in again"
                .to_owned(),
        );
    }
    Ok(SignedIn {
        credentials,
        environment,
        client,
        tokens,
        accounts,
    })
}

pub async fn authorize(client: Client, account_id: i64, tokens: TokenSet) -> Result<(), String> {
    client
        .account(account_id)
        .authorize(tokens.access_token.expose_secret())
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(service: &str) -> ProfileConfig {
        ProfileConfig {
            id: ProfileId::new_random(),
            display_name: service.to_owned(),
            service: service.to_owned(),
            client_id: Some("client".into()),
            callback_port: Some(CALLBACK_PORT),
            account_id: Some(7),
        }
    }

    #[test]
    fn service_tags_carry_the_environment_both_ways() {
        for environment in [Environment::Live, Environment::Demo] {
            assert_eq!(environment_of(service_tag(environment)), Some(environment));
        }
        assert_eq!(environment_of("ctrader-openapi-staging"), None);
        assert_eq!(environment_of("live"), None);
    }

    #[test]
    fn the_active_open_api_profile_is_reopened_first() {
        let other = profile("mcp-server");
        let demo = profile("ctrader-openapi-demo");
        let live = profile("ctrader-openapi-live");
        let all = vec![other.clone(), demo.clone(), live.clone()];
        let id = |p: Option<&ProfileConfig>| p.map(|p| p.id.clone());
        assert_eq!(id(saved_profile(Some(&live), &all)), Some(live.id.clone()));
        assert_eq!(id(saved_profile(Some(&other), &all)), Some(demo.id.clone()));
        assert_eq!(id(saved_profile(None, &all)), Some(demo.id.clone()));
        assert!(saved_profile(None, &[other]).is_none());
    }

    #[test]
    fn accounts_are_named_by_broker_kind_and_login() {
        let mut account: TraderAccount = serde_json::from_value(serde_json::json!({
            "ctidTraderAccountId": 99,
            "isLive": true,
            "traderLogin": 1234567,
            "brokerTitleShort": "Pepperstone",
        }))
        .unwrap();
        assert_eq!(account_label(&account), "Pepperstone Live - 1234567");
        account.is_live = None;
        account.trader_login = None;
        account.broker_title_short = None;
        assert_eq!(account_label(&account), "cTrader Demo - 99");
    }

    #[test]
    fn documents_are_kept_per_environment_and_account() {
        assert_eq!(document_scope(Environment::Live, 42), "live-42");
        assert_eq!(document_scope(Environment::Demo, 42), "demo-42");
    }
}
