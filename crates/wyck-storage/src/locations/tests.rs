use super::cloud::cloud_folders_with;
use super::folder::{resolve, write_marker};
use super::*;
use crate::error::ConfigError;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use wyck_platform::build_info::BuildMode;

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    }
}

#[test]
fn the_pointer_is_saved_and_read_back_and_an_empty_one_is_the_default() {
    let temp = dir();
    assert_eq!(Locations::load(temp.path()).unwrap(), Locations::default());

    let mut locations = Locations::default();
    locations.set(Kind::Settings, Some(PathBuf::from("/cloud/Wyck/settings")));
    locations.set(Kind::Logs, Some(PathBuf::from("/disk/logs")));
    locations
        .leftovers
        .push(PathBuf::from("/old/state.moved-1"));
    locations.save(temp.path()).unwrap();

    let back = Locations::load(temp.path()).unwrap();
    assert_eq!(back, locations);
    assert_eq!(
        back.get(Kind::Settings),
        Some(Path::new("/cloud/Wyck/settings"))
    );
    assert_eq!(back.get(Kind::Accounts), None);
    let text = fs::read_to_string(temp.path().join(FILE)).unwrap();
    assert!(
        !text.contains("accounts"),
        "a kind left alone is not written: {text}"
    );
}

#[test]
fn a_damaged_or_newer_pointer_is_reported_and_left_alone() {
    let temp = dir();
    fs::write(temp.path().join(FILE), "settings = [[[").unwrap();
    assert!(matches!(
        Locations::load(temp.path()),
        Err(ConfigError::Parse { .. })
    ));
    assert_eq!(
        fs::read_to_string(temp.path().join(FILE)).unwrap(),
        "settings = [[["
    );

    fs::write(
        temp.path().join(FILE),
        "schema_version = 9\nfrom_later = 1\n",
    )
    .unwrap();
    assert!(matches!(
        Locations::load(temp.path()),
        Err(ConfigError::UnsupportedSchema { found: 9, .. })
    ));
}

#[test]
fn every_kind_has_its_own_name_and_only_the_logs_stay_out_of_the_cloud() {
    let codes: std::collections::HashSet<_> = Kind::ALL.iter().map(|k| k.code()).collect();
    assert_eq!(codes.len(), Kind::ALL.len());

    assert_eq!(Kind::ALL.iter().filter(|k| k.in_cloud_preset()).count(), 4);
    assert!(!Kind::Logs.in_cloud_preset() && !Kind::Logs.carries_files());
    assert!(Kind::Settings.carries_files());
}

#[test]
fn a_folder_must_be_a_full_path_to_a_folder_that_can_be_written() {
    let temp = dir();
    let mode = BuildMode::Production;
    let problem = |dir: &Path, how| check(dir, Kind::Settings, mode, how).unwrap_err().reason;

    assert_eq!(
        problem(Path::new("relative/dir"), Use::Existing),
        Reason::NotAbsolute
    );
    assert_eq!(
        problem(&temp.path().join("nothing"), Use::Existing),
        Reason::Missing
    );
    let file = temp.path().join("file");
    fs::write(&file, "x").unwrap();
    assert_eq!(problem(&file, Use::Existing), Reason::NotAFolder);
    assert_eq!(
        problem(&file.join("below"), Use::Chosen),
        Reason::NotAFolder,
        "nothing can be made under a file"
    );
    assert_eq!(
        check(
            &temp.path().join("a").join("b"),
            Kind::Settings,
            mode,
            Use::Chosen
        )
        .unwrap(),
        Checked { has_data: false },
        "a chosen folder may be several levels below one that exists"
    );

    assert_eq!(
        check(temp.path(), Kind::Settings, mode, Use::Existing).unwrap(),
        Checked { has_data: true },
        "the temporary folder holds the file made above"
    );
    assert_eq!(
        check(&temp.path().join("new"), Kind::Settings, mode, Use::Chosen).unwrap(),
        Checked { has_data: false },
        "a folder to be made is fine"
    );
}

#[test]
fn a_chosen_folder_must_be_empty_or_made_by_wyck() {
    let temp = dir();
    let mode = BuildMode::Production;
    let empty = temp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    assert_eq!(
        check(&empty, Kind::Settings, mode, Use::Chosen).unwrap(),
        Checked { has_data: false }
    );

    let foreign = temp.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("holiday.jpg"), "x").unwrap();
    let problem = check(&foreign, Kind::Settings, mode, Use::Chosen).unwrap_err();
    assert_eq!(problem.reason, Reason::NotEmpty);
    assert!(problem.to_string().contains("mixed"), "{problem}");
    assert_eq!(
        target_in(&foreign, Kind::Settings, mode),
        foreign.join("settings"),
        "files that are not ours are never mixed with"
    );
    assert_eq!(target_in(&empty, Kind::Settings, mode), empty);

    write_marker(&foreign, Kind::Settings, mode).unwrap();
    assert_eq!(
        check(&foreign, Kind::Settings, mode, Use::Chosen).unwrap(),
        Checked { has_data: true },
        "a folder made by Wyck can be taken again"
    );
}

#[test]
fn the_folder_of_the_other_build_or_of_another_kind_is_refused() {
    let temp = dir();
    write_marker(temp.path(), Kind::Settings, BuildMode::Production).unwrap();

    let development = check(
        temp.path(),
        Kind::Settings,
        BuildMode::Development,
        Use::Existing,
    )
    .unwrap_err();
    assert_eq!(
        development.reason,
        Reason::OtherBuild {
            found: "production".into()
        }
    );
    assert!(
        development
            .to_string()
            .contains("belongs to the installed app"),
        "{development}"
    );
    let production = {
        let other = dir();
        write_marker(other.path(), Kind::Settings, BuildMode::Development).unwrap();
        check(
            other.path(),
            Kind::Settings,
            BuildMode::Production,
            Use::Chosen,
        )
        .unwrap_err()
    };
    assert!(
        production
            .to_string()
            .contains("belongs to a development build")
    );

    let kind = check(
        temp.path(),
        Kind::Accounts,
        BuildMode::Production,
        Use::Existing,
    )
    .unwrap_err();
    assert_eq!(
        kind.reason,
        Reason::OtherKind {
            found: "settings".into()
        }
    );
    assert!(
        check(
            temp.path(),
            Kind::Settings,
            BuildMode::Production,
            Use::Existing
        )
        .is_ok(),
        "its own build and kind pass"
    );
}

#[test]
fn a_folder_that_cannot_be_used_leaves_the_kind_in_its_standard_place_and_says_why() {
    let temp = dir();
    let standard = temp.path().join("standard");
    let mut locations = Locations::default();
    locations.set(Kind::Settings, Some(temp.path().join("gone")));
    let mut problems = Vec::new();

    let used = resolve(
        &locations,
        Kind::Settings,
        &standard,
        BuildMode::Production,
        &mut problems,
    );
    assert_eq!(used, standard);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].reason, Reason::Missing);

    // A kind with no choice is not a problem.
    let used = resolve(
        &locations,
        Kind::Accounts,
        &standard,
        BuildMode::Production,
        &mut problems,
    );
    assert_eq!(used, standard);
    assert_eq!(problems.len(), 1);

    // A folder that is there is used.
    let chosen = temp.path().join("chosen");
    fs::create_dir(&chosen).unwrap();
    locations.set(Kind::Accounts, Some(chosen.clone()));
    let used = resolve(
        &locations,
        Kind::Accounts,
        &standard,
        BuildMode::Production,
        &mut problems,
    );
    assert_eq!(used, chosen);
    assert_eq!(problems.len(), 1);
}

#[test]
fn the_cloud_folders_of_the_environment_and_of_the_home_folder_are_found_once_each() {
    let temp = dir();
    let home = temp.path().join("home");
    for name in [
        "OneDrive",
        "OneDrive - Contoso",
        "Dropbox",
        "Documents",
        "Library/CloudStorage/GoogleDrive-me",
        "Library/Mobile Documents/com~apple~CloudDocs",
    ] {
        fs::create_dir_all(home.join(name)).unwrap();
    }
    fs::write(home.join("OneDrive notes.txt"), "a file, not a folder").unwrap();
    let missing = home.join("OneDriveGhost");
    let consumer = home.join("OneDrive");
    let found = cloud_folders_with(
        env(&[
            ("OneDriveConsumer", consumer.to_str().unwrap()),
            ("OneDrive", consumer.to_str().unwrap()),
            ("OneDriveCommercial", missing.to_str().unwrap()),
        ]),
        Some(&home),
    );

    let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names.iter().filter(|name| **name == "OneDrive").count(),
        1,
        "named by the environment and found in the home folder, listed once: {names:?}"
    );
    for wanted in [
        "OneDrive - Contoso",
        "Dropbox",
        "GoogleDrive-me",
        "com~apple~CloudDocs",
    ] {
        assert!(names.contains(&wanted), "{wanted} in {names:?}");
    }
    assert!(!names.contains(&"Documents") && !names.contains(&"OneDriveGhost"));
    assert_eq!(cloud_folders_with(env(&[]), None), [] as [CloudFolder; 0]);
}

#[test]
fn a_development_build_has_its_own_folder_in_the_cloud() {
    let root = Path::new("/cloud");
    let production = cloud_layout(root, BuildMode::Production);
    let development = cloud_layout(root, BuildMode::Development);
    assert_eq!(production.len(), 4);
    assert!(
        production
            .iter()
            .all(|(_, path)| path.starts_with("/cloud/Wyck"))
    );
    assert!(
        development
            .iter()
            .all(|(_, path)| path.starts_with("/cloud/Wyck (dev)"))
    );
    assert!(!production.iter().any(|(kind, _)| *kind == Kind::Logs));
    assert_eq!(
        production[0],
        (Kind::Settings, PathBuf::from("/cloud/Wyck/settings"))
    );
}

#[test]
fn copies_a_sync_service_makes_are_found_and_the_documents_are_not() {
    let temp = dir();
    for name in [
        "preferences.toml",
        "alerts.toml",
        "preferences-LAPTOP.toml",
        "preferences (1).toml",
        "alerts (conflicted copy 2026-10-12).toml",
        "preferences.sync-conflict-20261012-101010-ABCDEFG.toml",
        "preferences.toml.v1.bak",
        "preferences.toml.bad",
        "notes.toml",
        "indicator_editor_drafts.toml",
    ] {
        fs::write(temp.path().join(name), "x").unwrap();
    }
    let found: Vec<String> = conflict_copies(temp.path(), &["preferences", "alerts"])
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        found,
        [
            "alerts (conflicted copy 2026-10-12).toml",
            "preferences (1).toml",
            "preferences-LAPTOP.toml",
            "preferences.sync-conflict-20261012-101010-ABCDEFG.toml",
        ]
    );
    assert_eq!(
        conflict_copies(&temp.path().join("nothing"), &["preferences"]),
        [] as [PathBuf; 0]
    );
}
