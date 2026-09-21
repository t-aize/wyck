//! The complete OAuth 2 sign in flow: open the consent page, catch the redirect on localhost, and
//! trade the code for a token pair.
//!
//! ```sh
//! WYCK_OPENAPI_CLIENT_ID=... WYCK_OPENAPI_CLIENT_SECRET=... cargo run -p ctrader-openapi --example sign_in
//! ```
//!
//! The redirect URI printed below must be registered for the application exactly as shown (port
//! included). Nothing is written anywhere: the tokens are only printed, redacted, at the end. A
//! real program stores them with a `session::TokenStore` on top of the OS keyring, never in a file.

use std::time::Duration;

use ctrader_openapi::ClientCredentials;
use ctrader_openapi::auth::{CallbackListener, OAuthClient, Scope, authorization_url, new_state};
use secrecy::ExposeSecret;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this example"))
}

#[tokio::main]
async fn main() -> ctrader_openapi::Result<()> {
    let credentials = ClientCredentials::new(
        env("WYCK_OPENAPI_CLIENT_ID"),
        env("WYCK_OPENAPI_CLIENT_SECRET"),
    );

    let listener = CallbackListener::bind(8765).await?;
    let redirect = listener.redirect_uri();
    let state = new_state();
    let url = authorization_url(&credentials.client_id, &redirect, Scope::Accounts, &state);

    println!("register this redirect URI for the application, if not done already: {redirect}");
    println!("open this URL in a browser and grant access:\n\n  {url}\n");
    println!("waiting up to 5 minutes for the redirect...");

    let code = listener.wait(&state, Duration::from_secs(300)).await?;
    if !code.state_echoed() {
        println!("note: the redirect carried no state to check; proceeding anyway");
    }

    let oauth = OAuthClient::new(credentials)?;
    let tokens = oauth.exchange_code(code.code(), &redirect).await?;

    println!("signed in.");
    println!(
        "access token:  {}...",
        &tokens.access_token.expose_secret()[..8.min(tokens.access_token.expose_secret().len())]
    );
    println!("refresh token: (kept secret; store it, never print it in a real program)");
    println!("expires in:    {:?}", tokens.expires_in);
    Ok(())
}
