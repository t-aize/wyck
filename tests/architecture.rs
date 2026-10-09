//! Keeps the layers of `src/` apart. See `docs/architecture.md` for the rules.
//!
//! The code is read as text, not compiled: every `use` and every `crate::` or `super::` path is
//! resolved to a module, and the importing layer must be allowed to depend on the imported one.
//! Violations that existed before the rules did are listed in `architecture-baseline.txt`; the
//! list can only shrink. After fixing some, run `BLESS=1 cargo test --test architecture` to
//! rewrite it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::{self, Visit};

const BASELINE: &str = "tests/architecture-baseline.txt";

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

/// The module path of a file: `src/ui/kit/theme.rs` is `ui::kit::theme`, `src/lib.rs` is empty.
fn module_of(rel: &str) -> Vec<String> {
    let trimmed = rel.trim_end_matches(".rs");
    let mut parts: Vec<String> = trimmed.split('/').map(str::to_owned).collect();
    if parts
        .last()
        .is_some_and(|last| last == "lib" || last == "main" || last == "mod")
    {
        parts.pop();
    }
    parts
}

/// The layer a module path belongs to.
fn zone_of(path: &[String]) -> String {
    let at = |i: usize| path.get(i).map(String::as_str).unwrap_or("");
    match at(0) {
        "domain" => "domain".into(),
        "infra" => "infra".into(),
        "app" => "app".into(),
        "ui" => match at(1) {
            "kit" | "assets" => "ui::kit".into(),
            "features" => format!("ui::features::{}", at(2)),
            "shell" => "ui::shell".into(),
            _ => "ui".into(),
        },
        _ => "root".into(),
    }
}

fn allowed(from: &str, to: &str) -> bool {
    if from == to || from == "root" || to == "root" {
        return true;
    }
    match from {
        "domain" => false,
        "infra" => to == "domain",
        "app" => matches!(to, "domain" | "infra"),
        "ui::kit" => false,
        "ui::shell" => {
            matches!(to, "domain" | "app" | "ui::kit") || to.starts_with("ui::features::")
        }
        _ if from.starts_with("ui::features::") => matches!(to, "domain" | "app" | "ui::kit"),
        _ => true,
    }
}

/// Crates (or `std` modules) a layer must not name.
fn banned_external(zone: &str, path: &[String]) -> Option<String> {
    let first = path.first()?.as_str();
    let second = path.get(1).map(String::as_str).unwrap_or("");
    let banned: &[&str] = match zone {
        "domain" => &[
            "gpui",
            "gpui_kit",
            "tokio",
            "reqwest",
            "tokio_tungstenite",
            "keyring",
            "rodio",
            "notify_rust",
            "cargo_packager_updater",
            "directories",
        ],
        "infra" => &["gpui", "gpui_kit"],
        "app" => &["gpui_kit"],
        "ui::kit" => &["tokio", "reqwest", "tokio_tungstenite", "keyring"],
        z if z.starts_with("ui::") => &[
            "gpui_kit",
            "tokio",
            "reqwest",
            "tokio_tungstenite",
            "keyring",
        ],
        _ => &[],
    };
    if banned.contains(&first) {
        return Some(first.to_owned());
    }
    if zone == "domain" && first == "std" && matches!(second, "fs" | "net" | "env" | "process") {
        return Some(format!("std::{second}"));
    }
    None
}

struct Violation {
    key: String,
    example: String,
}

struct Scan<'a> {
    file: &'a str,
    module: &'a [String],
    zone: &'a str,
    depth: usize,
    found: BTreeMap<String, String>,
}

fn has_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| match &attr.meta {
        syn::Meta::List(list) if list.path.is_ident("cfg") => {
            list.tokens.to_string().contains("test")
        }
        syn::Meta::Path(path) => path.is_ident("test"),
        _ => false,
    })
}

impl Scan<'_> {
    fn note(&mut self, rule: &str, detail: String, line: usize) {
        let key = format!("{}\t{}\t{}", self.file, rule, detail);
        self.found
            .entry(key)
            .or_insert_with(|| format!("{}:{}", self.file, line));
    }

    fn record(&mut self, segs: &[String], line: usize) {
        let Some(first) = segs.first() else {
            return;
        };
        let absolute: Vec<String> = match first.as_str() {
            "crate" => segs[1..].to_vec(),
            "super" => {
                let ups = segs.iter().take_while(|s| *s == "super").count();
                if ups <= self.depth {
                    return;
                }
                let up = ups - self.depth;
                if up > self.module.len() {
                    return;
                }
                let mut path = self.module[..self.module.len() - up].to_vec();
                path.extend_from_slice(&segs[ups..]);
                path
            }
            "self" | "Self" => return,
            _ => {
                if let Some(bad) = banned_external(self.zone, segs) {
                    self.note("extern", format!("{} uses {}", self.zone, bad), line);
                }
                return;
            }
        };
        let target = zone_of(&absolute);
        if !allowed(self.zone, &target) {
            self.note("layer", format!("{} -> {}", self.zone, target), line);
        }
    }

    fn leaves(tree: &syn::UseTree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                Self::leaves(&p.tree, prefix, out);
                prefix.pop();
            }
            syn::UseTree::Name(n) => {
                if n.ident == "self" {
                    out.push(prefix.clone());
                } else {
                    prefix.push(n.ident.to_string());
                    out.push(prefix.clone());
                    prefix.pop();
                }
            }
            syn::UseTree::Rename(r) => {
                if r.ident == "self" {
                    out.push(prefix.clone());
                } else {
                    prefix.push(r.ident.to_string());
                    out.push(prefix.clone());
                    prefix.pop();
                }
            }
            syn::UseTree::Glob(_) => out.push(prefix.clone()),
            syn::UseTree::Group(g) => {
                for item in &g.items {
                    Self::leaves(item, prefix, out);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        if has_attr(&node.attrs, "path") {
            self.note(
                "style",
                "path attribute on a module".into(),
                node.span().start().line,
            );
        }
        if node.content.is_some() {
            self.depth += 1;
            visit::visit_item_mod(self, node);
            self.depth -= 1;
        }
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if !is_cfg_test(&node.attrs) {
            visit::visit_item_fn(self, node);
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if !is_cfg_test(&node.attrs) {
            visit::visit_item_impl(self, node);
        }
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let line = node.span().start().line;
        let mut leaves = Vec::new();
        Self::leaves(&node.tree, &mut Vec::new(), &mut leaves);
        for leaf in leaves {
            if leaf.len() == 2 && leaf[0] == "super" && leaf[1] == "*" {
                continue;
            }
            self.record(&leaf, line);
        }
        if let syn::UseTree::Path(p) = &node.tree
            && p.ident == "super"
            && let syn::UseTree::Glob(_) = *p.tree
        {
            self.note("style", "use super::*".into(), line);
        }
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        let segs: Vec<String> = node.segments.iter().map(|s| s.ident.to_string()).collect();
        if segs.len() >= 2 {
            self.record(&segs, node.span().start().line);
        }
        visit::visit_path(self, node);
    }
}

fn scan_all() -> BTreeMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let mut all = BTreeMap::new();
    for path in files {
        let rel = path
            .strip_prefix(root.join("src"))
            .expect("inside src")
            .to_string_lossy()
            .replace('\\', "/");
        let shown = format!("src/{rel}");
        let module = module_of(&rel);
        let zone = zone_of(&module);
        let source = fs::read_to_string(&path).expect("a source file is readable");
        let ast = syn::parse_file(&source).unwrap_or_else(|e| panic!("{shown}: {e}"));
        let mut scan = Scan {
            file: &shown,
            module: &module,
            zone: &zone,
            depth: 0,
            found: BTreeMap::new(),
        };
        if rel.ends_with("/mod.rs") {
            scan.note("style", "mod.rs file".into(), 1);
        }
        scan.visit_file(&ast);
        all.extend(scan.found);
    }
    all
}

#[test]
fn layers_stay_apart() {
    let found = scan_all();
    let baseline_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(BASELINE);

    if std::env::var_os("BLESS").is_some() {
        let mut text = String::from(
            "# Violations that predate the architecture test. Fix some, then BLESS=1 to shrink.\n",
        );
        for key in found.keys() {
            text.push_str(key);
            text.push('\n');
        }
        fs::write(&baseline_path, text).expect("the baseline is writable");
        return;
    }

    let baseline: BTreeSet<String> = fs::read_to_string(&baseline_path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();

    let new: Vec<Violation> = found
        .iter()
        .filter(|(key, _)| !baseline.contains(*key))
        .map(|(key, example)| Violation {
            key: key.clone(),
            example: example.clone(),
        })
        .collect();
    let stale: Vec<&String> = baseline
        .iter()
        .filter(|key| !found.contains_key(*key))
        .collect();

    let mut message = String::new();
    if !new.is_empty() {
        message.push_str("new architecture violations (see docs/architecture.md):\n");
        for v in &new {
            message.push_str(&format!(
                "  {} (first at {})\n",
                v.key.replace('\t', " | "),
                v.example
            ));
        }
    }
    if !stale.is_empty() {
        message.push_str(
            "fixed violations still listed in the baseline (run BLESS=1 cargo test --test architecture):\n",
        );
        for key in &stale {
            message.push_str(&format!("  {}\n", key.replace('\t', " | ")));
        }
    }
    assert!(message.is_empty(), "{message}");
}
