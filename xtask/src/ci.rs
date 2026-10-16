//! `cargo xtask ci`: the checks a change must pass, in the order of how fast each one fails.
//!
//! The CI workflow calls this command and nothing else for these checks, so what passes on a
//! developer's machine is what passes there. The steps stop at the first failure.

use std::process::Command;

use anyhow::{Result, bail};

use crate::run::{Action, Step, cargo, cargo_subcommand_installed, run_all};

/// How to get the one tool of these steps that is not part of the Rust toolchain.
const DENY_INSTALL: &str = "cargo install --locked cargo-deny";

pub(crate) fn run() -> Result<()> {
    if !cargo_subcommand_installed("deny") {
        bail!("cargo-deny is not installed: {DENY_INSTALL}");
    }
    run_all(steps(true))
}

/// The steps, in order: format, clippy, tests, cargo-deny, rustdoc.
///
/// `deny_required` is whether a missing cargo-deny fails the run. `cargo xtask check` passes
/// `false` and skips the step with the command that installs the tool; CI always has it.
pub(crate) fn steps(deny_required: bool) -> Vec<Step> {
    let deny = if deny_required || cargo_subcommand_installed("deny") {
        Action::Command(cargo_command(&["deny", "check"]))
    } else {
        Action::Skip(format!("not installed: {DENY_INSTALL}"))
    };
    let mut docs = cargo_command(&["doc", "--workspace", "--no-deps", "--locked"]);
    docs.env("RUSTDOCFLAGS", "-D warnings");
    vec![
        step("format", cargo_command(&["fmt", "--all", "--check"])),
        step(
            "clippy",
            cargo_command(&[
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ]),
        ),
        // `cargo test` and not nextest: nextest starts a process for each test, which costs more
        // than the tests themselves on Windows (22 s against 3 s for the same 1189 tests).
        step(
            "tests",
            cargo_command(&["test", "--workspace", "--all-features", "--locked"]),
        ),
        Step {
            name: "cargo-deny",
            action: deny,
        },
        step("rustdoc", docs),
    ]
}

fn step(name: &'static str, command: Command) -> Step {
    Step {
        name,
        action: Action::Command(command),
    }
}

fn cargo_command(args: &[&str]) -> Command {
    let mut command = cargo();
    command.args(args);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arguments of a step that runs a command, as one line.
    fn line(step: &Step) -> String {
        let Action::Command(command) = &step.action else {
            panic!("`{}` does not run a command", step.name);
        };
        command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn the_steps_run_in_the_documented_order() {
        let steps = steps(true);
        let names: Vec<_> = steps.iter().map(|step| step.name).collect();
        assert_eq!(
            names,
            ["format", "clippy", "tests", "cargo-deny", "rustdoc"]
        );
    }

    #[test]
    fn every_cargo_step_is_strict() {
        let steps = steps(true);
        let lines: Vec<_> = steps.iter().map(line).collect();
        assert_eq!(lines[0], "fmt --all --check");
        assert_eq!(
            lines[1],
            "clippy --workspace --all-targets --all-features --locked -- -D warnings"
        );
        assert_eq!(lines[2], "test --workspace --all-features --locked");
        assert_eq!(lines[3], "deny check");
        assert_eq!(lines[4], "doc --workspace --no-deps --locked");
    }

    #[test]
    fn rustdoc_turns_warnings_into_errors() {
        let steps = steps(true);
        let Some(Action::Command(command)) = steps.last().map(|step| &step.action) else {
            panic!("the last step runs a command");
        };
        let flags = command
            .get_envs()
            .find(|(name, _)| *name == "RUSTDOCFLAGS")
            .and_then(|(_, value)| value);
        assert_eq!(flags, Some(std::ffi::OsStr::new("-D warnings")));
    }
}
