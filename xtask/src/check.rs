//! `cargo xtask check`: `cargo xtask ci` and the rules of the workspace, plus with `--full` the
//! tools that need installing.

use std::process::Command;

use anyhow::Result;

use crate::run::{Action, Step, cargo, cargo_subcommand_installed, command, installed, run_all};

pub(crate) fn run(full: bool) -> Result<()> {
    // The steps of `cargo xtask ci`, then the rules of the workspace; cargo-deny is skipped here
    // when it is missing, where `ci` fails.
    let mut steps = crate::ci::steps(false);
    steps.push(Step {
        name: "dependency rules",
        action: Action::Function(crate::deps::run),
    });
    steps.push(Step {
        name: "documentation files",
        action: Action::Function(crate::docs::run),
    });
    if full {
        steps.extend(external_tools());
    }
    run_all(steps)
}

/// The checks that need a tool installed. A missing tool is skipped with the command that
/// installs it: CI always has them.
fn external_tools() -> Vec<Step> {
    vec![
        optional(
            "cargo-machete",
            cargo_subcommand_installed("machete"),
            "cargo install --locked cargo-machete",
            cargo_command(&["machete"]),
        ),
        optional(
            "typos",
            installed("typos", "--version"),
            "cargo install --locked typos-cli",
            command("typos"),
        ),
        optional(
            "taplo (toml format)",
            installed("taplo", "--version"),
            "cargo install --locked taplo-cli",
            tool("taplo", &["fmt", "--check"]),
        ),
        optional(
            "zizmor (workflows)",
            installed("zizmor", "--version"),
            "cargo install --locked zizmor",
            tool("zizmor", &["--offline", ".github"]),
        ),
        optional(
            "actionlint (workflows)",
            installed("actionlint", "-version"),
            "https://github.com/rhysd/actionlint/releases",
            command("actionlint"),
        ),
    ]
}

fn cargo_command(args: &[&str]) -> Command {
    let mut command = cargo();
    command.args(args);
    command
}

fn tool(program: &str, args: &[&str]) -> Command {
    let mut command = command(program);
    command.args(args);
    command
}

fn optional(name: &'static str, available: bool, install: &str, command: Command) -> Step {
    let action = if available {
        Action::Command(command)
    } else {
        Action::Skip(format!("not installed: {install}"))
    };
    Step { name, action }
}
