//! Signs a user in to the Open API from the terminal, and shows what the tokens can reach.
//!
//! It runs the whole flow of the `auth` and `callback` modules:
//!
//! 1. starts the loopback listener on the redirect port,
//! 2. opens the consent page in the browser (and prints the address, in case it does not open),
//! 3. waits for the redirect, and trades the code for tokens,
//! 4. connects, identifies the application, and lists the trading accounts the token covers.
//!
//! ```text
//! WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=... cargo run -p ctrader-openapi --example sign_in
//! ```
//!
//! Optional variables: `WYCK_OPENAPI_PORT` (default 8765; the redirect URI
//! `http://localhost:<port>` must be registered for the application), `WYCK_OPENAPI_ENV`
//! (`demo`, the default, or `live`) and `WYCK_OPENAPI_SCOPE` (`accounts`, the default, or
//! `trading`).
//!
//! It **prints the tokens** so they can be given to the live test. They are secrets: do not paste
//! them anywhere public, and do not save them in a file. The access token lasts about 30 days.

use std::process::Command;
use std::time::Duration;

use ctrader_openapi::Client;
use ctrader_openapi::auth::{OAuthClient, Scope, authorization_url, new_state};
use ctrader_openapi::callback::CallbackListener;
use ctrader_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use secrecy::ExposeSecret;

fn need(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        eprintln!("Set {name} first. See the top of examples/sign_in.rs.");
        std::process::exit(2);
    })
}

/// Opens `url` in the default browser. Failure is fine: the address is printed as well.
fn open_browser(url: &str) {
    let result = if cfg!(target_os = "windows") {
        // `start` would treat the `&` of the query as a command separator; this does not.
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    if result.is_err() {
        println!("(the browser could not be opened, use the address above)");
    }
}

#[tokio::main]
async fn main() -> ctrader_openapi::Result<()> {
    let credentials = ClientCredentials::new(
        need("WYCK_OPENAPI_CLIENT_ID"),
        need("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let port: u16 = std::env::var("WYCK_OPENAPI_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8765);
    let environment = match std::env::var("WYCK_OPENAPI_ENV").as_deref() {
        Ok("live") => Environment::Live,
        _ => Environment::Demo,
    };
    let scope = match std::env::var("WYCK_OPENAPI_SCOPE").as_deref() {
        Ok("trading") => Scope::Trading,
        _ => Scope::Accounts,
    };

    // 1 and 2: listen, then send the user to the consent page.
    let listener = CallbackListener::bind(port).await?;
    let redirect = listener.redirect_uri();
    let state = new_state();
    let url = authorization_url(&credentials.client_id, &redirect, scope, &state);
    println!("Redirect URI: {redirect}  (it must be registered for the application)");
    println!("Consent page:\n{url}\n");
    open_browser(&url);
    println!("Waiting for you to sign in and grant access (5 minutes)...");

    // 3: the redirect, then the tokens. The code lives one minute, so go straight on.
    let code = listener.wait(&state, Duration::from_secs(300)).await?;
    println!(
        "Code received (state {}).",
        if code.state_echoed() {
            "echoed and matched"
        } else {
            "NOT echoed by the server: note this for the TODO"
        }
    );
    let tokens = OAuthClient::new(credentials.clone())?
        .exchange_code(code.code(), &redirect)
        .await?;
    println!(
        "Tokens received, the access token lasts {:?}.",
        tokens.expires_in
    );

    // 4: what can they reach?
    let client = Client::connect(&ConnectionConfig::new(environment)).await?;
    client.authenticate_application(&credentials).await?;
    println!("Proxy version: {:?}", client.version().await.ok());
    let accounts = client.accounts(tokens.access_token.expose_secret()).await?;
    println!(
        "Permission scope: {:?} (0 view, 1 trade)",
        accounts.permission_scope
    );
    for account in &accounts.ctid_trader_account {
        println!(
            "  account {}  {}  login {:?}  broker {:?}",
            account.ctid_trader_account_id,
            match account.is_live {
                Some(true) => "LIVE",
                Some(false) => "demo",
                None => "?",
            },
            account.trader_login,
            account.broker_title_short
        );
    }
    client.close().await;

    println!("\nFor the live test (PowerShell), in this window only:");
    println!(
        "  $env:WYCK_OPENAPI_ACCESS_TOKEN = \"{}\"",
        tokens.access_token.expose_secret()
    );
    println!(
        "\nKeep the refresh token for later renewals; do not share either token:\n  {}",
        tokens.refresh_token.expose_secret()
    );
    Ok(())
}
