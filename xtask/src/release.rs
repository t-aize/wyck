//! The steps of the release pipeline that are written in Rust so that they can be tested:
//! naming the packages cargo-packager made, and writing the manifest of the updater and the
//! checksums. `.github/workflows/release.yml` runs them.
//!
//! Every package comes with a `.sig` file, the signature of the updater. A release goes out only
//! if every package of every platform is there and signed.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use chrono::{SecondsFormat, Utc};
use clap::{Subcommand, ValueEnum};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use semver::Version;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Subcommand)]
pub(crate) enum Step {
    /// Give the packages cargo-packager wrote their release names, after checking their
    /// signatures.
    NameAssets {
        /// The folder cargo-packager wrote to (it is searched in depth).
        #[arg(long)]
        input: PathBuf,
        #[arg(long, value_enum)]
        platform: Platform,
        /// The version being released, `1.2.3`.
        #[arg(long)]
        release_version: String,
    },
    /// Check every package and signature of the release, then write `latest.json`, the manifest
    /// the updater reads, and `SHA256SUMS`.
    Manifest {
        /// The folder holding the named packages of every platform.
        #[arg(long)]
        assets: PathBuf,
        /// The tag of the release, `v1.2.3`.
        #[arg(long)]
        tag: String,
        /// `owner/name` of the GitHub repository.
        #[arg(long)]
        repository: String,
        /// The release notes, copied into the manifest.
        #[arg(long)]
        notes: PathBuf,
    },
}

pub(crate) fn run(step: Step) -> Result<()> {
    match step {
        Step::NameAssets {
            input,
            platform,
            release_version,
        } => name_assets(&input, platform, &release_version),
        Step::Manifest {
            assets,
            tag,
            repository,
            notes,
        } => manifest(&assets, &tag, &repository, &notes),
    }
}

/// A system the app is released for.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Platform {
    #[value(name = "windows-x86_64")]
    WindowsX86_64,
    #[value(name = "macos-x86_64")]
    MacosX86_64,
    #[value(name = "macos-aarch64")]
    MacosAarch64,
    #[value(name = "linux-x86_64")]
    LinuxX86_64,
}

impl Platform {
    /// The key of the platform in `latest.json`: the operating system and the architecture the way
    /// cargo-packager-updater names them.
    fn key(self) -> &'static str {
        match self {
            Self::WindowsX86_64 => "windows-x86_64",
            Self::MacosX86_64 => "macos-x86_64",
            Self::MacosAarch64 => "macos-aarch64",
            Self::LinuxX86_64 => "linux-x86_64",
        }
    }
}

/// One file of a release.
struct Package {
    platform: Platform,
    /// What the file cargo-packager wrote ends with.
    suffix: &'static str,
    /// The name it gets in the release, where `{version}` is the version.
    name: &'static str,
    /// How the updater installs it, when it is the file the updater downloads.
    updater: Option<&'static str>,
}

impl Package {
    fn file_name(&self, version: &str) -> String {
        self.name.replace("{version}", version)
    }
}

/// Every file a release holds besides the manifest and the checksums.
const PACKAGES: &[Package] = &[
    Package {
        platform: Platform::WindowsX86_64,
        suffix: ".exe",
        name: "Wyck_{version}_windows-x86_64-setup.exe",
        updater: Some("nsis"),
    },
    Package {
        platform: Platform::MacosX86_64,
        suffix: ".dmg",
        name: "Wyck_{version}_macos-x86_64.dmg",
        updater: None,
    },
    Package {
        platform: Platform::MacosX86_64,
        suffix: ".app.tar.gz",
        name: "Wyck_{version}_macos-x86_64.app.tar.gz",
        updater: Some("app"),
    },
    Package {
        platform: Platform::MacosAarch64,
        suffix: ".dmg",
        name: "Wyck_{version}_macos-aarch64.dmg",
        updater: None,
    },
    Package {
        platform: Platform::MacosAarch64,
        suffix: ".app.tar.gz",
        name: "Wyck_{version}_macos-aarch64.app.tar.gz",
        updater: Some("app"),
    },
    Package {
        platform: Platform::LinuxX86_64,
        suffix: ".AppImage",
        name: "Wyck_{version}_linux-x86_64.AppImage",
        updater: Some("appimage"),
    },
    Package {
        platform: Platform::LinuxX86_64,
        suffix: ".deb",
        name: "Wyck_{version}_linux-x86_64.deb",
        updater: None,
    },
];

const MANIFEST: &str = "latest.json";
const CHECKSUMS: &str = "SHA256SUMS";

/// The version of a release tag, `v1.2.3`: a stable version, with nothing after the patch number.
fn release_version(tag: &str) -> Result<Version> {
    let text = tag
        .strip_prefix('v')
        .with_context(|| format!("the release tag `{tag}` does not start with `v`"))?;
    let version = Version::parse(text)
        .with_context(|| format!("the release tag `{tag}` is not a version"))?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "the release tag `{tag}` is not a stable version: use `vX.Y.Z`"
    );
    Ok(version)
}

/// `owner/name`, written with the characters GitHub allows in them.
fn check_repository(repository: &str) -> Result<()> {
    let allowed = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    let valid = repository
        .split_once('/')
        .is_some_and(|(owner, name)| allowed(owner) && allowed(name));
    ensure!(
        valid,
        "the repository `{repository}` is not of the form owner/name"
    );
    Ok(())
}

fn name_assets(input: &Path, platform: Platform, version: &str) -> Result<()> {
    let version = Version::parse(version)
        .with_context(|| format!("`{version}` is not a version"))?
        .to_string();
    fs::create_dir_all(input).with_context(|| format!("creating {}", input.display()))?;
    for package in PACKAGES.iter().filter(|p| p.platform == platform) {
        let source = find_one(input, package.suffix)?;
        let destination = input.join(package.file_name(&version));
        signature_of(&source)?;
        if source == destination {
            // Already named: a run that is repeated must not fail on its own work.
            continue;
        }
        let (from, to) = (signature_path(&source), signature_path(&destination));
        ensure!(
            !destination.exists() && !to.exists(),
            "refusing to overwrite {}",
            destination.display()
        );
        fs::rename(&source, &destination)
            .with_context(|| format!("renaming {}", source.display()))?;
        fs::rename(&from, &to).with_context(|| format!("renaming {}", from.display()))?;
        println!(
            "{} -> {}",
            source.file_name().unwrap_or_default().to_string_lossy(),
            package.file_name(&version)
        );
    }
    Ok(())
}

/// The one file under `root` that ends with `suffix`, not counting signatures.
fn find_one(root: &Path, suffix: &str) -> Result<PathBuf> {
    let mut found = Vec::new();
    collect_files(root, &mut found)?;
    let mut matches: Vec<PathBuf> = found
        .into_iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.ends_with(suffix) && !name.ends_with(".sig")
            })
        })
        .collect();
    matches.sort();
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!(
            "expected one *{suffix} package in {}, found none",
            root.display()
        ),
        many => bail!(
            "expected one *{suffix} package in {}, found: {}",
            root.display(),
            many.iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn signature_path(package: &Path) -> PathBuf {
    let mut name = package.as_os_str().to_owned();
    name.push(".sig");
    PathBuf::from(name)
}

/// The text of the signature of `package`: its `.sig` file must exist and say something.
fn signature_of(package: &Path) -> Result<String> {
    let path = signature_path(package);
    let name = package.file_name().unwrap_or_default().to_string_lossy();
    ensure!(path.is_file(), "missing signature for {name}");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let text = text.trim();
    ensure!(!text.is_empty(), "empty signature for {name}");
    Ok(text.to_owned())
}

/// What the updater reads to know whether a newer version exists and where to get it.
#[derive(Serialize)]
struct Manifest {
    version: String,
    notes: String,
    pub_date: String,
    platforms: BTreeMap<&'static str, Entry>,
}

#[derive(Serialize)]
struct Entry {
    signature: String,
    url: String,
    format: &'static str,
}

/// The characters a file name keeps in a URL: letters, digits and `-._~`.
const URL_NAME: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

fn manifest(assets: &Path, tag: &str, repository: &str, notes: &Path) -> Result<()> {
    let version = release_version(tag)?.to_string();
    check_repository(repository)?;
    fs::create_dir_all(assets).with_context(|| format!("creating {}", assets.display()))?;

    // Every package of every platform must be there and signed, whether the updater uses it or
    // not. Nothing is written before all of them have been seen.
    let base = format!("https://github.com/{repository}/releases/download/{tag}");
    let mut platforms = BTreeMap::new();
    for package in PACKAGES {
        let name = package.file_name(&version);
        let path = assets.join(&name);
        ensure!(path.is_file(), "missing release package: {name}");
        let signature = signature_of(&path)?;
        if let Some(format) = package.updater {
            platforms.insert(
                package.platform.key(),
                Entry {
                    signature,
                    url: format!("{base}/{}", utf8_percent_encode(&name, URL_NAME)),
                    format,
                },
            );
        }
    }

    let manifest = Manifest {
        version,
        notes: fs::read_to_string(notes)
            .with_context(|| format!("reading {}", notes.display()))?
            .trim()
            .to_owned(),
        pub_date: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        platforms,
    };
    let text = serde_json::to_string_pretty(&manifest)? + "\n";
    fs::write(assets.join(MANIFEST), text).context("writing the manifest")?;
    write_checksums(assets)?;
    println!("wrote {MANIFEST} and {CHECKSUMS} in {}", assets.display());
    Ok(())
}

/// `SHA256SUMS`, in the format `sha256sum --check` reads, for every file of the folder.
fn write_checksums(dir: &Path) -> Result<()> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file() && path.file_name().is_some_and(|name| name != CHECKSUMS))
        .collect();
    files.sort();
    let mut text = String::new();
    for path in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        text.push_str(&format!("{}  {name}\n", sha256(&path)?));
    }
    fs::write(dir.join(CHECKSUMS), text).context("writing the checksums")
}

fn sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut block = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut block)
            .with_context(|| format!("reading {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&block[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = "1.2.3";

    /// Writes `name` and its signature in `dir`.
    fn write_signed(dir: &Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), format!("package {name}")).unwrap();
        fs::write(
            dir.join(format!("{name}.sig")),
            format!("signature of {name}\n"),
        )
        .unwrap();
    }

    /// Every package of the release, named as released.
    fn full_release(dir: &Path) {
        for package in PACKAGES {
            write_signed(dir, &package.file_name(VERSION));
        }
    }

    #[test]
    fn only_a_stable_version_is_a_release_tag() {
        assert_eq!(release_version("v1.2.3").unwrap().to_string(), "1.2.3");
        assert_eq!(release_version("v0.10.0").unwrap().to_string(), "0.10.0");
        for bad in [
            "1.2.3",
            "v1.2",
            "v01.2.3",
            "v1.2.3-rc.1",
            "v1.2.3+build",
            "vx",
            "",
        ] {
            assert!(release_version(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn a_repository_is_owner_slash_name() {
        assert!(check_repository("t-aize/wyck").is_ok());
        for bad in [
            "wyck",
            "a/b/c",
            "/wyck",
            "owner/",
            "owner/na me",
            "owner/..%2f",
        ] {
            assert!(check_repository(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn a_package_found_in_depth_is_renamed_with_its_signature() {
        let dir = tempfile::tempdir().unwrap();
        write_signed(&dir.path().join("nsis"), "wyck_1.2.3_x64-setup.exe");
        name_assets(dir.path(), Platform::WindowsX86_64, VERSION).unwrap();
        let named = dir.path().join("Wyck_1.2.3_windows-x86_64-setup.exe");
        assert!(named.is_file());
        assert_eq!(
            fs::read_to_string(signature_path(&named)).unwrap(),
            "signature of wyck_1.2.3_x64-setup.exe\n"
        );
        assert!(!dir.path().join("nsis/wyck_1.2.3_x64-setup.exe").exists());
    }

    #[test]
    fn naming_the_assets_twice_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write_signed(&dir.path().join("out"), "Wyck.AppImage");
        write_signed(&dir.path().join("out"), "wyck.deb");
        name_assets(dir.path(), Platform::LinuxX86_64, VERSION).unwrap();
        name_assets(dir.path(), Platform::LinuxX86_64, VERSION).unwrap();
        assert!(
            dir.path()
                .join("Wyck_1.2.3_linux-x86_64.AppImage")
                .is_file()
        );
        assert!(dir.path().join("Wyck_1.2.3_linux-x86_64.deb.sig").is_file());
    }

    #[test]
    fn a_package_that_is_unsigned_missing_or_doubled_stops_the_release() {
        // No signature.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.exe"), "x").unwrap();
        let error = name_assets(dir.path(), Platform::WindowsX86_64, VERSION).unwrap_err();
        assert!(error.to_string().contains("missing signature"), "{error}");

        // An empty signature.
        fs::write(dir.path().join("a.exe.sig"), " \n").unwrap();
        let error = name_assets(dir.path(), Platform::WindowsX86_64, VERSION).unwrap_err();
        assert!(error.to_string().contains("empty signature"), "{error}");

        // Two candidates.
        write_signed(&dir.path().join("one"), "b.exe");
        let error = name_assets(dir.path(), Platform::WindowsX86_64, VERSION).unwrap_err();
        assert!(error.to_string().contains("found:"), "{error}");

        // None.
        let empty = tempfile::tempdir().unwrap();
        let error = name_assets(empty.path(), Platform::WindowsX86_64, VERSION).unwrap_err();
        assert!(error.to_string().contains("found none"), "{error}");
    }

    #[test]
    fn a_leftover_signature_under_the_release_name_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        write_signed(&dir.path().join("out"), "x.exe");
        fs::write(
            dir.path().join("Wyck_1.2.3_windows-x86_64-setup.exe.sig"),
            "an old signature",
        )
        .unwrap();
        let error = name_assets(dir.path(), Platform::WindowsX86_64, VERSION).unwrap_err();
        assert!(
            error.to_string().contains("refusing to overwrite"),
            "{error}"
        );
        assert!(dir.path().join("out/x.exe").is_file(), "nothing was moved");
    }
    #[test]
    fn the_manifest_names_the_files_the_updater_downloads() {
        let dir = tempfile::tempdir().unwrap();
        full_release(dir.path());
        let notes = dir.path().join("notes.md");
        fs::write(&notes, "\nWhat changed.\n\n").unwrap();
        // The notes file sits in the folder of the assets, as in the pipeline.
        manifest(dir.path(), "v1.2.3", "t-aize/wyck", &notes).unwrap();

        let text = fs::read_to_string(dir.path().join(MANIFEST)).unwrap();
        assert!(text.ends_with("}\n"));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["version"], "1.2.3");
        assert_eq!(json["notes"], "What changed.");
        assert!(json["pub_date"].as_str().unwrap().ends_with('Z'));
        let platforms = json["platforms"].as_object().unwrap();
        let keys: Vec<&str> = platforms.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "linux-x86_64",
                "macos-aarch64",
                "macos-x86_64",
                "windows-x86_64"
            ]
        );
        let windows = &platforms["windows-x86_64"];
        assert_eq!(windows["format"], "nsis");
        assert_eq!(
            windows["url"],
            "https://github.com/t-aize/wyck/releases/download/v1.2.3/Wyck_1.2.3_windows-x86_64-setup.exe"
        );
        assert_eq!(
            windows["signature"],
            "signature of Wyck_1.2.3_windows-x86_64-setup.exe"
        );
        assert_eq!(platforms["macos-aarch64"]["format"], "app");
        assert_eq!(platforms["linux-x86_64"]["format"], "appimage");
    }

    #[test]
    fn the_checksums_cover_every_file_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "abc").unwrap();
        fs::write(dir.path().join("b.txt"), "").unwrap();
        fs::create_dir(dir.path().join("subfolder")).unwrap();
        write_checksums(dir.path()).unwrap();
        let sums = fs::read_to_string(dir.path().join(CHECKSUMS)).unwrap();
        assert_eq!(
            sums,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  a.txt\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  b.txt\n"
        );
        // Writing them again does not list the checksum file itself.
        write_checksums(dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join(CHECKSUMS)).unwrap(),
            sums
        );
    }

    #[test]
    fn a_release_missing_a_package_or_a_signature_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        full_release(dir.path());
        let notes = dir.path().join("notes.md");
        fs::write(&notes, "notes").unwrap();

        let deb = dir.path().join("Wyck_1.2.3_linux-x86_64.deb");
        fs::remove_file(signature_path(&deb)).unwrap();
        let error = manifest(dir.path(), "v1.2.3", "t-aize/wyck", &notes).unwrap_err();
        assert!(error.to_string().contains("missing signature"), "{error}");

        fs::remove_file(&deb).unwrap();
        let error = manifest(dir.path(), "v1.2.3", "t-aize/wyck", &notes).unwrap_err();
        assert!(
            error.to_string().contains("missing release package"),
            "{error}"
        );
        assert!(
            !dir.path().join(MANIFEST).exists(),
            "nothing is written on failure"
        );
    }
}
