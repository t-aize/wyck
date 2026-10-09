//! Looks over a Wyck config and says what is wrong. Changes nothing.
//!
//! ```text
//! cargo run --example config_doctor
//! cargo run --example config_doctor -- --dir ./wyck-data
//! WYCK_PASSPHRASE=... cargo run --example config_doctor -- --dir ./wyck-data
//! ```
//!
//! * With no argument it checks the standard config of the operating system, credentials in the
//!   OS keyring.
//! * `--dir <folder>` checks a portable install (see `WyckConfig::builder().portable(..)`).
//! * With `WYCK_PASSPHRASE` set, credentials are read from encrypted files under the data folder
//!   instead of the keyring (the setup of a machine with no keyring).
//!
//! The exit code is 0 when nothing does not work, 1 when the check-up found an error, and 2 when
//! the config could not even be opened.

use std::process::ExitCode;

use secrecy::SecretString;
use wyck::infra::storage::{AppPaths, Severity, WyckConfig};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" => dir = args.next(),
            "-h" | "--help" => {
                println!(
                    "usage: config_doctor [--dir <folder>]   (WYCK_PASSPHRASE selects encrypted files)"
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument `{other}` (try --help)");
                return ExitCode::from(2);
            }
        }
    }

    let mut builder = WyckConfig::builder();
    if let Some(dir) = &dir {
        builder = builder.portable(dir);
    }
    if let Ok(passphrase) = std::env::var("WYCK_PASSPHRASE") {
        builder = builder.encrypted_file(SecretString::from(passphrase));
    }
    let config = match builder.build() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("could not open the config: {error}");
            return ExitCode::from(2);
        }
    };

    let paths: &AppPaths = config.paths();
    println!("config folder : {}", paths.config_dir().display());
    println!("data folder   : {}", paths.data_dir().display());
    println!("profiles      : {}", config.profiles().len());
    for profile in config.profiles() {
        let active = config.active_profile().is_some_and(|a| a.id == profile.id);
        println!(
            "  {} {} ({})",
            if active { "*" } else { " " },
            profile.display_name,
            profile.service
        );
    }
    println!();
    let report = config.diagnose();
    println!("{report}");

    if report.worst() == Some(Severity::Error) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
