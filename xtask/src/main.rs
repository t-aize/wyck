//! The developer commands of the workspace, run as `cargo xtask <command>`.
//!
//! They are plain Rust so that they work the same on Windows, macOS and Linux, with no `make`,
//! shell or Python to install. CI runs the same commands, so what passes here passes there.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command line tool talks to its terminal"
)]

mod check;
mod deps;
mod run;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The developer commands of the Wyck workspace.
#[derive(Parser)]
#[command(
    name = "cargo xtask",
    bin_name = "cargo xtask",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Everything CI runs: format, clippy, tests, docs and the dependency rules.
    Check {
        /// Also run the tools that need installing (cargo-deny, cargo-machete, typos, taplo,
        /// zizmor, actionlint). The ones that are missing are skipped.
        #[arg(long)]
        full: bool,
    },
    /// Check that every crate takes its dependencies, metadata and lints from the root
    /// Cargo.toml, and only depends on the crates its layer allows.
    Deps,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check { full } => check::run(full),
        Command::Deps => deps::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
