//! `cargo xtask deps`: the rules about dependencies, metadata and lints that make the workspace
//! one thing instead of many.
//!
//! - A crate never writes a version, a path or a git source for a dependency: it says
//!   `name.workspace = true` and the root `Cargo.toml` decides.
//! - The shared package metadata and the lints come from the root too.
//! - Every crate has a place in [`LAYERS`]: the other crates of the workspace it may depend on and
//!   the external crates it must not use. Cargo already refuses a cycle; this refuses an edge that
//!   goes the wrong way without making one.
//! - Every dependency of the root is used by at least one crate.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use toml::{Table, Value};

/// Where a crate sits in the layering.
struct Layer {
    name: &'static str,
    /// The other crates of the workspace it may depend on.
    internal: &'static [&'static str],
    /// External crates it must not depend on, whatever the kind of dependency.
    forbidden: &'static [&'static str],
    /// Keeps a table of lints of its own instead of `[lints] workspace = true`. Only for the crate
    /// the others are being taken out of.
    own_lints: bool,
}

/// External crates the domain crates (no screen, no file, no network) must not use.
const NOT_IN_THE_DOMAIN: &[&str] = &[
    "ab_glyph",
    "cargo-packager-updater",
    "directories",
    "gpui",
    "gpui-kit",
    "keyring",
    "notify-rust",
    "reqwest",
    "rodio",
    "rust-embed",
    "tiny-skia",
    "tokio",
    "tokio-tungstenite",
    "tracing-appender",
    "tracing-subscriber",
];

/// External crates that only some crates may use, because one crate owns the job: a second user
/// would be a second way of doing the same thing.
const OWNERS: &[(&str, &[&str])] = &[("rhai", &["wyck-indicators"])];

/// Every crate of the workspace, with what it may depend on. A new crate is added here first.
const LAYERS: &[Layer] = &[
    Layer {
        name: "wyck-core",
        internal: &[],
        forbidden: NOT_IN_THE_DOMAIN,
        own_lints: false,
    },
    Layer {
        name: "wyck-market",
        internal: &["wyck-core"],
        forbidden: NOT_IN_THE_DOMAIN,
        own_lints: false,
    },
    Layer {
        name: "wyck-trading",
        internal: &["wyck-core", "wyck-market"],
        forbidden: NOT_IN_THE_DOMAIN,
        own_lints: false,
    },
    Layer {
        name: "wyck-drawings",
        internal: &["wyck-core", "wyck-trading"],
        forbidden: NOT_IN_THE_DOMAIN,
        own_lints: false,
    },
    Layer {
        name: "wyck-indicators",
        internal: &["wyck-core", "wyck-drawings", "wyck-market"],
        forbidden: NOT_IN_THE_DOMAIN,
        own_lints: false,
    },
    Layer {
        name: "wyck",
        internal: &[
            "wyck-core",
            "wyck-drawings",
            "wyck-indicators",
            "wyck-market",
            "wyck-trading",
        ],
        forbidden: &[],
        own_lints: true,
    },
    Layer {
        name: "xtask",
        internal: &[],
        forbidden: &["gpui", "gpui-kit", "tokio", "reqwest", "keyring"],
        own_lints: false,
    },
];

/// The `[package]` fields that must be `field.workspace = true`.
const INHERITED_FIELDS: [&str; 7] = [
    "version",
    "edition",
    "rust-version",
    "license",
    "authors",
    "repository",
    "publish",
];

/// The keys of a dependency that say where it comes from: the root decides that.
const SOURCE_KEYS: [&str; 8] = [
    "version", "path", "git", "branch", "tag", "rev", "registry", "package",
];

const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

struct Manifest {
    /// Shown in messages, `crates/wyck/Cargo.toml`.
    path: String,
    /// The name of the folder of the crate.
    folder: String,
    table: Table,
}

pub(crate) fn run() -> Result<()> {
    let root = workspace_root();
    let root_table = read(&root.join("Cargo.toml"))?;
    let members = member_manifests(&root)?;
    let problems = check_workspace(&root_table, &members, LAYERS);
    if problems.is_empty() {
        println!("{} crates follow the dependency rules", members.len());
        return Ok(());
    }
    for problem in &problems {
        eprintln!("  {problem}");
    }
    bail!("{} problem(s) with the dependency rules", problems.len())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn read(path: &Path) -> Result<Table> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.parse::<Table>()
        .with_context(|| format!("parsing {}", path.display()))
}

/// The manifests of `crates/*` and of `xtask`.
fn member_manifests(root: &Path) -> Result<Vec<Manifest>> {
    let mut folders = vec![root.join("xtask")];
    let crates = root.join("crates");
    for entry in fs::read_dir(&crates).with_context(|| format!("reading {}", crates.display()))? {
        folders.push(entry?.path());
    }
    let mut manifests = Vec::new();
    for folder in folders {
        let manifest = folder.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let name = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let shown = manifest
            .strip_prefix(root)
            .unwrap_or(&manifest)
            .to_string_lossy()
            .replace('\\', "/");
        manifests.push(Manifest {
            path: shown,
            folder: name,
            table: read(&manifest)?,
        });
    }
    manifests.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(manifests)
}

/// All the problems of the workspace, one line each.
fn check_workspace(root: &Table, members: &[Manifest], layers: &[Layer]) -> Vec<String> {
    let mut problems = Vec::new();
    let package_name = |manifest: &Manifest| {
        manifest
            .table
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let names: BTreeSet<String> = members.iter().filter_map(package_name).collect();
    let mut used: BTreeSet<String> = BTreeSet::new();

    for member in members {
        let Some(name) = package_name(member) else {
            problems.push(format!("{}: no package name", member.path));
            continue;
        };
        if name != member.folder {
            problems.push(format!(
                "{}: the crate is called `{name}` but its folder is `{}`",
                member.path, member.folder
            ));
        }
        let Some(layer) = layers.iter().find(|layer| layer.name == name) else {
            problems.push(format!(
                "{}: `{name}` has no place in LAYERS (xtask/src/deps.rs)",
                member.path
            ));
            continue;
        };
        problems.extend(check_metadata(member, layer));
        problems.extend(check_dependencies(member, layer, &names, &mut used));
    }

    for layer in layers {
        if !names.contains(layer.name) {
            problems.push(format!(
                "LAYERS lists `{}` but there is no such crate",
                layer.name
            ));
        }
    }

    let declared = root
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Value::as_table);
    for name in declared.into_iter().flat_map(Table::keys) {
        if !used.contains(name) {
            problems.push(format!(
                "Cargo.toml: `{name}` is in [workspace.dependencies] but no crate uses it"
            ));
        }
    }
    problems
}

/// `field.workspace = true` for the shared package metadata, and the lints of the root.
fn check_metadata(member: &Manifest, layer: &Layer) -> Vec<String> {
    let mut problems = Vec::new();
    let package = member.table.get("package");
    for field in INHERITED_FIELDS {
        if !inherits(package.and_then(|package| package.get(field))) {
            problems.push(format!(
                "{}: [package] `{field}` must be `{field}.workspace = true`",
                member.path
            ));
        }
    }
    if !layer.own_lints && !inherits(member.table.get("lints")) {
        problems.push(format!(
            "{}: it must have `[lints] workspace = true`",
            member.path
        ));
    }
    problems
}

fn inherits(value: Option<&Value>) -> bool {
    value
        .and_then(|value| value.get("workspace"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn check_dependencies(
    member: &Manifest,
    layer: &Layer,
    names: &BTreeSet<String>,
    used: &mut BTreeSet<String>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (section, name, value) in dependencies(&member.table) {
        used.insert(name.to_owned());
        let from_root =
            inherits(Some(value)) && SOURCE_KEYS.iter().all(|key| value.get(key).is_none());
        if !from_root {
            problems.push(format!(
                "{}: `{name}` in [{section}] must be `{name}.workspace = true`: the root \
                 Cargo.toml says where it comes from and which version",
                member.path
            ));
        }
        if names.contains(name) && !layer.internal.contains(&name) {
            problems.push(format!(
                "{}: `{}` may not depend on `{name}` (see LAYERS in xtask/src/deps.rs)",
                member.path, layer.name
            ));
        }
        if layer.forbidden.contains(&name) {
            problems.push(format!(
                "{}: `{}` may not use `{name}`",
                member.path, layer.name
            ));
        }
        if let Some((_, owners)) = OWNERS.iter().find(|(owned, _)| *owned == name)
            && !owners.contains(&layer.name)
        {
            problems.push(format!(
                "{}: only {} may use `{name}`, not `{}`",
                member.path,
                owners.join(", "),
                layer.name
            ));
        }
    }
    problems
}

/// Every dependency of a manifest, of every kind and for every target: the table it is in
/// (`dependencies`, `target.'cfg(windows)'.build-dependencies`, ...), its name and its value.
fn dependencies(manifest: &Table) -> Vec<(String, &str, &Value)> {
    let mut found = Vec::new();
    collect_dependencies(&mut found, "", manifest);
    if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
        for (target, table) in targets {
            if let Some(table) = table.as_table() {
                collect_dependencies(&mut found, &format!("target.'{target}'."), table);
            }
        }
    }
    found
}

fn collect_dependencies<'a>(
    found: &mut Vec<(String, &'a str, &'a Value)>,
    prefix: &str,
    table: &'a Table,
) {
    for section in DEPENDENCY_TABLES {
        let Some(dependencies) = table.get(section).and_then(Value::as_table) else {
            continue;
        };
        for (name, value) in dependencies {
            found.push((format!("{prefix}{section}"), name.as_str(), value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        text.parse().unwrap()
    }

    fn member(folder: &str, extra: &str) -> Manifest {
        let text = format!(
            "[package]\nname = \"{folder}\"\nversion.workspace = true\nedition.workspace = true\n\
             rust-version.workspace = true\nlicense.workspace = true\nauthors.workspace = true\n\
             repository.workspace = true\npublish.workspace = true\n[lints]\nworkspace = true\n{extra}"
        );
        Manifest {
            path: format!("crates/{folder}/Cargo.toml"),
            folder: folder.to_owned(),
            table: table(&text),
        }
    }

    fn layer(name: &'static str, internal: &'static [&'static str]) -> Layer {
        Layer {
            name,
            internal,
            forbidden: &[],
            own_lints: false,
        }
    }

    const ROOT: &str = "[workspace.dependencies]\nserde = \"1\"\n";

    #[test]
    fn a_crate_that_follows_the_rules_passes() {
        let members = [member("a", "[dependencies]\nserde.workspace = true\n")];
        let problems = check_workspace(&table(ROOT), &members, &[layer("a", &[])]);
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn a_version_or_a_path_in_a_crate_is_refused() {
        let members = [member(
            "a",
            "[dependencies]\nserde = \"1\"\nlocal = { path = \"../x\" }\n",
        )];
        let problems = check_workspace(&table(ROOT), &members, &[layer("a", &[])]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems
                .iter()
                .any(|p| p.contains("`serde` in [dependencies]"))
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("`local` in [dependencies]"))
        );
    }

    #[test]
    fn an_edge_the_layer_does_not_allow_is_refused() {
        let root = "[workspace.dependencies]\nb = { path = \"crates/b\" }\n";
        let members = [
            member("a", "[dependencies]\nb.workspace = true\n"),
            member("b", ""),
        ];
        let layers = [layer("a", &[]), layer("b", &[])];
        let problems = check_workspace(&table(root), &members, &layers);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("`a` may not depend on `b`"));
    }

    #[test]
    fn a_crate_that_does_not_own_a_dependency_may_not_use_it() {
        let root = "[workspace.dependencies]
rhai = \"1\"
";
        let members = [member(
            "a",
            "[dependencies]
rhai.workspace = true
",
        )];
        let problems = check_workspace(&table(root), &members, &[layer("a", &[])]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("only wyck-indicators may use `rhai`"));
    }

    #[test]
    fn a_root_dependency_nobody_uses_is_reported() {
        let members = [member("a", "")];
        let problems = check_workspace(&table(ROOT), &members, &[layer("a", &[])]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("`serde` is in [workspace.dependencies]"));
    }
}
