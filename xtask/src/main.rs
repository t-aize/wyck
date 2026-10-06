//! Repository automation, run as `cargo xtask <command>`.
//!
//! `check` is the one command that says whether a change is fit to commit. CI, editor hooks and
//! agents run the same thing, so "green here" means "green there".

use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  check [--no-fmt]   format check, clippy with -D warnings, tests
                     --no-fmt skips the format check (CI runs it in its own job)
  help               show this text
";

/// One cargo invocation, as the arguments after `cargo`.
type Step = Vec<String>;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => {
            let rest = &args[1..];
            if let Some(unknown) = rest.iter().find(|a| a.as_str() != "--no-fmt") {
                eprintln!("unknown option `{unknown}`\n\n{USAGE}");
                return ExitCode::from(2);
            }
            let fmt = !rest.iter().any(|a| a == "--no-fmt");
            run(&check_steps(fmt, env::var_os("CI").is_some()))
        }
        Some("help" | "--help" | "-h") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command `{other}`\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// The cargo commands `check` runs, in order. The second clippy run is the SDK without its
/// network feature (types and calculations only), which CI also lints. `locked` makes cargo
/// refuse to touch `Cargo.lock`, as CI does.
fn check_steps(fmt: bool, locked: bool) -> Vec<Step> {
    let step = |args: &[&str]| -> Step {
        let mut out: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        if locked && args[0] != "fmt" {
            out.insert(1, "--locked".to_owned());
        }
        out
    };

    let mut steps = Vec::new();
    if fmt {
        steps.push(step(&["fmt", "--all", "--check"]));
    }
    steps.push(step(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ]));
    steps.push(step(&[
        "clippy",
        "-p",
        "wyck-openapi",
        "--no-default-features",
        "--lib",
        "--",
        "-D",
        "warnings",
    ]));
    steps.push(step(&["test", "--workspace", "--all-features"]));
    steps
}

fn run(steps: &[Step]) -> ExitCode {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    for step in steps {
        eprintln!("==> cargo {}", step.join(" "));
        match Command::new(&cargo).args(step).current_dir(&root).status() {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!("`cargo {}` failed with {status}", step[0]);
                return ExitCode::FAILURE;
            }
            Err(error) => {
                eprintln!("could not start cargo: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    eprintln!("==> all checks passed");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(steps: &[Step]) -> Vec<String> {
        steps.iter().map(|s| s.join(" ")).collect()
    }

    #[test]
    fn check_runs_format_then_clippy_then_tests() {
        let steps = lines(&check_steps(true, false));
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0], "fmt --all --check");
        assert!(steps[1].starts_with("clippy --workspace --all-targets --all-features"));
        assert!(steps[1].ends_with("-D warnings"));
        assert!(steps[2].contains("-p wyck-openapi --no-default-features --lib"));
        assert_eq!(steps[3], "test --workspace --all-features");
    }

    #[test]
    fn the_format_check_can_be_skipped() {
        let steps = lines(&check_steps(false, false));
        assert_eq!(steps.len(), 3);
        assert!(steps.iter().all(|s| !s.starts_with("fmt")));
    }

    #[test]
    fn locked_goes_after_the_subcommand_but_not_on_fmt() {
        let steps = lines(&check_steps(true, true));
        assert_eq!(steps[0], "fmt --all --check");
        assert!(steps[1].starts_with("clippy --locked --workspace"));
        assert!(steps[3].starts_with("test --locked --workspace"));
    }
}
