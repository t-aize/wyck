//! A test that reads the source of the screens and fails on a color spelled out as a literal:
//! the screens take their colors from the theme.

use std::path::Path;

use crate::keymap_guard::rust_files;

/// Files that may hold color literals: the brand colors of the symbol marks, the palette, and
/// the color picker.
const ALLOWED: &[&str] = &[
    "src/ui/shell/dashboard/marks.rs",
    "wyck-ui-kit/src/theme.rs",
    "wyck-ui-kit/src/color_picker.rs",
    "wyck-ui-kit/src/color_picker/math.rs",
];

/// Whether a line has a color literal: `0x` and six or eight hex digits, other than the
/// transparent one used as a placeholder.
fn has_color(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or("");
    let mut rest = code;
    while let Some(at) = rest.find("0x") {
        let digits: String = rest[at + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if (digits.len() == 6 || digits.len() == 8) && digits != "00000000" {
            return true;
        }
        rest = &rest[at + 2..];
    }
    false
}

#[test]
fn screens_read_their_colors_from_the_theme() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    // The screens of the app and the shared controls; the chart has its own colors.
    for dir in [
        "src/ui/features/trading",
        "src/ui/features/indicators",
        "src/ui/shell/dashboard",
        "src/ui/shell/connection",
        "src/ui/shell/settings_hub",
        "src/ui/features/multichart",
        "../wyck-ui-kit/src",
    ] {
        rust_files(&root.join(dir), &mut files);
    }
    let mut found = Vec::new();
    for file in files {
        let name = file
            .canonicalize()
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED.iter().any(|a| name.ends_with(a)) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        // The tests of a file may spell colors out.
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for (n, line) in code.lines().enumerate() {
            if has_color(line) {
                found.push(format!("{name}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "colors spelled out instead of read from the theme:\n{}",
        found.join("\n")
    );
}
