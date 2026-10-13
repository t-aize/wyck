//! Keeps screens on the design system.
//!
//! - No `Button::new` outside `src/ui/kit`: screens take a button from `ui::kit::button`.
//! - No gpui-kit size method (`.xsmall()`, `.small()`, `.medium()`, `.large()`, `.compact()`) outside
//!   `src/ui/kit`: a button or field gets its size from the kit constructor it comes from.
//! - Spacing classes (`.gap_2()`, `.px_3()`) outside the kit stay on the scale of the app: 0, 1, 2,
//!   3, 4 and 6 (0, 4, 8, 12, 16 and 24 pixels, the steps of `tokens::space`). Anything else
//!   belongs in a token.
//! - No sized corner or shadow class (`.rounded_md()`, `.shadow_lg()`) anywhere but
//!   `src/ui/kit/shape.rs`: corners and shadows come from `ui::kit::shape::Shape`, which keeps the
//!   interface square and flat. `.rounded_full()` stays, for a dot or a switch.
//! - Pixel sizes written as numbers (`px(12.0)`) outside the kit are counted per file. The count
//!   may not grow; fixing some means running `BLESS=1 cargo test --test design_system`.
//! - So are the sizes that get round that rule: a `const NAME: f32 = 12.0` and a numeric size class
//!   (`.h_8()`, `.w_64()`, `.size_4()`), counted per file in their own list.
// A failed `unwrap` or `expect` is how a test fails. clippy.toml covers only `#[test]` functions
// and `#[cfg(test)]` modules, not the helpers of an integration test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const PIXELS: &str = "tests/design-baseline.txt";
const SIZES: &str = "tests/design-baseline-sizes.txt";

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

/// The corner and shadow classes with a size of their own: `.rounded_md()`, `.shadow_lg()`.
fn sized_corners(code: &str) -> Vec<String> {
    const CLASSES: [&str; 11] = [
        ".rounded_sm(",
        ".rounded_md(",
        ".rounded_lg(",
        ".rounded_xl(",
        ".rounded_2xl(",
        ".rounded_3xl(",
        ".shadow_sm(",
        ".shadow_md(",
        ".shadow_lg(",
        ".shadow_xl(",
        ".shadow_2xl(",
    ];
    CLASSES
        .iter()
        .filter(|class| code.contains(**class))
        .map(|class| (*class).to_owned())
        .collect()
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

/// How many sizes are written round the rule above: `const WIDTH: f32 = 12.0;` and the numeric size
/// classes `.h_8()`, `.w_64()`, `.size_4()`, `.min_w_32()`.
fn literal_sizes(code: &str) -> usize {
    let mut count = 0;
    for line in code.lines() {
        let line = line.trim_start();
        let line = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub(super) "))
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        if let Some(rest) = line.strip_prefix("const ")
            && let Some((_, value)) = rest.split_once(": f32 =")
            && value.trim_start().starts_with(|c: char| c.is_ascii_digit())
        {
            count += 1;
        }
    }
    for class in [
        ".h_", ".w_", ".size_", ".min_h_", ".min_w_", ".max_h_", ".max_w_",
    ] {
        let mut rest = code;
        while let Some(at) = rest.find(class) {
            let after = &rest[at + class.len()..];
            if after.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                count += 1;
            }
            rest = after;
        }
    }
    count
}

fn read_baseline(path: &Path) -> BTreeMap<String, usize> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (file, count) = l.split_once('\t')?;
            Some((file.to_owned(), count.parse().ok()?))
        })
        .collect()
}

/// Compares the counts of a rule with its list, or writes the list when blessing. Returns what is
/// wrong, empty when the counts are the listed ones or fewer than they were (then the list is to be
/// lowered).
fn check_counts(
    root: &Path,
    list: &str,
    title: &str,
    advice: &str,
    counts: &BTreeMap<String, usize>,
) -> String {
    let path = root.join(list);
    if std::env::var_os("BLESS").is_some() {
        let mut text = format!("# {title}, per file. Only goes down.\n");
        for (file, count) in counts {
            text.push_str(&format!("{file}\t{count}\n"));
        }
        fs::write(&path, text).expect("the baseline is writable");
        return String::new();
    }
    let baseline = read_baseline(&path);
    let mut grew = Vec::new();
    let mut shrank = Vec::new();
    for (file, count) in counts {
        let allowed = baseline.get(file).copied().unwrap_or(0);
        if *count > allowed {
            grew.push(format!("{file}: {count} (allowed {allowed})"));
        } else if *count < allowed {
            shrank.push(format!("{file}: {count} (listed {allowed})"));
        }
    }
    for (file, allowed) in &baseline {
        if !counts.contains_key(file) {
            shrank.push(format!("{file}: 0 (listed {allowed})"));
        }
    }
    let mut message = String::new();
    if !grew.is_empty() {
        message.push_str(&format!("more {title} than before, {advice}:\n"));
        for line in &grew {
            message.push_str(&format!("  {line}\n"));
        }
    }
    if !shrank.is_empty() {
        message.push_str(&format!(
            "fewer {title} than listed in {list}, run BLESS=1 cargo test --test design_system:\n"
        ));
        for line in &shrank {
            message.push_str(&format!("  {line}\n"));
        }
    }
    message
}

#[test]
fn screens_use_the_design_system() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);

    let mut buttons = Vec::new();
    let mut sizes = Vec::new();
    let mut spacing: Vec<String> = Vec::new();
    let mut corners: Vec<String> = Vec::new();
    let mut pixels: BTreeMap<String, usize> = BTreeMap::new();
    let mut literals: BTreeMap<String, usize> = BTreeMap::new();
    for path in files {
        let rel = path
            .strip_prefix(root)
            .expect("inside the repo")
            .to_string_lossy()
            .replace('\\', "/");
        let code = code_of(&fs::read_to_string(&path).expect("a source file is readable"));
        // Corners and shadows come from `ui::kit::shape`, in the kit as much as in the screens.
        if rel != "src/ui/kit/shape.rs" {
            for class in sized_corners(&code) {
                corners.push(format!("{rel}: {class})"));
            }
        }
        // `chart/paint.rs` draws the chart into an image, in pixels of that image and not of the
        // interface, so it has no tokens to take.
        if rel.starts_with("src/ui/kit")
            || rel == "src/lib.rs"
            || rel == "src/ui/features/chart/paint.rs"
        {
            continue;
        }
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
            pixels.insert(rel.clone(), count);
        }
        let count = if rel.starts_with("src/ui/") {
            literal_sizes(&code)
        } else {
            0
        };
        if count > 0 {
            literals.insert(rel, count);
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
    assert!(
        corners.is_empty(),
        "a sized corner or shadow class, use round_control, round_card, round_dialog or floats \
         of ui::kit::shape::Shape:\n  {}",
        corners.join("\n  ")
    );

    let mut message = check_counts(
        root,
        PIXELS,
        "literal pixel sizes",
        "use a token from ui::kit::tokens",
        &pixels,
    );
    message.push_str(&check_counts(
        root,
        SIZES,
        "sizes written as a constant or a numeric class",
        "take them from ui::kit::tokens",
        &literals,
    ));
    assert!(message.is_empty(), "{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rules_see_what_they_are_meant_to() {
        assert_eq!(sized_corners(".rounded_md().bg(x)"), [".rounded_md("]);
        assert!(sized_corners(".rounded_full().round_card().floats()").is_empty());
        assert_eq!(
            literal_sizes("const WIDTH: f32 = 12.0;\nconst NAME: &str = \"x\";\nel.h_8().w_full()"),
            2
        );
        assert_eq!(literal_sizes("pub(super) const GAP: f32 = 3.;"), 1);
        assert_eq!(literal_sizes("const FACTOR: f32 = SCALE * 2.0;"), 0);
        assert_eq!(literal_pixels("px(12.0) + px(x) + spx(3.0)"), 1);
    }
}
