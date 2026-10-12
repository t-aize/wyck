//! Keeps the writing of the repository plain (AGENTS.md, "Writing style").
//!
//! No typographic dashes, quotes, arrows or ellipsis, no emoji and no invisible characters in any
//! text file the repository tracks, and Rust sources are ASCII (the section sign is the one
//! exception, for citing a section of an external document). The characters are named by code
//! point here so that this file stays clean itself.
//!
//! The files are the ones `git ls-files` lists; outside a git checkout (a source archive) the
//! folder is walked instead, leaving out build output and tool state.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Folders that are never ours to word, for the walk.
const SKIP_DIRS: [&str; 5] = ["target", ".git", "dist", ".claude", ".agents"];

/// Files that are not ours to word: a licence text is somebody else's, a lock file is generated.
const SKIP_FILES: [&str; 2] = ["LICENSE", "Cargo.lock"];

/// The text files that are checked, by extension. The rest (fonts, images, sounds) are binary.
const TEXT_EXTENSIONS: [&str; 15] = [
    "rs",
    "md",
    "yml",
    "yaml",
    "toml",
    "rhai",
    "py",
    "json",
    "txt",
    "svg",
    "pubkey",
    "example",
    "editorconfig",
    "gitignore",
    "gitattributes",
];

/// The section sign, allowed in Rust sources.
const SECTION_SIGN: char = '\u{00A7}';

/// What is wrong with `c`, if anything.
fn forbidden(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{2013}' => "en dash",
        '\u{2014}' => "em dash",
        '\u{2018}' | '\u{2019}' => "curly single quote",
        '\u{201C}' | '\u{201D}' => "curly double quote",
        '\u{2026}' => "horizontal ellipsis",
        '\u{2190}'..='\u{2193}' => "typographic arrow",
        '\u{2212}' => "true minus sign",
        '\u{00D7}' => "multiplication sign",
        '\u{2248}' => "approximately-equal sign",
        '\u{200B}' | '\u{FEFF}' => "invisible character",
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => "bidirectional control",
        '\u{1F300}'..='\u{1FAFF}' => "emoji",
        _ => return None,
    })
}

/// Every problem in `text`, as `line: what (code point)`.
fn findings(text: &str, ascii_only: bool) -> Vec<String> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for c in line.chars() {
            let what = forbidden(c).or_else(|| {
                (ascii_only && !c.is_ascii() && c != SECTION_SIGN).then_some("non-ASCII character")
            });
            if let Some(what) = what {
                found.push(format!(
                    "line {}: {what} (U+{:04X})",
                    index + 1,
                    u32::from(c)
                ));
            }
        }
    }
    found
}

fn tracked_files(root: &Path) -> Vec<PathBuf> {
    let listed = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .ok()
        .filter(|output| output.status.success());
    match listed {
        Some(output) => output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .filter_map(|name| std::str::from_utf8(name).ok())
            .map(|name| root.join(name))
            .collect(),
        None => {
            let mut files = Vec::new();
            walk(root, &mut files);
            files
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !SKIP_DIRS.iter().any(|skip| name == **skip) {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn is_checked(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if SKIP_FILES.contains(&name) {
        return false;
    }
    // `.gitignore` has no extension by Rust's reckoning: its name is the extension.
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .or_else(|| name.strip_prefix('.'));
    extension.is_some_and(|ext| TEXT_EXTENSIONS.contains(&ext))
}

#[test]
fn no_tracked_text_file_has_a_character_that_marks_machine_written_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut problems = Vec::new();
    let mut checked = 0;
    for path in tracked_files(root)
        .into_iter()
        .filter(|path| is_checked(path))
    {
        let Ok(bytes) = fs::read(&path) else {
            continue; // listed but gone, such as a file deleted and not yet committed
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let Ok(text) = String::from_utf8(bytes) else {
            problems.push(format!("{relative}: not valid UTF-8"));
            continue;
        };
        checked += 1;
        let ascii_only = path.extension().is_some_and(|ext| ext == "rs");
        problems.extend(
            findings(&text, ascii_only)
                .into_iter()
                .map(|finding| format!("{relative}: {finding}")),
        );
    }
    assert!(
        checked > 100,
        "only {checked} files were looked at: is the file list right?"
    );
    assert!(
        problems.is_empty(),
        "AGENTS.md, \"Writing style\": use a comma, a colon, parentheses or a new sentence for a \
         dash, \"->\" for an arrow, three ASCII dots for an ellipsis, straight quotes, and keep Rust \
         sources ASCII.\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_check_sees_each_kind_of_character_it_is_there_for() {
    let samples = [
        ("a \u{2014} b", "em dash"),
        ("a \u{2013} b", "en dash"),
        ("it\u{2019}s", "curly single quote"),
        ("\u{201C}x\u{201D}", "curly double quote"),
        ("wait\u{2026}", "horizontal ellipsis"),
        ("a \u{2192} b", "typographic arrow"),
        ("a \u{2212} b", "true minus sign"),
        ("2 \u{00D7} 3", "multiplication sign"),
        ("a \u{2248} b", "approximately-equal sign"),
        ("a\u{200B}b", "invisible character"),
        ("\u{FEFF}a", "invisible character"),
        ("a\u{202E}b", "bidirectional control"),
        ("done \u{1F680}", "emoji"),
    ];
    for (text, what) in samples {
        let found = findings(text, false);
        assert!(
            found.iter().any(|finding| finding.contains(what)),
            "{what} not found in {text:?}: {found:?}"
        );
    }
    assert!(findings("plain text, a -> b, x... and 'quotes' \"too\"", true).is_empty());
}

#[test]
fn rust_sources_are_ascii_with_the_section_sign_as_the_one_exception() {
    assert!(findings("// see \u{00A7}4.2 of the spec", true).is_empty());
    assert_eq!(findings("let name = \"caf\u{00E9}\";", true).len(), 1);
    assert!(
        findings("let name = \"caf\u{00E9}\";", false).is_empty(),
        "other text files may hold accents; only the marks above are refused"
    );
}

#[test]
fn only_text_files_are_checked() {
    for checked in [
        "src/lib.rs",
        "README.md",
        ".gitignore",
        "assets/a/b.svg",
        "scripts/x.py",
    ] {
        assert!(is_checked(Path::new(checked)), "{checked}");
    }
    for skipped in [
        "LICENSE",
        "Cargo.lock",
        "assets/Inter.ttf",
        "assets/icon.png",
        "dist/a.exe",
    ] {
        assert!(!is_checked(Path::new(skipped)), "{skipped}");
    }
}
