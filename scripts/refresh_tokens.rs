//! Trades the refresh token for a fresh access/refresh pair against the real token endpoint, so a
//! `.env` set up by `scripts/sign_in.rs` does not go stale between live test runs.
//!
//! The old refresh token stops working the moment this succeeds (see [`OAuthClient::refresh`]'s
//! docs), so this always prints the new pair, and with `--write-env` saves it before anything
//! else could go wrong and lose it.
//!
//! ```sh
//! cargo run --bin refresh-tokens -- --write-env
//! ```

use std::path::PathBuf;

use clap::Parser;
use secrecy::ExposeSecret;
use wyck::openapi::auth::OAuthClient;
use wyck::openapi::config::ClientCredentials;

mod env_file;
mod tracing_init;

/// Refreshes the Open API access/refresh token pair.
#[derive(Parser)]
#[command(
    version,
    about = "Trades a cTrader Open API refresh token for a fresh access/refresh pair",
    long_about = None
)]
struct Args {
    /// The registered application's client id.
    #[arg(long, env = "WYCK_OPENAPI_CLIENT_ID")]
    client_id: String,

    /// The registered application's client secret.
    #[arg(long, env = "WYCK_OPENAPI_CLIENT_SECRET")]
    client_secret: String,

    /// The refresh token to trade in. Usually the one `sign-in` last wrote to `.env`.
    #[arg(long, env = "WYCK_OPENAPI_REFRESH_TOKEN")]
    refresh_token: String,

    /// Write the new pair into this `.env` file (existing keys replaced in place, everything
    /// else untouched). Pass with no path to mean `.env`, or omit the flag to only print them.
    #[arg(long, num_args = 0..=1, default_missing_value = ".env")]
    write_env: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_init::init();
    let args = Args::parse();

    let credentials = ClientCredentials::new(args.client_id, args.client_secret);
    let oauth = OAuthClient::new(credentials)?;

    println!("refreshing...");
    let tokens = oauth.refresh(&args.refresh_token).await?;
    println!(
        "got a new pair{}. the old refresh token no longer works.",
        tokens
            .expires_in
            .map(|d| format!(
                ", the access token is valid for about {} days",
                d.as_secs() / 86_400
            ))
            .unwrap_or_default()
    );
    println!("access token:  {}", tokens.access_token.expose_secret());
    println!("refresh token: {}", tokens.refresh_token.expose_secret());

    let Some(write_env) = args.write_env else {
        println!("\n(not writing a .env file: pass --write-env to save these into one)");
        return Ok(());
    };
    env_file::update(
        &write_env,
        &[
            (
                "WYCK_OPENAPI_ACCESS_TOKEN",
                tokens.access_token.expose_secret(),
            ),
            (
                "WYCK_OPENAPI_REFRESH_TOKEN",
                tokens.refresh_token.expose_secret(),
            ),
        ],
    )?;
    println!("wrote the new pair to {write_env:?}.");
    Ok(())
}
