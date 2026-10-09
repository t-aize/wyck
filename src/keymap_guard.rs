//! Fails the build when two actions are bound to the same keys in the same context: GPUI
//! lets the later binding win, so the earlier action silently stops working.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The text between the parentheses of the call whose opening one is at `open`.
fn call_body(source: &str, open: usize) -> Option<&str> {
    let mut depth = 0usize;
    let mut in_string = false;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&source[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// `(keys, action, context)` of a `KeyBinding::new(keys, Action, Some("Context"))` call body.
fn parse_binding(body: &str) -> Option<(String, String, String)> {
    let mut parts = body.splitn(3, '"');
    parts.next()?;
    let keys = parts.next()?;
    let rest = parts.next()?;
    let action = rest
        .trim_start_matches(|c: char| c == ',' || c.is_whitespace())
        .split(',')
        .next()?
        .trim();
    let context = rest
        .split("Some(\"")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .unwrap_or("");
    Some((keys.to_owned(), action.to_owned(), context.to_owned()))
}

#[test]
fn no_two_actions_share_keys_in_one_context() {
    let mut files = Vec::new();
    rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);

    let mut seen: HashMap<(String, String), (String, PathBuf)> = HashMap::new();
    let mut clashes = Vec::new();
    for file in files {
        if file.ends_with("keymap_guard.rs") {
            continue;
        }
        let source = fs::read_to_string(&file).expect("a source file is readable");
        for (at, _) in source.match_indices("KeyBinding::new(") {
            let open = at + "KeyBinding::new".len();
            let Some((keys, action, context)) = call_body(&source, open).and_then(parse_binding)
            else {
                continue;
            };
            if let Some((other, other_file)) =
                seen.insert((keys.clone(), context.clone()), (action.clone(), file.clone()))
            {
                clashes.push(format!(
                    "{keys} in {context}: {other} ({}) and {action} ({})",
                    other_file.display(),
                    file.display()
                ));
            }
        }
    }
    assert!(clashes.is_empty(), "key binding clashes:\n{}", clashes.join("\n"));
}
