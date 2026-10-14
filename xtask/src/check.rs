//! `cargo xtask check`: what CI runs, in the order of how fast each step fails.

use std::process::Command;

use anyhow::Result;

use crate::run::{Action, Step, cargo, cargo_subcommand_installed, command, installed, run_all};

pub(crate) fn run(full: bool) -> Result<()> {
    let mut steps = vec![
        cargo_step("format", &["fmt", "--all", "--check"]),
        cargo_step(
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
    ];
    // `cargo test` and not nextest: nextest starts a process for each test, which costs more than
    // the tests themselves on Windows (22 s against 3 s for the same 1189 tests).
    steps.push(cargo_step("tests", &["test", "--workspace", "--locked"]));
    let mut docs = cargo();
    docs.args(["doc", "--workspace", "--no-deps", "--locked"])
        .env("RUSTDOCFLAGS", "-D warnings");
    steps.push(Step {
        name: "rustdoc",
        action: Action::Command(docs),
    });
    steps.push(Step {
        name: "dependency rules",
        action: Action::Function(crate::deps::run),
    });
    if full {
        steps.extend(external_tools());
    }
    run_all(steps)
}

fn cargo_step(name: &'static str, args: &[&str]) -> Step {
    let mut command = cargo();
    command.args(args);
    Step {
        name,
        action: Action::Command(command),
    }
}

/// The checks that need a tool installed. A missing tool is skipped with the command that
/// installs it: CI always has them.
fn external_tools() -> Vec<Step> {
    vec![
        optional(
            "cargo-deny",
            cargo_subcommand_installed("deny"),
            "cargo install --locked cargo-deny",
            cargo_command(&["deny", "check", "--hide-inclusion-graph"]),
        ),
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
