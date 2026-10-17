//! `cargo xtask docs-check`: the `AGENTS.md` and `CLAUDE.md` files say what the code is, and a
//! check keeps them from drifting.
//!
//! - Every crate (and `xtask`) has an `AGENTS.md` with the sections an agent needs, and a
//!   `CLAUDE.md` that starts by importing it.
//! - The `Depends on:` and `Used by:` lines of an `AGENTS.md` name exactly the workspace crates
//!   that `Cargo.toml` says. Only the crates of the workspace are checked, not the external ones.
//! - The root `AGENTS.md` stays short and is the index of every other Markdown file.
//! - The relative links of the Markdown files point at something that exists.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use toml::{Table, Value};

/// The most lines the root `AGENTS.md` may have: it is read at the start of every session.
const ROOT_LIMIT: usize = 200;

/// The sections every crate `AGENTS.md` has.
const REQUIRED_HEADINGS: [&str; 3] = ["## Invariants and traps", "## Tests", "## Do not"];

/// What the `CLAUDE.md` next to an `AGENTS.md` starts with: Claude Code reads `CLAUDE.md` only, so
/// it imports the shared file.
const IMPORT: &str = "@AGENTS.md";

struct Member {
    /// The package name, `wyck-core`.
    name: String,
    /// Relative to the root, `crates/wyck-core`.
    folder: String,
    /// The workspace crates it depends on, of every kind except development.
    uses: BTreeSet<String>,
}

pub(crate) fn run() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let members = members(&root)?;
    let mut problems = Vec::new();
    for member in &members {
        problems.extend(check_member(&root, member, &members));
    }
    problems.extend(check_root(&root, &members));
    problems.extend(check_links(&root, &members));
    if problems.is_empty() {
        println!(
            "the AGENTS.md and CLAUDE.md of {} crates and the root are in order",
            members.len()
        );
        return Ok(());
    }
    for problem in &problems {
        eprintln!("  {problem}");
    }
    bail!("{} problem(s) with the documentation files", problems.len())
}

/// The crates of `crates/*` and `xtask`.
fn members(root: &Path) -> Result<Vec<Member>> {
    let mut folders = vec!["xtask".to_owned()];
    let crates = root.join("crates");
    for entry in fs::read_dir(&crates).with_context(|| format!("reading {}", crates.display()))? {
        let entry = entry?;
        folders.push(format!("crates/{}", entry.file_name().to_string_lossy()));
    }
    let mut found = Vec::new();
    for folder in folders {
        let path = root.join(&folder).join("Cargo.toml");
        if !path.is_file() {
            continue;
        }
        let table = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?
            .parse::<Table>()
            .with_context(|| format!("parsing {}", path.display()))?;
        let name = table
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(Value::as_str)
            .with_context(|| format!("{} has no package name", path.display()))?
            .to_owned();
        found.push((name, folder, table));
    }
    let names: BTreeSet<String> = found.iter().map(|(name, _, _)| name.clone()).collect();
    let mut members: Vec<Member> = found
        .into_iter()
        .map(|(name, folder, table)| {
            let uses = crate::deps::dependencies(&table)
                .into_iter()
                .filter(|(section, dependency, _)| {
                    !section.ends_with("dev-dependencies") && names.contains(*dependency)
                })
                .map(|(_, dependency, _)| dependency.to_owned())
                .collect();
            Member { name, folder, uses }
        })
        .collect();
    members.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(members)
}

fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.replace("\r\n", "\n"))
}

fn check_member(root: &Path, member: &Member, all: &[Member]) -> Vec<String> {
    let folder = &member.folder;
    let mut problems = Vec::new();
    let Some(agents) = read_text(&root.join(folder).join("AGENTS.md")) else {
        problems.push(format!("{folder}/AGENTS.md is missing"));
        return problems;
    };
    match read_text(&root.join(folder).join("CLAUDE.md")) {
        None => problems.push(format!("{folder}/CLAUDE.md is missing")),
        Some(claude) if claude.lines().next() != Some(IMPORT) => {
            problems.push(format!("{folder}/CLAUDE.md must start with {IMPORT}"));
        }
        Some(_) => {}
    }
    let title = format!("# {}", member.name);
    if agents.lines().next() != Some(title.as_str()) {
        problems.push(format!("{folder}/AGENTS.md must start with `{title}`"));
    }
    for heading in REQUIRED_HEADINGS {
        if !agents.lines().any(|line| line == heading) {
            problems.push(format!("{folder}/AGENTS.md has no `{heading}` section"));
        }
    }
    let known: BTreeSet<&str> = all.iter().map(|member| member.name.as_str()).collect();
    let used_by: BTreeSet<String> = all
        .iter()
        .filter(|other| other.uses.contains(&member.name))
        .map(|other| other.name.clone())
        .collect();
    for (key, text, expected) in [
        ("Depends on:", depends_on(&agents), &member.uses),
        ("Used by:", used_by_line(&agents), &used_by),
    ] {
        let Some(text) = text else {
            problems.push(format!("{folder}/AGENTS.md has no `{key}` line"));
            continue;
        };
        let named = crate_names(&text, &known, &member.name);
        if &named != expected {
            problems.push(format!(
                "{folder}/AGENTS.md `{key}` names {} but Cargo.toml says {}",
                list(&named),
                list(expected)
            ));
        }
    }
    problems
}

fn check_root(root: &Path, members: &[Member]) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(agents) = read_text(&root.join("AGENTS.md")) else {
        problems.push("AGENTS.md is missing at the root".to_owned());
        return problems;
    };
    match read_text(&root.join("CLAUDE.md")) {
        None => problems.push("CLAUDE.md is missing at the root".to_owned()),
        Some(claude) if claude.lines().next() != Some(IMPORT) => {
            problems.push(format!("CLAUDE.md must start with {IMPORT}"));
        }
        Some(_) => {}
    }
    let lines = agents.lines().count();
    if lines > ROOT_LIMIT {
        problems.push(format!(
            "AGENTS.md has {lines} lines, the limit is {ROOT_LIMIT}"
        ));
    }
    // The index: every Markdown file of the root and every crate folder is named in it.
    let mut wanted: Vec<String> = markdown_files(root)
        .into_iter()
        .filter(|name| name != "AGENTS.md" && name != "CLAUDE.md")
        .collect();
    wanted.extend(members.iter().map(|member| member.folder.clone()));
    for name in wanted {
        if !agents.contains(&name) {
            problems.push(format!("the root AGENTS.md does not mention {name}"));
        }
    }
    problems
}

/// The names of the Markdown files at the root.
fn markdown_files(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        })
        .collect();
    found.sort();
    found
}

/// The relative links of the Markdown files that point at nothing.
fn check_links(root: &Path, members: &[Member]) -> Vec<String> {
    let mut files: Vec<String> = markdown_files(root);
    for member in members {
        files.push(format!("{}/AGENTS.md", member.folder));
        files.push(format!("{}/CLAUDE.md", member.folder));
    }
    let rules = root.join(".claude").join("rules");
    for entry in fs::read_dir(&rules).into_iter().flatten().flatten() {
        files.push(format!(
            ".claude/rules/{}",
            entry.file_name().to_string_lossy()
        ));
    }
    let mut problems = Vec::new();
    for file in files {
        let Some(text) = read_text(&root.join(&file)) else {
            continue;
        };
        let base = Path::new(&file).parent().unwrap_or_else(|| Path::new(""));
        for target in relative_links(&text) {
            if !root.join(base).join(&target).exists() {
                problems.push(format!("{file} links to {target}, which does not exist"));
            }
        }
    }
    problems
}

/// The targets of the relative Markdown links `[text](target)` of a text, without the fragment.
fn relative_links(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if in_code {
            continue;
        }
        // Code spans are not links.
        let plain = line.split('`').step_by(2).collect::<Vec<_>>().join(" ");
        let mut rest = plain.as_str();
        while let Some((_, after)) = rest.split_once("](") {
            let Some((target, tail)) = after.split_once(')') else {
                break;
            };
            rest = tail;
            let target = target.split('#').next().unwrap_or_default();
            if target.is_empty()
                || target.contains("://")
                || target.starts_with("mailto:")
                || target.contains(' ')
            {
                continue;
            }
            found.push(target.to_owned());
        }
    }
    found
}

/// The text of the `Depends on:` paragraph, up to a `Used by:` that follows it.
fn depends_on(text: &str) -> Option<String> {
    let (_, after) = text.split_once("Depends on:")?;
    let paragraph = after.split("\n\n").next().unwrap_or(after);
    Some(
        paragraph
            .split("Used by:")
            .next()
            .unwrap_or(paragraph)
            .to_owned(),
    )
}

/// The text of the `Used by:` paragraph.
fn used_by_line(text: &str) -> Option<String> {
    let (_, after) = text.split_once("Used by:")?;
    Some(after.split("\n\n").next().unwrap_or(after).to_owned())
}

/// The members named between backticks in a text, `own` left out.
fn crate_names(text: &str, known: &BTreeSet<&str>, own: &str) -> BTreeSet<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| known.contains(token) && *token != own)
        .map(str::to_owned)
        .collect()
}

fn list(names: &BTreeSet<String>) -> String {
    if names.is_empty() {
        return "nothing".to_owned();
    }
    names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# wyck-x\n\nDepends on: `wyck-core`, `serde` (`wyck-clock` in tests).\nUsed by: `wyck-y`, `wyck`.\n\n## Tests\n";

    fn known() -> BTreeSet<&'static str> {
        ["wyck", "wyck-clock", "wyck-core", "wyck-x", "wyck-y"]
            .into_iter()
            .collect()
    }

    #[test]
    fn the_two_lines_are_read_apart_even_in_one_paragraph() {
        let depends = crate_names(&depends_on(TEXT).unwrap(), &known(), "wyck-x");
        assert_eq!(
            depends,
            ["wyck-clock", "wyck-core"].map(str::to_owned).into()
        );
        let used = crate_names(&used_by_line(TEXT).unwrap(), &known(), "wyck-x");
        assert_eq!(used, ["wyck", "wyck-y"].map(str::to_owned).into());
    }

    #[test]
    fn a_line_that_is_missing_is_none() {
        assert_eq!(depends_on("# wyck-x\n"), None);
        assert_eq!(used_by_line("# wyck-x\n"), None);
    }

    #[test]
    fn links_are_found_outside_code_and_fragments_are_dropped() {
        let text = "See [a](docs/a.md#top), [b](https://x.y), [c](#here) and `[d](no.md)`.\n```\n[e](skipped.md)\n```\n[f](f.md)\n";
        assert_eq!(relative_links(text), ["docs/a.md", "f.md"]);
    }
}
