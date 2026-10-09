//! Keeps screens on the design system. See `docs/ui-design-system.md`.
//!
//! - No `Button::new` outside `src/ui/kit`: screens take a button from `ui::kit::button`.
//! - No gpui-kit size method (`.xsmall()`, `.small()`, `.medium()`, `.large()`, `.compact()`) outside
//!   `src/ui/kit`: a button or field gets its size from the kit constructor it comes from.
//! - Spacing classes (`.gap_2()`, `.px_3()`) outside the kit stay on the scale of the app: 0, 1, 2,
//!   3, 4 and 6 (0, 4, 8, 12, 16 and 24 pixels, the steps of `tokens::space`). Anything else
//!   belongs in a token.
//! - Pixel sizes written as numbers (`px(12.0)`) outside the kit are counted per file. The count
//!   may not grow; fixing some means running `BLESS=1 cargo test --test design_system`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const BASELINE: &str = "tests/design-baseline.txt";

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

/// The code of a file without its tests, line comments and doc comments.
fn code_of(source: &str) -> String {
    let body = source.split("#[cfg(test)]").next().unwrap_or("");
    body.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The spacing steps of gpui classes the screens may use.
const SPACING_STEPS: [&str; 6] = ["0", "1", "2", "3", "4", "6"];

/// The spacing classes written with a step that is not on the scale: `.gap_8()`, `.px_3p5()`.
fn off_scale_spacing(code: &str) -> Vec<String> {
    const PREFIXES: [&str; 17] = [
        "gap", "gap_x", "gap_y", "p", "px", "py", "pt", "pb", "pl", "pr", "m", "mx", "my", "mt",
        "mb", "ml", "mr",
    ];
    let mut found = Vec::new();
    for prefix in PREFIXES {
        let needle = format!(".{prefix}_");
        let mut rest = code;
        while let Some(at) = rest.find(&needle) {
            let after = &rest[at + needle.len()..];
            let step: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let closes = after[step.len()..].starts_with("()");
            let numeric = step.chars().next().is_some_and(|c| c.is_ascii_digit());
            if closes && numeric && !SPACING_STEPS.contains(&step.as_str()) {
                found.push(format!("{needle}{step}()"));
            }
            rest = after;
        }
    }
    found
}

/// How many times a pixel size is written as a number: `px(12.0)`, `px(8.)`, `px(4)`.
fn literal_pixels(code: &str) -> usize {
    let mut count = 0;
    let mut rest = code;
    while let Some(at) = rest.find("px(") {
        let before = rest[..at].chars().last();
        let after = rest[at + 3..].trim_start();
        let numeric = after.chars().next().is_some_and(|c| c.is_ascii_digit());
        if numeric && !before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            count += 1;
        }
        rest = &rest[at + 3..];
    }
    count
}

#[test]
fn screens_use_the_design_system() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);

    let mut buttons = Vec::new();
    let mut sizes = Vec::new();
    let mut spacing: Vec<String> = Vec::new();
    let mut pixels: BTreeMap<String, usize> = BTreeMap::new();
    for path in files {
        let rel = path
            .strip_prefix(root)
            .expect("inside the repo")
            .to_string_lossy()
            .replace('\\', "/");
        // `chart/paint.rs` draws the chart into an image, in pixels of that image and not of the
        // interface, so it has no tokens to take.
        if rel.starts_with("src/ui/kit")
            || rel == "src/lib.rs"
            || rel == "src/ui/features/chart/paint.rs"
        {
            continue;
        }
        let code = code_of(&fs::read_to_string(&path).expect("a source file is readable"));
        if code.contains("Button::new(") {
            buttons.push(rel.clone());
        }
        for class in off_scale_spacing(&code) {
            spacing.push(format!("{rel}: {class}"));
        }
        if [
            ".xsmall()",
            ".small()",
            ".medium()",
            ".large()",
            ".compact()",
        ]
        .iter()
        .any(|method| code.contains(method))
        {
            sizes.push(rel.clone());
        }
        let count = literal_pixels(&code);
        if count > 0 {
            pixels.insert(rel, count);
        }
    }

    assert!(
        buttons.is_empty(),
        "Button::new outside the kit, use a constructor of ui::kit::button:\n  {}",
        buttons.join("\n  ")
    );

    assert!(
        spacing.is_empty(),
        "spacing off the scale outside the kit, use a step of the scale or a token:\n  {}",
        spacing.join("\n  ")
    );
    assert!(
        sizes.is_empty(),
        "a size method outside the kit, take the size from a constructor of ui::kit:\n  {}",
        sizes.join("\n  ")
    );

    let baseline_path = root.join(BASELINE);
    if std::env::var_os("BLESS").is_some() {
        let mut text =
            String::from("# Pixel sizes written as numbers, per file. Only goes down.\n");
        for (file, count) in &pixels {
            text.push_str(&format!("{file}\t{count}\n"));
        }
        fs::write(&baseline_path, text).expect("the baseline is writable");
        return;
    }

    let baseline: BTreeMap<String, usize> = fs::read_to_string(&baseline_path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (file, count) = l.split_once('\t')?;
            Some((file.to_owned(), count.parse().ok()?))
        })
        .collect();

    let mut grew = Vec::new();
    let mut shrank = Vec::new();
    for (file, count) in &pixels {
        let allowed = baseline.get(file).copied().unwrap_or(0);
        if *count > allowed {
            grew.push(format!("{file}: {count} (allowed {allowed})"));
        } else if *count < allowed {
            shrank.push(format!("{file}: {count} (listed {allowed})"));
        }
    }
    for (file, allowed) in &baseline {
        if !pixels.contains_key(file) {
            shrank.push(format!("{file}: 0 (listed {allowed})"));
        }
    }
    let mut message = String::new();
    if !grew.is_empty() {
        message
            .push_str("more literal pixel sizes than before, use a token from ui::kit::tokens:\n");
        for line in &grew {
            message.push_str(&format!("  {line}\n"));
        }
    }
    if !shrank.is_empty() {
        message.push_str("fewer than listed, run BLESS=1 cargo test --test design_system:\n");
        for line in &shrank {
            message.push_str(&format!("  {line}\n"));
        }
    }
    assert!(message.is_empty(), "{message}");
}
