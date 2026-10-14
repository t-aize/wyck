//! Running the steps of a command and telling how long each one took.

use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

/// What a step does.
pub(crate) enum Action {
    /// Runs a program and passes if it exits with success.
    Command(Command),
    /// Runs code of this tool.
    Function(fn() -> Result<()>),
    /// Does nothing, and says why.
    Skip(String),
}

pub(crate) struct Step {
    pub(crate) name: &'static str,
    pub(crate) action: Action,
}

/// A command to run for the developer, without the variables `cargo run` sets for this tool's own
/// package. Some tools (cargo-machete) take `CARGO_MANIFEST_DIR` for their input and would look at
/// the folder of xtask instead of the workspace.
pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("CARGO_PKG_")
            || name.starts_with("CARGO_BIN_")
            || matches!(
                name.as_ref(),
                "CARGO_MANIFEST_DIR" | "CARGO_CRATE_NAME" | "CARGO_PRIMARY_PACKAGE"
            )
        {
            command.env_remove(name.as_ref());
        }
    }
    command
}

/// `cargo`, the one that started this tool when there is one.
pub(crate) fn cargo() -> Command {
    command(std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")))
}

/// Whether `program --version` runs, i.e. the tool is installed.
pub(crate) fn installed(program: &str, version_flag: &str) -> bool {
    command(program)
        .arg(version_flag)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Whether `cargo <subcommand> --version` runs.
pub(crate) fn cargo_subcommand_installed(subcommand: &str) -> bool {
    cargo()
        .args([subcommand, "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

enum Outcome {
    Passed,
    Skipped(String),
    Failed,
}

/// Runs the steps in order and stops at the first one that fails. Prints a table of the time
/// each step took at the end.
pub(crate) fn run_all(steps: Vec<Step>) -> Result<()> {
    let mut report: Vec<(&'static str, Duration, Outcome)> = Vec::new();
    let mut failed = None;
    for step in steps {
        println!("==> {}", step.name);
        let started = Instant::now();
        let outcome = match step.action {
            Action::Skip(why) => Outcome::Skipped(why),
            Action::Function(function) => match function() {
                Ok(()) => Outcome::Passed,
                Err(error) => {
                    eprintln!("{error:#}");
                    Outcome::Failed
                }
            },
            Action::Command(mut command) => match command.status() {
                Ok(status) if status.success() => Outcome::Passed,
                Ok(_) => Outcome::Failed,
                Err(error) => {
                    eprintln!("could not start {:?}: {error}", command.get_program());
                    Outcome::Failed
                }
            },
        };
        let stop = matches!(outcome, Outcome::Failed);
        report.push((step.name, started.elapsed(), outcome));
        if stop {
            failed = Some(step.name);
            break;
        }
    }
    print_report(&report);
    match failed {
        Some(name) => bail!("`{name}` failed"),
        None => Ok(()),
    }
}

fn print_report(report: &[(&'static str, Duration, Outcome)]) {
    println!("\nSummary");
    for (name, took, outcome) in report {
        let state = match outcome {
            Outcome::Passed => "ok".to_owned(),
            Outcome::Skipped(why) => format!("skipped ({why})"),
            Outcome::Failed => "FAILED".to_owned(),
        };
        println!("  {name:<28} {:>7.1}s  {state}", took.as_secs_f32());
    }
}
