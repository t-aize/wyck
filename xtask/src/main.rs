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
mod ci;
mod deps;
mod docs;
mod release;
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
    /// What `ci` runs (cargo-deny is skipped when missing), plus the dependency rules.
    Check {
        /// Also run the tools that need installing (cargo-deny, cargo-machete, typos, taplo,
        /// zizmor, actionlint). The ones that are missing are skipped.
        #[arg(long)]
        full: bool,
    },
    /// What CI runs: format, clippy, tests, cargo-deny and rustdoc, all strict, stopping at the
    /// first failure. Needs cargo-deny installed.
    Ci,
    /// Check that every crate takes its dependencies, metadata and lints from the root
    /// Cargo.toml, and only depends on the crates its layer allows.
    Deps,
    /// Check that every crate has its AGENTS.md and CLAUDE.md, that their `Depends on` and
    /// `Used by` lines agree with Cargo.toml, and that the Markdown links point somewhere.
    DocsCheck,
    /// The steps of the release pipeline that are written in Rust (see release.yml).
    Release {
        #[command(subcommand)]
        step: release::Step,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check { full } => check::run(full),
        Command::Ci => ci::run(),
        Command::Deps => deps::run(),
        Command::DocsCheck => docs::run(),
        Command::Release { step } => release::run(step),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
