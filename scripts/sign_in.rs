//! Signs a user in with the cTrader Open API's OAuth flow, then lists every trading account the
//! resulting token covers, and optionally writes everything a `.env` file needs straight into it.
//!
//! This is the one-time (or once-every-30-days) step that turns a registered application into
//! working credentials for the live tests in `tests/live.rs` and for the other scripts here. See
//! `scripts/README.md` for the full walkthrough.
//!
//! ```sh
//! cargo run --bin sign-in -- --write-env
//! ```

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use secrecy::ExposeSecret;
use wyck::openapi::ClientBuilder;
use wyck::openapi::auth::{CallbackListener, OAuthClient, Scope, authorization_url, new_state};
use wyck::openapi::config::{ClientCredentials, Environment};

mod env_file;

/// Signs in through the browser and lists the accounts the token covers.
#[derive(Parser)]
#[command(
    version,
    about = "Signs in to the cTrader Open API through the browser and lists the accounts the resulting token covers",
    long_about = None
)]
struct Args {
    /// The registered application's client id.
    #[arg(long, env = "WYCK_OPENAPI_CLIENT_ID")]
    client_id: String,

    /// The registered application's client secret. Treat it like a password.
    #[arg(long, env = "WYCK_OPENAPI_CLIENT_SECRET")]
    client_secret: String,

    /// Which server to connect to once signed in, only to list the accounts the token covers
    /// (`GetAccountsByAccessToken` answers the same regardless of which one you ask).
    #[arg(long, env = "WYCK_OPENAPI_ENVIRONMENT", default_value = "demo")]
    environment: EnvironmentArg,

    /// Port for the local redirect listener. Must match a redirect URI registered for the
    /// application exactly (`http://localhost:<port>`), port included.
    #[arg(long, env = "WYCK_OPENAPI_CALLBACK_PORT", default_value_t = 8765)]
    callback_port: u16,

    /// `accounts` is read only and enough for every live test but the trading one; ask for
    /// `trading` only if you intend to run `place_and_close_a_minimal_market_order_on_a_demo_account`.
    #[arg(long, default_value = "accounts")]
    scope: ScopeArg,

    /// How long to wait for the browser redirect before giving up.
    #[arg(long, default_value_t = 300)]
    timeout_secs: u64,

    /// Skip trying to open the consent page in a browser automatically; just print the URL.
    #[arg(long)]
    no_open: bool,

    /// If the token covers more than one account in the chosen environment, which one to keep
    /// for `--write-env`. Omit it to be shown the list and asked.
    #[arg(long)]
    account_id: Option<i64>,

    /// Write the obtained credentials into this `.env` file (existing keys are replaced in
    /// place; anything else in the file, and its comments, are left untouched). Pass with no
    /// path to mean `.env`, or omit the flag entirely to only print the values.
    #[arg(long, num_args = 0..=1, default_missing_value = ".env")]
    write_env: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum EnvironmentArg {
    Demo,
    Live,
}

impl From<EnvironmentArg> for Environment {
    fn from(value: EnvironmentArg) -> Self {
        match value {
            EnvironmentArg::Demo => Self::Demo,
            EnvironmentArg::Live => Self::Live,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ScopeArg {
    Accounts,
    Trading,
}

impl From<ScopeArg> for Scope {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::Accounts => Self::Accounts,
            ScopeArg::Trading => Self::Trading,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `.env` is optional here: every value can also come from real flags or the environment
    // (clap's `env` feature reads both the same way), so a missing file is not an error.
    let _ = dotenvy::dotenv();
    let args = Args::parse();

    let credentials = ClientCredentials::new(args.client_id.clone(), args.client_secret.clone());
    let scope: Scope = args.scope.into();
    let environment: Environment = args.environment.into();

    println!(
        "starting the local redirect listener on port {}...",
        args.callback_port
    );
    let listener = CallbackListener::bind(args.callback_port).await?;
    let redirect_uri = listener.redirect_uri();
    let state = new_state();
    let url = authorization_url(&args.client_id, &redirect_uri, scope, &state);

    println!("\nopen this page and sign in, picking the demo account(s) to grant access to:\n");
    println!("  {url}\n");
    if !args.no_open {
        try_open_in_browser(&url);
    }
    println!("waiting up to {}s for the redirect...", args.timeout_secs);

    let code = listener
        .wait(&state, Duration::from_secs(args.timeout_secs))
        .await?;
    if !code.state_echoed() {
        eprintln!(
            "warning: the redirect carried no `state` to check; accepted the code anyway (see \
             AuthorizationCode::state_echoed's docs)"
        );
    }

    println!("exchanging the code for tokens...");
    let oauth = OAuthClient::new(credentials.clone())?;
    let tokens = oauth.exchange_code(code.code(), &redirect_uri).await?;
    println!(
        "got an access token{} and a refresh token.",
        tokens
            .expires_in
            .map(|d| format!(" valid for about {} days", d.as_secs() / 86_400))
            .unwrap_or_default()
    );

    println!("\nconnecting to {environment:?} to list the accounts this token covers...");
    let client = ClientBuilder::new(environment)
        .credentials(credentials)
        .connect()
        .await?;
    let access = tokens.access_token.expose_secret();
    let accounts = client.accounts(access).await?;
    client.close().await;

    if accounts.ctid_trader_account.is_empty() {
        eprintln!(
            "no account is covered by this token at all: pick at least one on the consent page."
        );
        return Ok(());
    }

    println!(
        "\n{:<14} {:<6} {:<10} broker",
        "account id", "kind", "login"
    );
    for account in &accounts.ctid_trader_account {
        println!(
            "{:<14} {:<6} {:<10} {}",
            account.ctid_trader_account_id,
            match account.is_live {
                Some(true) => "live",
                Some(false) => "demo",
                None => "?",
            },
            account
                .trader_login
                .map(|l| l.to_string())
                .unwrap_or_default(),
            account.broker_title_short.as_deref().unwrap_or(""),
        );
    }

    let Some(write_env) = args.write_env else {
        println!(
            "\n(not writing a .env file: pass --write-env to save these into one, or set the \
             three WYCK_OPENAPI_* variables above by hand)"
        );
        return Ok(());
    };

    let chosen = pick_account(&accounts.ctid_trader_account, args.account_id)?;
    println!(
        "\nusing account {} for {}",
        chosen.ctid_trader_account_id,
        write_env.display()
    );

    env_file::update(
        &write_env,
        &[
            ("WYCK_OPENAPI_ACCESS_TOKEN", access),
            (
                "WYCK_OPENAPI_REFRESH_TOKEN",
                tokens.refresh_token.expose_secret(),
            ),
            (
                "WYCK_OPENAPI_ACCOUNT_ID",
                &chosen.ctid_trader_account_id.to_string(),
            ),
        ],
    )?;
    println!("wrote the access token, refresh token and account id to {write_env:?}.");
    println!(
        "these are secrets: {write_env:?} must never be committed (it already matches \
         .gitignore's `.env*` pattern, but double check before pushing anything)."
    );
    Ok(())
}

/// Picks the account to keep for `--write-env`: the one asked for by id, the only one there is,
/// or a prompt on stdin when there is more than one and none was named.
fn pick_account(
    accounts: &[wyck::openapi::transport::messages::TraderAccount],
    wanted: Option<i64>,
) -> Result<&wyck::openapi::transport::messages::TraderAccount, Box<dyn std::error::Error>> {
    if let Some(id) = wanted {
        return accounts
            .iter()
            .find(|a| a.ctid_trader_account_id == id)
            .ok_or_else(|| format!("account {id} is not covered by this token").into());
    }
    if accounts.len() == 1 {
        return Ok(&accounts[0]);
    }
    println!("\nmore than one account is covered by this token; which one goes in the .env?");
    print!("account id: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let id: i64 = line.trim().parse()?;
    accounts
        .iter()
        .find(|a| a.ctid_trader_account_id == id)
        .ok_or_else(|| format!("account {id} is not covered by this token").into())
}

/// Best-effort browser launch: a failure here is never fatal, the URL is already printed.
fn try_open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let command = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "linux")]
    let command = std::process::Command::new("xdg-open").arg(url).status();
    #[cfg(target_os = "windows")]
    let command = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let command: std::io::Result<std::process::ExitStatus> = Err(std::io::Error::other(
        "no known way to open a browser on this platform",
    ));

    if let Err(error) = command {
        eprintln!("(could not open a browser automatically: {error}; use the URL above)");
    }
}
