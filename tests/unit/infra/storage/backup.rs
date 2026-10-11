use super::*;

fn at(dir: &Path) -> AppPaths {
    AppPaths::at(dir)
}

fn write(dir: &Path, relative: &str, content: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn read(dir: &Path, relative: &str) -> String {
    std::fs::read_to_string(dir.join(relative)).unwrap()
}

fn secret(text: &str) -> SecretString {
    SecretString::from(text.to_owned())
}

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "state/preferences.toml", "magnet = true\n");
    write(dir.path(), "state/appearance.toml", "mode = \"light\"\n");
    write(dir.path(), "scopes/demo-1/drawings.toml", "next_id = 4\n");
    write(dir.path(), "scopes/demo-1/watchlists.toml", "x = 1\n");
    write(dir.path(), "scopes/live-2/drawings.toml", "next_id = 9\n");
    // Never in a backup: the app's own config, a damaged copy, and what is not a document.
    write(dir.path(), "config.toml", "client_id = \"secret\"\n");
    write(dir.path(), "state/preferences.toml.bad", "junk");
    write(dir.path(), "state/notes.txt", "hello");
    dir
}

fn collect(dir: &Path) -> Backup {
    Backup::collect(&at(dir), None, "1.2.3", "today").unwrap()
}

#[test]
fn a_backup_holds_every_document_and_nothing_else() {
    let dir = setup();
    let backup = collect(dir.path());
    let listed: Vec<(&str, &str)> = backup
        .files
        .iter()
        .map(|f| (f.scope.as_str(), f.name.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            ("global", "appearance"),
            ("global", "preferences"),
            ("scope:demo-1", "drawings"),
            ("scope:demo-1", "watchlists"),
            ("scope:live-2", "drawings"),
        ]
    );
    assert!(
        !backup.to_text().unwrap().contains("secret"),
        "the config file stays out"
    );
    assert_eq!(
        (backup.app_version.as_str(), backup.created.as_str()),
        ("1.2.3", "today")
    );
    let contents = backup.contents();
    assert_eq!(contents.documents(), 5);
    assert!(contents.has_global("appearance"));
    assert_eq!(contents.scopes_with("drawings"), 2);
    assert_eq!(contents.scopes_with("alerts"), 0);
}

#[test]
fn the_indicator_scripts_travel_in_a_backup_and_come_back() {
    let dir = setup();
    let scripts = ScriptStore::new(dir.path().join("elsewhere"));
    scripts
        .write("Trend/My average", "plot(\"a\", close);")
        .unwrap();
    scripts.write("Other", "plot(\"b\", open);").unwrap();
    let backup = Backup::collect(&at(dir.path()), Some(&scripts), "1", "now").unwrap();
    assert_eq!(backup.scripts.len(), 2);
    let text = backup.to_text().unwrap();
    assert_eq!(Backup::parse(&text).unwrap().scripts, backup.scripts);

    // Applied on another machine: the scripts land in the default folder.
    let other = tempfile::tempdir().unwrap();
    stage_import(&at(other.path()), &text, None).unwrap();
    let applied = apply_pending(&at(other.path()), "s").unwrap();
    assert_eq!(applied.imported, Some(backup.files.len() + 2));
    assert_eq!(
        read(other.path(), "indicators/Trend/My average.rhai"),
        "plot(\"a\", close);"
    );
}

#[test]
fn a_script_that_is_there_and_differs_is_kept_aside_when_a_backup_is_applied() {
    let source = tempfile::tempdir().unwrap();
    let scripts = ScriptStore::in_config(&at(source.path()));
    scripts.write("Mine", "plot(\"new\", close);").unwrap();
    let text = Backup::collect(&at(source.path()), Some(&scripts), "1", "now")
        .unwrap()
        .to_text()
        .unwrap();

    let target = tempfile::tempdir().unwrap();
    write(
        target.path(),
        "indicators/Mine.rhai",
        "plot(\"old\", close);",
    );
    stage_import(&at(target.path()), &text, None).unwrap();
    let applied = apply_pending(&at(target.path()), "s").unwrap();
    assert_eq!(
        read(target.path(), "indicators/Mine.rhai"),
        "plot(\"new\", close);"
    );

    let kept = Backup::read_file(&applied.kept_in.unwrap(), None).unwrap();
    assert_eq!(kept.scripts[0].content, "plot(\"old\", close);");
}

#[test]
fn a_script_with_an_unsafe_place_makes_the_backup_refused() {
    for bad in ["../escape", "a/../b", "con", "C:/x", ".hidden"] {
        let mut backup = Backup::new("", "");
        backup.scripts.push(BackupScript {
            id: bad.to_owned(),
            content: String::new(),
        });
        assert!(Backup::parse(&backup.to_text().unwrap()).is_err(), "{bad}");
    }
}

#[test]
fn a_backup_of_nothing_is_valid_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let backup = collect(dir.path());
    assert!(backup.is_empty());
    let back = Backup::parse(&backup.to_text().unwrap()).unwrap();
    assert!(back.contents().is_empty());
}

#[test]
fn what_is_written_is_read_back() {
    let dir = setup();
    let backup = collect(dir.path());
    assert_eq!(Backup::parse(&backup.to_text().unwrap()).unwrap(), backup);
}

#[test]
fn a_file_that_is_not_a_backup_or_is_broken_is_refused() {
    assert!(matches!(
        Backup::parse("hello"),
        Err(ConfigError::NotABackup)
    ));
    assert!(matches!(
        Backup::parse("format = \"other\"\nversion = 1\n"),
        Err(ConfigError::NotABackup)
    ));
    assert!(matches!(
        Backup::parse("format = \"wyck-backup\"\nversion = 99\n"),
        Err(ConfigError::BackupTooNew { found: 99, .. })
    ));
    let file = |name: &str, content: &str| {
        format!("[[files]]\nscope = \"global\"\nname = \"{name}\"\ncontent = \"{content}\"\n")
    };
    let head = "format = \"wyck-backup\"\nversion = 1\n";
    let broken = format!("{head}{}", file("a", "not = = toml"));
    assert!(matches!(
        Backup::parse(&broken),
        Err(ConfigError::BackupDamaged(_))
    ));
    let twice = format!("{head}{}{}", file("a", ""), file("a", ""));
    assert!(matches!(
        Backup::parse(&twice),
        Err(ConfigError::BackupDamaged(_))
    ));
}

#[test]
fn a_document_cannot_go_outside_the_config_folder() {
    for (scope, name) in [
        ("global", "../evil"),
        ("global", "a/b"),
        ("global", "..\\evil"),
        ("global", ".hidden"),
        ("global", ""),
        ("scope:../x", "a"),
        ("scope:", "a"),
        ("elsewhere", "a"),
        ("scope:a/b", "a"),
    ] {
        let text = format!(
            "format = \"wyck-backup\"\nversion = 1\n[[files]]\nscope = \"{}\"\nname = \"{}\"\ncontent = \"\"\n",
            scope.replace('\\', "\\\\"),
            name.replace('\\', "\\\\")
        );
        assert!(
            Backup::parse(&text).is_err(),
            "{scope:?} {name:?} was accepted"
        );
    }
}

#[test]
fn a_sealed_backup_needs_its_passphrase_and_says_which_problem_it_has() {
    let dir = setup();
    let backup = collect(dir.path());
    let text = backup.to_text_with(Some(&secret("pw"))).unwrap();
    assert!(sealed::is_sealed(&text));
    assert!(!text.contains("magnet"), "the content is not readable");

    assert!(matches!(
        Backup::read(&text, None),
        Err(ConfigError::PassphraseRequired)
    ));
    assert!(matches!(
        Backup::read(&text, Some(&secret("nope"))),
        Err(ConfigError::WrongPassphrase)
    ));
    assert_eq!(Backup::read(&text, Some(&secret("pw"))).unwrap(), backup);
    // A passphrase given for a plain backup is not needed, and not an error.
    assert_eq!(
        Backup::read(&backup.to_text().unwrap(), Some(&secret("pw"))).unwrap(),
        backup
    );
}

#[test]
fn export_and_import_go_through_a_file_sealed_or_not() {
    let dir = setup();
    let out = tempfile::tempdir().unwrap();
    let plain = out.path().join("plain.toml");
    let locked = out.path().join("deep").join("locked.toml");
    let pw = secret("pw");
    let backup = export_to_file(&at(dir.path()), None, &plain, None, "1", "t").unwrap();
    export_to_file(&at(dir.path()), None, &locked, Some(&pw), "1", "t").unwrap();

    let target = tempfile::tempdir().unwrap();
    assert_eq!(
        stage_import_file(&at(target.path()), &plain, None).unwrap(),
        backup
    );
    assert!(matches!(
        stage_import_file(&at(target.path()), &locked, None),
        Err(ConfigError::PassphraseRequired)
    ));
    assert_eq!(
        stage_import_file(&at(target.path()), &locked, Some(&pw)).unwrap(),
        backup
    );
}

#[test]
fn an_import_waits_and_is_applied_at_the_next_start_keeping_what_it_replaces() {
    let source = setup();
    let text = collect(source.path()).to_text().unwrap();

    // Another machine: one document differs, one is new, one is not in the backup.
    let target = tempfile::tempdir().unwrap();
    write(target.path(), "state/preferences.toml", "magnet = false\n");
    write(target.path(), "state/appearance.toml", "mode = \"light\"\n");
    write(target.path(), "scopes/other/alerts.toml", "keep = true\n");
    let staged = stage_import(&at(target.path()), &text, None).unwrap();
    assert_eq!(staged.files.len(), 5);
    assert!(import_pending(&at(target.path())));
    assert_eq!(
        read(target.path(), "state/preferences.toml"),
        "magnet = false\n",
        "nothing is written until the next start"
    );

    let applied = apply_pending(&at(target.path()), "2026-09-24 10:00").unwrap();
    assert_eq!(applied.imported, Some(5));
    assert!(!import_pending(&at(target.path())));
    assert_eq!(
        read(target.path(), "state/preferences.toml"),
        "magnet = true\n"
    );
    assert_eq!(
        read(target.path(), "scopes/live-2/drawings.toml"),
        "next_id = 9\n"
    );
    assert_eq!(
        read(target.path(), "scopes/other/alerts.toml"),
        "keep = true\n",
        "what the backup does not hold is left alone"
    );

    // What was replaced (and only that: appearance was the same) is kept, and can be restored.
    let kept = Backup::read_file(&applied.kept_in.clone().unwrap(), None).unwrap();
    assert_eq!(kept.files.len(), 1);
    assert_eq!(kept.files[0].content, "magnet = false\n");
    let entries = at(target.path()).backups().list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, BackupKind::BeforeImport);

    // Applied once.
    assert_eq!(
        apply_pending(&at(target.path()), "again").unwrap(),
        Applied::default()
    );
}

#[test]
fn a_staged_import_can_be_cancelled_and_a_broken_one_is_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let text = collect(setup().path()).to_text().unwrap();
    stage_import(&at(dir.path()), &text, None).unwrap();
    cancel_import(&at(dir.path())).unwrap();
    assert!(!import_pending(&at(dir.path())));
    cancel_import(&at(dir.path())).unwrap();

    write(dir.path(), "pending-import.toml", "this is not a backup");
    let applied = apply_pending(&at(dir.path()), "t").unwrap();
    assert!(applied.set_aside);
    assert!(!import_pending(&at(dir.path())));
    assert!(dir.path().join("pending-import.toml.bad").is_file());
    assert!(stage_import(&at(dir.path()), "garbage", None).is_err());
    assert!(!import_pending(&at(dir.path())), "a bad file is not staged");
}

#[test]
fn a_reset_wipes_every_account_but_keeps_a_copy_and_leaves_sign_in_and_scripts_alone() {
    let dir = setup();
    write(dir.path(), "indicators/Mine.rhai", "plot(\"a\", close);");
    stage_reset(&at(dir.path())).unwrap();
    assert!(reset_pending(&at(dir.path())));
    assert!(
        dir.path().join("state/preferences.toml").is_file(),
        "nothing is removed until the next start"
    );

    let applied = apply_pending(&at(dir.path()), "2026-09-24").unwrap();
    assert!(applied.reset);
    assert!(!reset_pending(&at(dir.path())));
    assert!(!dir.path().join("state").exists());
    assert!(!dir.path().join("scopes").exists());
    assert!(
        dir.path().join("config.toml").is_file(),
        "the sign-in stays"
    );
    assert!(
        dir.path().join("indicators/Mine.rhai").is_file(),
        "the scripts stay"
    );

    // The reset can be undone from the copy it made.
    let store = at(dir.path()).backups();
    let copy = store.list().unwrap().remove(0);
    assert_eq!(copy.kind, BackupKind::BeforeReset);
    assert_eq!(store.load(&copy.id, None).unwrap().files.len(), 5);
    store.restore(&copy.id, None).unwrap();
    apply_pending(&at(dir.path()), "2026-09-25").unwrap();
    assert_eq!(
        read(dir.path(), "state/preferences.toml"),
        "magnet = true\n"
    );

    // Applied once; a reset of nothing makes no copy.
    assert_eq!(
        apply_pending(&at(dir.path()), "x").unwrap(),
        Applied::default()
    );
    let empty = tempfile::tempdir().unwrap();
    stage_reset(&at(empty.path())).unwrap();
    assert_eq!(apply_pending(&at(empty.path()), "x").unwrap().kept_in, None);
}

#[test]
fn a_reset_and_an_import_each_cancel_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let text = collect(setup().path()).to_text().unwrap();

    stage_import(&at(dir.path()), &text, None).unwrap();
    stage_reset(&at(dir.path())).unwrap();
    assert!(reset_pending(&at(dir.path())));
    assert!(
        !import_pending(&at(dir.path())),
        "the reset cancels the import"
    );

    stage_import(&at(dir.path()), &text, None).unwrap();
    assert!(import_pending(&at(dir.path())));
    assert!(
        !reset_pending(&at(dir.path())),
        "the import cancels the reset"
    );
}

#[test]
fn the_store_lists_reads_exports_and_deletes_copies() {
    let dir = setup();
    let paths = at(dir.path());
    let store = paths.backups();
    assert!(store.list().unwrap().is_empty(), "no folder yet");

    let backup = collect(dir.path());
    let first = store.create(&backup).unwrap();
    let second = store.create(&backup).unwrap();
    assert_eq!(first.id, "backup-today");
    assert_eq!(second.id, "backup-today-2", "a taken name gets a number");
    let sealed = store
        .save(
            BackupKind::Manual,
            &Backup::new("1", "later"),
            Some(&secret("pw")),
        )
        .unwrap();
    assert!(sealed.sealed);

    // Not copies: another kind of file, a name that is not one.
    write(&store.dir(), "notes.txt", "x");
    write(&store.dir(), "bad name.toml", "x");
    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 3);
    assert!(listed.iter().find(|e| e.id == sealed.id).unwrap().sealed);
    assert!(!listed.iter().find(|e| e.id == first.id).unwrap().sealed);

    assert_eq!(store.load(&first.id, None).unwrap(), backup);
    assert!(matches!(
        store.load(&sealed.id, None),
        Err(ConfigError::PassphraseRequired)
    ));
    assert!(matches!(
        store.load("missing", None),
        Err(ConfigError::BackupNotFound(_))
    ));
    assert!(store.load("../config", None).is_err(), "an id is a name");

    let out = tempfile::tempdir().unwrap().keep().join("carried.toml");
    store.export(&first.id, &out).unwrap();
    assert_eq!(Backup::read_file(&out, None).unwrap(), backup);

    store.remove(&first.id).unwrap();
    store.remove(&first.id).unwrap();
    assert_eq!(store.list().unwrap().len(), 2);
}

#[test]
fn prune_keeps_the_newest_of_one_kind_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = at(dir.path()).backups();
    for n in 0..4 {
        store
            .save(
                BackupKind::Automatic,
                &Backup::new("1", &format!("t{n}")),
                None,
            )
            .unwrap();
    }
    let manual = store.create(&Backup::new("1", "m")).unwrap();
    assert_eq!(store.prune(BackupKind::Automatic, 2).unwrap(), 2);
    let kinds: Vec<_> = store.list().unwrap().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == BackupKind::Automatic)
            .count(),
        2
    );
    assert!(
        store.get(&manual.id).is_ok(),
        "the other kinds are left alone"
    );
}

#[test]
fn an_automatic_copy_is_made_once_per_interval_and_never_of_nothing() {
    let dir = setup();
    let paths = at(dir.path());
    let store = paths.backups();
    let policy = AutoPolicy {
        every: Duration::from_secs(3600),
        keep: 2,
    };

    let empty = tempfile::tempdir().unwrap();
    assert!(
        at(empty.path())
            .backups()
            .auto_snapshot(None, "1", "t", policy)
            .unwrap()
            .is_none(),
        "nothing to save"
    );
    let first = store
        .auto_snapshot(None, "1", "t1", policy)
        .unwrap()
        .unwrap();
    assert_eq!(first.kind, BackupKind::Automatic);
    assert!(
        store
            .auto_snapshot(None, "1", "t2", policy)
            .unwrap()
            .is_none(),
        "too soon"
    );
    let always = AutoPolicy {
        every: Duration::ZERO,
        keep: 2,
    };
    store
        .auto_snapshot(None, "1", "t3", always)
        .unwrap()
        .unwrap();
    store
        .auto_snapshot(None, "1", "t4", always)
        .unwrap()
        .unwrap();
    assert_eq!(store.list().unwrap().len(), 2, "the oldest went");
}

#[test]
fn file_names_made_from_a_date_are_safe() {
    assert_eq!(
        file_stamp("2026-09-29T10:15:00+02:00"),
        "2026-09-29T10-15-00-02-00"
    );
    assert_eq!(file_stamp("../../x"), "x");
    assert_eq!(file_stamp(""), "now");
    assert!(is_valid_name(&format!("backup-{}", file_stamp("a b/c\\d"))));
}
