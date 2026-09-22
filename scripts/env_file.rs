//! Updates specific `KEY=value` lines of a `.env`-style file, in place, without disturbing
//! anything else in it: comments, blank lines, unrelated keys, and their order all survive.
//!
//! Shared by `scripts/sign_in.rs` and `scripts/refresh_tokens.rs`, the two scripts that write
//! credentials back into a `.env` file rather than only printing them.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

/// Sets each `key=value` pair of `updates` in the file at `path`, creating the file if it does
/// not exist yet.
///
/// A key whose line already exists (`KEY=...`, no leading whitespace before the key: the shape
/// every line in `.env.example` uses) is replaced in place; a key not yet present is appended at
/// the end, after a blank line if the file did not already end with one. Values are written
/// as-is, with no quoting: every value this is used for is a token or a numeric id, neither of
/// which needs any.
pub fn update(path: &Path, updates: &[(&str, &str)]) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut set: HashSet<&str> = HashSet::new();
    let mut lines: Vec<String> = existing
        .lines()
        .map(|line| match line.split_once('=') {
            Some((key, _)) if !key.starts_with(['#', ' ', '\t']) => {
                match updates.iter().find(|(k, _)| *k == key) {
                    Some((k, v)) => {
                        set.insert(k);
                        format!("{k}={v}")
                    }
                    None => line.to_owned(),
                }
            }
            _ => line.to_owned(),
        })
        .collect();

    let missing: Vec<&(&str, &str)> = updates.iter().filter(|(k, _)| !set.contains(k)).collect();
    if !missing.is_empty() {
        if lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(String::new());
        }
        lines.extend(missing.into_iter().map(|(k, v)| format!("{k}={v}")));
    }

    let mut content = lines.join("\n");
    content.push('\n');
    fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path under the OS temp dir, unique to this process and to `tag`, so parallel tests
    /// (this file is compiled into more than one binary, each with its own test harness) never
    /// touch the same file.
    fn scratch_file(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "wyck-env-file-test-{}-{tag}.env",
            std::process::id()
        ))
    }

    #[test]
    fn an_existing_key_is_replaced_in_place_and_everything_else_survives() {
        let path = scratch_file("replace");
        fs::write(
            &path,
            "# a comment\nWYCK_OPENAPI_CLIENT_ID=old-id\n\nWYCK_OPENAPI_SYMBOL=EURUSD\n",
        )
        .unwrap();

        update(&path, &[("WYCK_OPENAPI_CLIENT_ID", "new-id")]).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(
            content,
            "# a comment\nWYCK_OPENAPI_CLIENT_ID=new-id\n\nWYCK_OPENAPI_SYMBOL=EURUSD\n"
        );
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_missing_key_is_appended_and_a_missing_file_is_created() {
        let path = scratch_file("append");
        let _ = fs::remove_file(&path);

        update(
            &path,
            &[
                ("WYCK_OPENAPI_ACCESS_TOKEN", "AT"),
                ("WYCK_OPENAPI_REFRESH_TOKEN", "RT"),
            ],
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(
            content,
            "WYCK_OPENAPI_ACCESS_TOKEN=AT\nWYCK_OPENAPI_REFRESH_TOKEN=RT\n"
        );
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_commented_out_key_is_left_alone_and_a_real_one_is_appended() {
        let path = scratch_file("comment");
        fs::write(&path, "# WYCK_OPENAPI_ACCOUNT_ID=\n").unwrap();

        update(&path, &[("WYCK_OPENAPI_ACCOUNT_ID", "123")]).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(
            content,
            "# WYCK_OPENAPI_ACCOUNT_ID=\n\nWYCK_OPENAPI_ACCOUNT_ID=123\n"
        );
        fs::remove_file(&path).unwrap();
    }
}
