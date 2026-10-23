//! A test that reads the source of the screens and fails on a color spelled out as a literal:
//! the screens take their colors from the theme.

use crate::sources::{crates_dir, name_of, rust_files};

/// Files (or, ending in a slash, folders) that may hold color literals: the brand colors of the
/// symbol marks, the palette, and the color picker.
const ALLOWED: &[&str] = &[
    "wyck-ui/src/shell/dashboard/marks.rs",
    "wyck-ui/src/shell/dashboard/marks/",
    "wyck-ui/src/kit/theme.rs",
    "wyck-ui/src/kit/color_picker.rs",
    "wyck-ui/src/kit/color_picker/math.rs",
];

/// Whether a line has a color literal: `0x` and six or eight hex digits, other than the
/// transparent one used as a placeholder.
fn has_color(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or("");
    code.match_indices("0x").any(|(at, _)| {
        let digits: String = code
            .get(at + 2..)
            .unwrap_or_default()
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        (digits.len() == 6 || digits.len() == 8) && digits != "00000000"
    })
}

#[test]
fn screens_read_their_colors_from_the_theme() {
    let mut files = Vec::new();
    // The screens of the app and the shared controls; the chart has its own colors.
    for dir in [
        "wyck-ui/src/trading",
        "wyck-ui/src/editor",
        "wyck-ui/src/shell",
        "wyck-ui/src/chart/multichart",
        "wyck-ui/src/kit",
    ] {
        rust_files(&crates_dir().join(dir), &mut files);
    }
    let mut found = Vec::new();
    for file in files {
        let name = name_of(&file);
        if ALLOWED
            .iter()
            .any(|a| name == *a || (a.ends_with('/') && name.starts_with(a)))
        {
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
