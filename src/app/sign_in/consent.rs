//! The network steps of a sign-in: the consent page, the redirect, the code exchange and the
//! list of accounts.

use std::time::Duration;

use gpui::AsyncApp;
use gpui::WeakEntity;
use secrecy::ExposeSecret;

use super::rules::{SIGN_IN_SCOPE, account_label};
use super::{AccountChoice, CALLBACK_PORT, Failure, Pending, SignIn};
use crate::infra::ctrader::auth::{CallbackListener, OAuthClient, authorization_url, new_state};
use crate::infra::ctrader::config::ClientCredentials;
use crate::infra::ctrader::{ClientBuilder, Environment, TraderAccount};
use crate::infra::platform::runtime;

/// How long to wait for the user to allow access on cTrader's page.
const CONSENT_TIMEOUT: Duration = Duration::from_secs(300);

/// Whether the attempt is still the current one.
fn current(this: &WeakEntity<SignIn>, cx: &mut AsyncApp, attempt: u64) -> bool {
    this.update(cx, |this, _| this.attempt == attempt)
        .unwrap_or(false)
}

/// Shows a failure, if the attempt is still the current one.
fn fail(this: &WeakEntity<SignIn>, cx: &mut AsyncApp, attempt: u64, failure: Failure) {
    let _ = this.update(cx, |this, cx| {
        if this.attempt == attempt {
            this.state.fail(failure);
            this.abort = None;
            cx.notify();
        }
    });
}

const STOPPED: &str = "the background runtime stopped";

/// Verifies the application, opens the consent page, waits for the redirect, exchanges the code
/// and lists the accounts.
pub(super) async fn consent(
    this: WeakEntity<SignIn>,
    cx: &mut AsyncApp,
    attempt: u64,
    credentials: ClientCredentials,
    environment: Environment,
    remember: bool,
) {
    let listener = match runtime::spawn(CallbackListener::bind(CALLBACK_PORT)).await {
        Ok(Ok(listener)) => listener,
        Ok(Err(_)) => return fail(&this, cx, attempt, Failure::PortBusy(CALLBACK_PORT)),
        Err(_) => return fail(&this, cx, attempt, Failure::Other(STOPPED.into())),
    };

    let checked = credentials.clone();
    let client = match runtime::spawn(async move {
        ClientBuilder::new(environment)
            .credentials(checked)
            .connect()
            .await
    })
    .await
    {
        Ok(Ok(client)) => client,
        Ok(Err(error)) => return fail(&this, cx, attempt, Failure::from_connect(&error)),
        Err(_) => return fail(&this, cx, attempt, Failure::Other(STOPPED.into())),
    };

    let redirect = listener.redirect_uri();
    let oauth_state = new_state();
    let url = match authorization_url(
        &credentials.client_id,
        &redirect,
        SIGN_IN_SCOPE,
        &oauth_state,
    ) {
        Ok(url) => url,
        Err(error) => return fail(&this, cx, attempt, Failure::from_connect(&error)),
    };
    let opened = this
        .update(cx, |this, cx| {
            if this.attempt != attempt {
                return false;
            }
            cx.open_url(&url);
            this.state.browser_opened(url.clone());
            cx.notify();
            true
        })
        .unwrap_or(false);
    if !opened {
        return;
    }

    let waiting = runtime::spawn(async move { listener.wait(&oauth_state, CONSENT_TIMEOUT).await });
    let abort = waiting.abort_handle();
    let _ = this.update(cx, |this, _| {
        if this.attempt == attempt {
            this.abort = Some(abort);
        }
    });
    let code = match waiting.await {
        Ok(Ok(code)) => code,
        Ok(Err(error)) => return fail(&this, cx, attempt, Failure::from_consent(&error)),
        // Aborted by a cancel: the form is already back.
        Err(error) if error.is_cancelled() => return,
        Err(_) => return fail(&this, cx, attempt, Failure::Other(STOPPED.into())),
    };
    if !current(&this, cx, attempt) {
        return;
    }

    let oauth = match OAuthClient::new(credentials.clone()) {
        Ok(oauth) => oauth,
        Err(error) => return fail(&this, cx, attempt, Failure::from_consent(&error)),
    };
    let code_value = code.code().to_owned();
    let tokens = match runtime::spawn(
        async move { oauth.exchange_code(&code_value, &redirect).await },
    )
    .await
    {
        Ok(Ok(tokens)) => tokens,
        Ok(Err(error)) => return fail(&this, cx, attempt, Failure::from_consent(&error)),
        Err(_) => return fail(&this, cx, attempt, Failure::Other(STOPPED.into())),
    };

    let access = tokens.access_token.expose_secret().to_owned();
    let lister = client.clone();
    let listed = match runtime::spawn(async move { lister.accounts(&access).await }).await {
        Ok(Ok(res)) => res.ctid_trader_account,
        Ok(Err(error)) => return fail(&this, cx, attempt, Failure::from_consent(&error)),
        Err(_) => return fail(&this, cx, attempt, Failure::Other(STOPPED.into())),
    };

    let live = environment == Environment::Live;
    let accounts: Vec<TraderAccount> = listed
        .into_iter()
        .filter(|account| account.is_live.unwrap_or(false) == live)
        .collect();
    if accounts.is_empty() {
        return fail(&this, cx, attempt, Failure::NoAccount { live });
    }
    let choices: Vec<AccountChoice> = accounts
        .iter()
        .map(|account| AccountChoice {
            id: account.ctid_trader_account_id,
            label: account_label(account),
            is_live: account.is_live.unwrap_or(false),
        })
        .collect();
    let _ = this.update(cx, |this, cx| {
        if this.attempt != attempt {
            return;
        }
        this.abort = None;
        let only = (accounts.len() == 1).then(|| accounts[0].clone());
        this.pending = Some(Pending {
            credentials,
            environment,
            remember,
            client,
            tokens,
            accounts,
        });
        this.state.accounts_listed(choices);
        match only {
            Some(account) => this.authorize(account, cx),
            None => cx.notify(),
        }
    });
}
