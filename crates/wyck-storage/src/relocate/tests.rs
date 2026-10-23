use super::copying::{copy_tree, verify};
use super::staging::{Staged, StagedChange};
use super::tree::within;
use super::*;
use crate::DocumentStore;
use crate::build_info::BuildMode;
use crate::error::ConfigError;
use crate::locations::{self, Kind, Locations};
use crate::paths::AppPaths;
use std::fs;
use std::path::{Path, PathBuf};

const MODE: BuildMode = BuildMode::Production;

struct Setup {
    _temp: tempfile::TempDir,
    base: PathBuf,
    cloud: PathBuf,
}

fn setup() -> Setup {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("config");
    let cloud = temp.path().join("cloud");
    fs::create_dir_all(&base).unwrap();
    fs::create_dir_all(&cloud).unwrap();
    Setup {
        _temp: temp,
        base,
        cloud,
    }
}

/// The paths as the next start finds them: the standard ones with the saved choices.
fn paths(setup: &Setup) -> AppPaths {
    AppPaths::at_for(MODE, &setup.base).with_locations(Locations::load(&setup.base).unwrap())
}

fn move_to(kind: Kind, dir: &Path) -> Change {
    Change {
        kind,
        to: Some(dir.to_path_buf()),
    }
}

fn save(store: &DocumentStore, name: &str, text: &str) {
    store.save_text(name, text).unwrap();
}

#[test]
fn the_files_are_copied_checked_and_the_old_folder_is_kept_aside() {
    let setup = setup();
    let before = paths(&setup);
    save(&before.documents(), "preferences", "magnet = true\n");
    save(&before.scope("demo-1"), "alerts", "next_id = 4\n");
    let target = setup.cloud.join("settings");

    let steps = stage(&before, &[move_to(Kind::Settings, &target)]).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(
        steps[0].action,
        Action::Copy {
            files: 1,
            bytes: 14
        }
    );
    assert!(pending(&before).is_some());

    let applied = apply_pending(&before, "2026-10-12");
    assert_eq!(applied.copied, [Kind::Settings]);
    assert!(applied.failed.is_empty() && applied.refused.is_none());
    assert!(pending(&before).is_none(), "applied once");

    let after = paths(&setup);
    assert_eq!(after.dir(Kind::Settings), target);
    assert!(after.is_custom(Kind::Settings) && !after.is_custom(Kind::Accounts));
    assert_eq!(
        after
            .documents()
            .load_text("preferences")
            .unwrap()
            .as_deref(),
        Some("magnet = true\n")
    );
    assert!(target.join(locations::MARKER).is_file(), "marked as Wyck's");
    assert_eq!(
        after
            .scope("demo-1")
            .load_text("alerts")
            .unwrap()
            .as_deref(),
        Some("next_id = 4\n"),
        "the kinds not moved are where they were"
    );

    let left = leftovers(&after);
    assert_eq!(left.len(), 1);
    assert!(
        left[0].path.ends_with("state.moved-2026-10-12"),
        "{:?}",
        left[0].path
    );
    assert_eq!(left[0].files, 1);
    assert!(
        !before.standard_dir(Kind::Settings).exists(),
        "the old name is free"
    );
}

#[test]
fn a_second_computer_uses_what_the_first_one_put_in_the_cloud() {
    let first = setup();
    let target = first.cloud.join("settings");
    let one = paths(&first);
    save(&one.documents(), "preferences", "magnet = true\n");
    stage(&one, &[move_to(Kind::Settings, &target)]).unwrap();
    apply_pending(&one, "a");

    // Another computer: its own settings, and the same cloud folder.
    let second = setup();
    let two = paths(&second);
    save(&two.documents(), "preferences", "magnet = false\n");
    save(&two.documents(), "extra", "x = 1\n");
    let steps = stage(&two, &[move_to(Kind::Settings, &target)]).unwrap();
    assert_eq!(steps[0].action, Action::Adopt { files: 1 });
    let applied = apply_pending(&two, "b");
    assert_eq!(applied.adopted, [Kind::Settings]);

    let after = paths(&second);
    assert_eq!(
        after
            .documents()
            .load_text("preferences")
            .unwrap()
            .as_deref(),
        Some("magnet = true\n"),
        "the data in the cloud wins"
    );
    assert_eq!(after.documents().load_text("extra").unwrap(), None);
    assert_eq!(
        fs::read_to_string(target.join("preferences.toml")).unwrap(),
        "magnet = true\n",
        "nothing of this computer was copied over it"
    );
    let kept = leftovers(&after);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        fs::read_to_string(kept[0].path.join("preferences.toml")).unwrap(),
        "magnet = false\n",
        "and what this computer had is kept"
    );
}

#[test]
fn nothing_to_carry_is_a_switch_and_the_logs_are_never_carried() {
    let setup = setup();
    let before = paths(&setup);
    let logs = setup.cloud.join("logs");
    fs::create_dir_all(before.logs_dir()).unwrap();
    fs::write(before.logs_dir().join("wyck.2026-10-12.log"), "old line").unwrap();

    let steps = stage(
        &before,
        &[
            move_to(Kind::Indicators, &setup.cloud.join("indicators")),
            move_to(Kind::Logs, &logs),
        ],
    )
    .unwrap();
    assert!(
        steps.iter().all(|step| step.action == Action::Switch),
        "{steps:?}"
    );
    let applied = apply_pending(&before, "s");
    assert_eq!(applied.switched.len(), 2);
    assert_eq!(leftovers(&paths(&setup)), [] as [Leftover; 0]);
    assert!(
        before.logs_dir().join("wyck.2026-10-12.log").is_file(),
        "the old log stays where it was written"
    );
    assert_eq!(paths(&setup).logs_dir(), logs);
}

#[test]
fn going_back_to_the_standard_folder_brings_the_files_home() {
    let setup = setup();
    let target = setup.cloud.join("settings");
    let start = paths(&setup);
    save(&start.documents(), "preferences", "magnet = true\n");
    stage(&start, &[move_to(Kind::Settings, &target)]).unwrap();
    apply_pending(&start, "one");

    let away = paths(&setup);
    save(&away.documents(), "later", "n = 2\n");
    let steps = stage(
        &away,
        &[Change {
            kind: Kind::Settings,
            to: None,
        }],
    )
    .unwrap();
    assert!(steps[0].to_standard);
    let applied = apply_pending(&away, "two");
    assert_eq!(applied.copied, [Kind::Settings]);

    let home = paths(&setup);
    assert!(!home.is_custom(Kind::Settings));
    assert!(
        home.locations().get(Kind::Settings).is_none(),
        "the choice is forgotten"
    );
    assert_eq!(
        home.documents().load_text("later").unwrap().as_deref(),
        Some("n = 2\n")
    );
    assert_eq!(
        home.documents()
            .load_text("preferences")
            .unwrap()
            .as_deref(),
        Some("magnet = true\n")
    );
}

#[test]
fn what_is_not_ours_is_never_mixed_with_and_the_other_build_is_refused() {
    let setup = setup();
    let current = paths(&setup);
    let foreign = setup.cloud.join("holidays");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("photo.jpg"), "x").unwrap();
    assert!(matches!(
        plan(&current, &[move_to(Kind::Settings, &foreign)]),
        Err(PlanError::Folder(Problem {
            reason: locations::Reason::NotEmpty,
            ..
        }))
    ));

    let installed = setup.cloud.join("installed");
    fs::create_dir_all(&installed).unwrap();
    locations::write_marker(&installed, Kind::Settings, BuildMode::Production).unwrap();
    let development = AppPaths::at_for(BuildMode::Development, &setup.base);
    let error = plan(&development, &[move_to(Kind::Settings, &installed)]).unwrap_err();
    assert!(
        error.to_string().contains("belongs to the installed app"),
        "{error}"
    );
    assert!(plan(&current, &[move_to(Kind::Settings, &installed)]).is_ok());
    assert!(
        matches!(
            plan(&current, &[move_to(Kind::Accounts, &installed)]),
            Err(PlanError::Folder(Problem {
                reason: locations::Reason::OtherKind { .. },
                ..
            }))
        ),
        "a folder of one kind is not another's"
    );
}

#[test]
fn the_credentials_and_the_accounts_list_never_go_into_a_folder_you_move() {
    let setup = setup();
    let current = paths(&setup);
    for bad in [
        setup.base.clone(),
        setup.base.parent().unwrap().to_path_buf(),
        current.secrets_dir().join("inner"),
    ] {
        fs::create_dir_all(&bad).unwrap();
        let result = plan(&current, &[move_to(Kind::Backups, &bad)]);
        assert!(
            matches!(result, Err(PlanError::Sensitive(_) | PlanError::Folder(_))),
            "{}: {result:?}",
            bad.display()
        );
    }
    let beside = setup.cloud.join("fine");
    assert!(plan(&current, &[move_to(Kind::Backups, &beside)]).is_ok());
}

#[test]
fn two_kinds_cannot_share_a_folder_or_hold_each_other() {
    let setup = setup();
    let current = paths(&setup);
    let shared = setup.cloud.join("everything");
    let both = [
        move_to(Kind::Settings, &shared),
        move_to(Kind::Accounts, &shared),
    ];
    assert!(matches!(
        plan(&current, &both),
        Err(PlanError::Mixed {
            a: Kind::Settings,
            b: Kind::Accounts
        })
    ));
    fs::create_dir_all(&shared).unwrap();
    let inside = [
        move_to(Kind::Settings, &shared),
        move_to(Kind::Accounts, &shared.join("accounts")),
    ];
    assert!(matches!(
        plan(&current, &inside),
        Err(PlanError::Mixed { .. })
    ));

    let apart = [
        move_to(Kind::Settings, &setup.cloud.join("a")),
        move_to(Kind::Accounts, &setup.cloud.join("b")),
    ];
    assert_eq!(plan(&current, &apart).unwrap().len(), 2);
}

#[test]
fn a_kind_already_there_is_left_out_and_nothing_is_staged_for_it() {
    let setup = setup();
    let current = paths(&setup);
    let standard = current.standard_dir(Kind::Settings).to_path_buf();
    assert_eq!(
        plan(&current, &[move_to(Kind::Settings, &standard)]).unwrap(),
        [] as [Step; 0]
    );
    let nothing = stage(
        &current,
        &[Change {
            kind: Kind::Backups,
            to: None,
        }],
    )
    .unwrap();
    assert_eq!(nothing, [] as [Step; 0]);
    assert!(pending(&current).is_none(), "no file for nothing to do");
}

#[test]
fn a_move_that_cannot_be_read_is_set_aside_and_one_that_cannot_be_made_is_dropped() {
    let setup = setup();
    let current = paths(&setup);
    fs::write(setup.base.join(PENDING), "changes = [[[").unwrap();
    let applied = apply_pending(&current, "x");
    assert!(applied.refused.is_some());
    assert!(setup.base.join(format!("{PENDING}.bad")).is_file());
    assert!(!setup.base.join(PENDING).exists());

    // The folder was made by the other build between the request and the start.
    let installed = setup.cloud.join("other");
    fs::create_dir_all(&installed).unwrap();
    locations::write_marker(&installed, Kind::Settings, BuildMode::Development).unwrap();
    let staged = Staged {
        schema_version: 1,
        changes: vec![StagedChange {
            kind: "settings".into(),
            to: Some(installed),
        }],
    };
    fs::write(setup.base.join(PENDING), toml::to_string(&staged).unwrap()).unwrap();
    let applied = apply_pending(&current, "y");
    assert!(
        applied
            .refused
            .as_deref()
            .unwrap()
            .contains("belongs to a development build")
    );
    assert!(
        Locations::load(&setup.base).unwrap().settings.is_none(),
        "nothing switched"
    );
    assert!(!setup.base.join(PENDING).exists());
    assert!(
        apply_pending(&current, "z").is_empty(),
        "nothing waits any more"
    );
}

#[test]
fn a_copy_skips_what_is_not_data_and_is_read_back() {
    let setup = setup();
    let from = setup.base.join("state");
    fs::create_dir_all(from.join("sounds")).unwrap();
    fs::write(from.join("a.toml"), "a = 1").unwrap();
    fs::write(from.join("sounds").join("bell.wav"), vec![7_u8; 3000]).unwrap();
    fs::write(from.join(".a.toml.tmp-9"), "half a write").unwrap();
    fs::write(from.join(locations::MARKER), "x").unwrap();
    let to = setup.cloud.join("copy");
    fs::create_dir_all(&to).unwrap();

    let mut made = Vec::new();
    copy_tree(&from, &to, &mut made).unwrap();
    verify(&from, &to).unwrap();

    assert_eq!(made.len(), 2);
    assert!(to.join("sounds").join("bell.wav").is_file());
    assert!(!to.join(".a.toml.tmp-9").exists() && !to.join(locations::MARKER).exists());

    fs::write(to.join("a.toml"), "a = 2").unwrap();
    assert!(
        matches!(verify(&from, &to), Err(ConfigError::Relocate { .. })),
        "same size, other bytes"
    );
    fs::write(to.join("a.toml"), "longer").unwrap();
    assert!(
        matches!(verify(&from, &to), Err(ConfigError::Relocate { .. })),
        "other size"
    );
    fs::remove_file(to.join("a.toml")).unwrap();
    assert!(
        matches!(verify(&from, &to), Err(ConfigError::Relocate { .. })),
        "missing"
    );
}

#[test]
fn only_a_folder_a_move_left_behind_can_be_deleted_and_never_one_in_use() {
    let setup = setup();
    let start = paths(&setup);
    save(&start.documents(), "preferences", "magnet = true\n");
    stage(
        &start,
        &[move_to(Kind::Settings, &setup.cloud.join("settings"))],
    )
    .unwrap();
    apply_pending(&start, "d");
    let now = paths(&setup);
    let left = leftovers(&now).remove(0).path;

    let stranger = setup.cloud.join("keep-me");
    fs::create_dir_all(&stranger).unwrap();
    assert!(matches!(
        delete_leftover(&now, &stranger),
        Err(ConfigError::Relocate { .. })
    ));
    assert!(stranger.is_dir());

    delete_leftover(&now, &left).unwrap();
    assert!(!left.exists());
    assert_eq!(leftovers(&paths(&setup)), [] as [Leftover; 0]);
    assert_eq!(
        Locations::load(&setup.base).unwrap().leftovers,
        [] as [PathBuf; 0]
    );
    assert!(
        paths(&setup)
            .dir(Kind::Settings)
            .join("preferences.toml")
            .is_file(),
        "the data in use is untouched"
    );
}

#[test]
fn a_reset_empties_a_moved_folder_but_keeps_the_folder_and_its_marker() {
    use crate::backup::staged;
    use crate::backup::store::BackupKind;

    let setup = setup();
    let start = paths(&setup);
    save(&start.documents(), "preferences", "magnet = true\n");
    let target = setup.cloud.join("settings");
    stage(&start, &[move_to(Kind::Settings, &target)]).unwrap();
    apply_pending(&start, "m");

    let now = paths(&setup);
    staged::stage_reset(&now).unwrap();
    let applied = staged::apply_pending(&now, "2026-10-12").unwrap();

    assert!(applied.reset);
    assert!(target.is_dir(), "the folder a service syncs is not removed");
    assert!(
        target.join(locations::MARKER).is_file(),
        "and is still marked as Wyck's"
    );
    assert!(!target.join("preferences.toml").exists());
    let kinds: Vec<_> = now
        .backups()
        .list()
        .unwrap()
        .iter()
        .map(|e| e.kind)
        .collect();
    assert!(kinds.contains(&BackupKind::BeforeReset), "{kinds:?}");
}

#[test]
fn a_path_is_compared_by_the_place_it_names() {
    let setup = setup();
    let real = setup.cloud.join("real");
    fs::create_dir_all(&real).unwrap();
    assert!(
        within(&real.join("a").join("b"), &real),
        "even where nothing exists yet"
    );
    assert!(within(&real, &real));
    assert!(!within(&real, &real.join("a")));
    assert!(
        within(&real.join("x").join("..").join("y"), &real),
        "a `..` cancels a folder that is not there yet"
    );
    assert!(
        !within(&real.join("..").join("elsewhere"), &real),
        "and one that leads out of the folder is out"
    );
    if cfg!(windows) {
        let upper = PathBuf::from(real.to_string_lossy().to_uppercase());
        assert!(
            within(&upper, &real),
            "Windows does not tell capitals apart"
        );
    }
}
