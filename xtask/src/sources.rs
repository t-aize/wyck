//! The source rules of `cargo xtask deps`: which module of a crate may name which, and in which
//! files an external crate may appear. Cargo only sees whole crates; these see inside one.
//!
//! The scan is textual and line based: a `//` comment is cut off, nothing else is parsed. A rule
//! about modules sees `crate::module` paths only, so a module reaches a sibling with `crate::`,
//! never with `super::super::`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// A `.rs` file: its path from the workspace root with `/` separators, and its text.
pub(crate) struct SourceFile {
    pub(crate) path: String,
    pub(crate) text: String,
}

/// A top-level module of a crate and the sibling modules it may name through `crate::`.
struct ModuleLayer {
    /// The `src` folder of the crate, `crates/wyck-domain/src`.
    src: &'static str,
    module: &'static str,
    may_use: &'static [&'static str],
}

/// An external crate that only some files may name, because one place owns the job.
struct Owner {
    /// The name in code, with underscores.
    ident: &'static str,
    /// Path prefixes from the workspace root: a folder (with its final `/`) or a file.
    only_under: &'static [&'static str],
}

/// The layering inside the crates that hold several modules. A crate that is merged from several
/// ones adds its modules here, in the order of its old dependency chain.
const MODULES: &[ModuleLayer] = &[
    ModuleLayer {
        src: "crates/wyck-domain/src",
        module: "base",
        may_use: &[],
    },
    ModuleLayer {
        src: "crates/wyck-domain/src",
        module: "market",
        may_use: &["base"],
    },
    ModuleLayer {
        src: "crates/wyck-domain/src",
        module: "trading",
        may_use: &["base", "market"],
    },
    ModuleLayer {
        src: "crates/wyck-domain/src",
        module: "drawings",
        may_use: &["base", "trading"],
    },
    ModuleLayer {
        src: "crates/wyck-domain/src",
        module: "indicators",
        may_use: &["base", "drawings", "market"],
    },
    ModuleLayer {
        src: "crates/wyck-domain/src",
        module: "chart",
        may_use: &["base", "drawings", "indicators", "market"],
    },
    ModuleLayer {
        src: "crates/wyck-ui/src",
        module: "kit",
        may_use: &[],
    },
    ModuleLayer {
        src: "crates/wyck-ui/src",
        module: "chart",
        may_use: &["kit"],
    },
    ModuleLayer {
        src: "crates/wyck-ui/src",
        module: "trading",
        may_use: &["kit", "chart"],
    },
    ModuleLayer {
        src: "crates/wyck-ui/src",
        module: "editor",
        may_use: &["kit", "chart"],
    },
    ModuleLayer {
        src: "crates/wyck-ui/src",
        module: "shell",
        may_use: &["kit", "chart", "trading", "editor"],
    },
];

const OWNERS: &[Owner] = &[
    Owner {
        ident: "anyhow",
        only_under: &["crates/wyck-ui/src/editor/"],
    },
    Owner {
        ident: "ab_glyph",
        only_under: &[
            "crates/wyck-ui/src/chart/chart/paint.rs",
            "crates/wyck-ui/src/chart/chart/raster.rs",
        ],
    },
    Owner {
        ident: "gpui_kit",
        only_under: &["crates/wyck-ui/src/kit/"],
    },
    Owner {
        ident: "rhai",
        only_under: &["crates/wyck-domain/src/indicators/custom/"],
    },
    Owner {
        ident: "tiny_skia",
        only_under: &[
            "crates/wyck-ui/src/chart/chart/paint.rs",
            "crates/wyck-ui/src/chart/chart/raster.rs",
        ],
    },
];

/// The problems of every `.rs` file under `crates/`, one line each.
pub(crate) fn check_tree(root: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    collect(root, &root.join("crates"), &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(check(&files, MODULES, OWNERS))
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<SourceFile>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect(root, &path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let shown = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push(SourceFile { path: shown, text });
        }
    }
    Ok(())
}

fn check(files: &[SourceFile], modules: &[ModuleLayer], owners: &[Owner]) -> Vec<String> {
    let mut problems = Vec::new();
    for file in files {
        let code = strip_comments(&file.text);
        for owner in owners {
            if owner
                .only_under
                .iter()
                .any(|prefix| file.path.starts_with(prefix))
            {
                continue;
            }
            for offset in named(&code, owner.ident) {
                problems.push(format!(
                    "{}:{}: `{}` may only be named under {}",
                    file.path,
                    line_of(&code, offset),
                    owner.ident,
                    owner.only_under.join(", ")
                ));
            }
        }
        let Some(layer) = modules.iter().find(|layer| in_module(&file.path, layer)) else {
            continue;
        };
        let siblings = modules.iter().filter(|other| other.src == layer.src);
        for (offset, target) in crate_refs(&code) {
            let is_sibling = siblings.clone().any(|other| other.module == target);
            if is_sibling && target != layer.module && !layer.may_use.contains(&target) {
                problems.push(format!(
                    "{}:{}: module `{}` may not name `crate::{target}` (see MODULES in \
                     xtask/src/sources.rs)",
                    file.path,
                    line_of(&code, offset),
                    layer.module
                ));
            }
        }
    }
    problems
}

/// The file is `src/module.rs` or somewhere under `src/module/`.
fn in_module(path: &str, layer: &ModuleLayer) -> bool {
    path.strip_prefix(layer.src)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|rest| {
            rest == format!("{}.rs", layer.module)
                || rest.starts_with(&format!("{}/", layer.module))
        })
}

/// The text without its `//` comments (and so its doc comments), newlines kept so that a byte
/// offset still gives the right line. A `//` inside a string cuts the line too: that can hide a
/// name but never invent one.
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.split("//").next().unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_of(code: &str, offset: usize) -> usize {
    code.get(..offset)
        .map_or(0, |before| before.matches('\n').count())
        + 1
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A path start: not the tail of a longer name (`a_crate::`) nor of a longer path (`x::crate::`).
fn starts_a_path(code: &str, offset: usize) -> bool {
    !code
        .get(..offset)
        .and_then(|before| before.chars().next_back())
        .is_some_and(|c| is_ident_char(c) || c == ':')
}

/// The offsets of `ident::` used as the start of a path.
fn named(code: &str, ident: &str) -> Vec<usize> {
    code.match_indices(&format!("{ident}::"))
        .map(|(offset, _)| offset)
        .filter(|offset| starts_a_path(code, *offset))
        .collect()
}

fn head(text: &str) -> &str {
    let length = text.chars().take_while(|c| is_ident_char(*c)).count();
    text.get(..length).unwrap_or_default()
}

/// The offset and first segment of every `crate::x` and every item of `crate::{x, y::z}`.
fn crate_refs(code: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    for offset in named(code, "crate") {
        let Some(rest) = code.get(offset + "crate::".len()..) else {
            continue;
        };
        if let Some(group) = rest.strip_prefix('{') {
            found.extend(group_heads(group).into_iter().map(|name| (offset, name)));
        } else {
            let name = head(rest);
            if !name.is_empty() {
                found.push((offset, name));
            }
        }
    }
    found
}

/// The first segment of each item of a `{ ... }` group, given the text after its `{`.
fn group_heads(rest: &str) -> Vec<&str> {
    let mut heads = Vec::new();
    let mut depth = 1_usize;
    let mut at_item = true;
    let mut skip_until = 0;
    for (index, byte) in rest.bytes().enumerate() {
        if index < skip_until {
            continue;
        }
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => at_item = true,
            b if b.is_ascii_whitespace() => {}
            _ if at_item && depth == 1 => {
                let name = rest.get(index..).map(head).unwrap_or_default();
                skip_until = index + name.len();
                if !name.is_empty() {
                    heads.push(name);
                }
                at_item = false;
            }
            _ => {}
        }
    }
    heads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            path: path.to_owned(),
            text: text.to_owned(),
        }
    }

    const CHAIN: &[ModuleLayer] = &[
        ModuleLayer {
            src: "crates/d/src",
            module: "core",
            may_use: &[],
        },
        ModuleLayer {
            src: "crates/d/src",
            module: "market",
            may_use: &["core"],
        },
        ModuleLayer {
            src: "crates/d/src",
            module: "chart",
            may_use: &["core", "market"],
        },
    ];

    const RHAI: &[Owner] = &[Owner {
        ident: "rhai",
        only_under: &["crates/d/src/custom/"],
    }];

    #[test]
    fn an_owner_may_name_its_crate_and_nobody_else() {
        let files = [
            file("crates/d/src/custom/engine.rs", "use rhai::Engine;\n"),
            file("crates/d/src/other.rs", "\nlet e = rhai::Engine::new();\n"),
        ];
        let problems = check(&files, &[], RHAI);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("crates/d/src/other.rs:2:"));
    }

    #[test]
    fn a_comment_or_a_longer_name_is_not_a_use() {
        let files = [file(
            "crates/d/src/other.rs",
            "// rhai::Engine is documented here\n/// rhai::Scope\nuse crate::my_rhai::Thing;\nuse x::rhai::Y;\n",
        )];
        assert_eq!(check(&files, &[], RHAI), Vec::<String>::new());
    }

    #[test]
    fn a_module_may_name_the_modules_below_it_and_itself() {
        let files = [
            file(
                "crates/d/src/chart/scene.rs",
                "use crate::market::Bar;\nuse crate::core::ta;\n",
            ),
            file("crates/d/src/chart.rs", "use crate::chart::scene;\n"),
        ];
        assert_eq!(check(&files, CHAIN, &[]), Vec::<String>::new());
    }

    #[test]
    fn a_module_may_not_name_one_above_it() {
        let files = [file(
            "crates/d/src/market/bar.rs",
            "\n\nuse crate::chart::Scene;\n",
        )];
        let problems = check(&files, CHAIN, &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("crates/d/src/market/bar.rs:3"));
        assert!(problems[0].contains("module `market` may not name `crate::chart`"));
    }

    #[test]
    fn a_group_is_read_item_by_item_across_lines() {
        let files = [file(
            "crates/d/src/core/ta.rs",
            "use crate::{\n    core::Price,\n    market::{Bar, Tick},\n    chart,\n};\n",
        )];
        let problems = check(&files, CHAIN, &[]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("`crate::market`")));
        assert!(problems.iter().any(|p| p.contains("`crate::chart`")));
    }

    #[test]
    fn a_name_that_is_not_a_module_is_left_alone() {
        let files = [file("crates/d/src/core/ta.rs", "use crate::Error;\n")];
        assert_eq!(check(&files, CHAIN, &[]), Vec::<String>::new());
    }

    #[test]
    fn a_file_outside_every_module_is_not_checked() {
        let files = [file(
            "crates/d/src/lib.rs",
            "pub use crate::chart::Scene;\n",
        )];
        assert_eq!(check(&files, CHAIN, &[]), Vec::<String>::new());
    }

    #[test]
    fn the_real_tree_follows_the_rules() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let problems = check_tree(root).unwrap();
        assert_eq!(problems, Vec::<String>::new());
    }
}
