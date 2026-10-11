use super::*;
use crate::domain::indicators::Placement;

const SMA: &str = "indicator(#{ name: \"Two lines\", short: \"TL\", overlay: true });\n\
    let len = input_int(\"length\", 20, #{ min: 2, max: 200 });\n\
    plot(\"fast\", sma(close, len));";

fn library() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::new(dir.path().join("indicators"));
    library.ensure_dir().unwrap();
    (dir, library)
}

#[test]
fn a_file_in_the_folder_is_an_indicator_with_a_spec() {
    let (_guard, mut library) = library();
    library.create(None, "Two lines", SMA).unwrap();
    let entry = library.get("Two lines").unwrap();
    assert!(entry.is_ready());
    assert_eq!(entry.spec.label, "Two lines");
    assert_eq!(entry.spec.short, "TL");
    assert_eq!(entry.spec.placement, Placement::Overlay);
    assert_eq!(entry.spec.inputs.len(), 1);
    assert_eq!(entry.spec.inputs[0].key, "length");
    assert_eq!(entry.spec.plots.len(), 1);
}

#[test]
fn a_script_with_a_mistake_is_listed_as_broken_with_its_problems() {
    let (_guard, mut library) = library();
    library.create(None, "Oops", "plot(\"a\", clos);").unwrap();
    let entry = library.get("Oops").unwrap();
    assert!(!entry.is_ready());
    assert_eq!(entry.problems.len(), 1);
    assert_eq!(entry.info.category, "Broken");
}

#[test]
fn folders_make_categories_and_ids() {
    let (_guard, mut library) = library();
    let id = library.create(Some("Trend"), "Two lines", SMA).unwrap();
    assert_eq!(id, "Trend/Two lines");
    assert_eq!(library.get(&id).unwrap().info.category, "Trend");
    assert!(
        library.path_of(&id).ends_with("Trend/Two lines.rhai")
            || library.path_of(&id).ends_with("Trend\\Two lines.rhai")
    );
}

#[test]
fn refreshing_tells_what_was_added_edited_and_removed_and_leaves_the_rest() {
    let (_guard, mut library) = library();
    library.create(None, "One", SMA).unwrap();
    library.create(None, "Two", SMA).unwrap();
    assert!(library.refresh().is_empty());
    let one = library.get("One").unwrap();

    let path = library.path_of("Two");
    std::fs::write(&path, format!("{SMA}\nplot(\"slow\", sma(close, 50));")).unwrap();
    std::fs::write(library.path_of("Three"), SMA).unwrap();
    std::fs::remove_file(library.path_of("One")).unwrap();
    let changes = library.refresh();
    assert_eq!(changes.added, ["Three"]);
    assert_eq!(changes.changed, ["Two"]);
    assert_eq!(changes.removed, ["One"]);
    assert!(library.get("One").is_none());
    assert_eq!(one.id, "One");
}

#[test]
fn saving_the_same_text_changes_nothing_for_anyone() {
    let (_guard, mut library) = library();
    library.create(None, "One", SMA).unwrap();
    let before = library.get("One").unwrap();
    let changes = library.save("One", SMA).unwrap();
    assert!(changes.is_empty(), "{changes:?}");
    assert!(Arc::ptr_eq(&before, &library.get("One").unwrap()));
}

#[test]
fn names_that_are_taken_get_a_number() {
    let (_guard, mut library) = library();
    assert_eq!(library.create(None, "A", SMA).unwrap(), "A");
    assert_eq!(library.create(None, "A", SMA).unwrap(), "A (2)");
    assert_eq!(library.duplicate("A").unwrap(), "A copy");
}

#[test]
fn names_the_system_would_refuse_are_refused_here() {
    for bad in [
        "",
        "  ",
        ".hidden",
        "a/b",
        "con",
        "NUL",
        "what?",
        "x\\y",
        "tab\tname",
    ] {
        assert!(clean_part(bad).is_err(), "{bad:?}");
    }
    assert_eq!(clean_part(" My average (2) ").unwrap(), "My average (2)");
    assert!(clean_id("a/b/c/d/e/f").is_err());
    assert_eq!(clean_id("Trend/Fast MA").unwrap(), "Trend/Fast MA");
}

#[test]
fn renaming_moves_the_file_and_refuses_a_taken_name() {
    let (_guard, mut library) = library();
    library.create(Some("F"), "Old", SMA).unwrap();
    library.create(Some("F"), "Other", SMA).unwrap();
    assert_eq!(library.rename("F/Old", "New").unwrap(), "F/New");
    assert!(library.get("F/Old").is_none() && library.get("F/New").is_some());
    assert!(matches!(
        library.rename("F/New", "Other"),
        Err(LibraryError::Exists(_))
    ));
    assert!(matches!(
        library.rename("nope", "X"),
        Err(LibraryError::Missing(_))
    ));
}

#[test]
fn deleting_keeps_the_file_in_the_trash() {
    let (_guard, mut library) = library();
    library.create(None, "Gone", SMA).unwrap();
    library.delete("Gone").unwrap();
    assert!(library.get("Gone").is_none());
    let trash: Vec<_> = std::fs::read_dir(library.dir().join(TRASH))
        .unwrap()
        .collect();
    assert_eq!(trash.len(), 1);
    // The trash is not read as indicators.
    assert!(library.refresh().is_empty());
}

#[test]
fn importing_takes_scripts_and_says_why_it_skips_the_rest() {
    let (guard, mut library) = library();
    let elsewhere = guard.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("good.rhai"), SMA).unwrap();
    std::fs::write(elsewhere.join("notes.txt"), "hello").unwrap();
    std::fs::write(elsewhere.join("binary.rhai"), [0xff, 0xfe, 0x00, 0x81]).unwrap();
    let report = library.import(&[
        elsewhere.join("good.rhai"),
        elsewhere.join("notes.txt"),
        elsewhere.join("binary.rhai"),
    ]);
    assert_eq!(report.imported, ["good"]);
    assert_eq!(report.skipped.len(), 2);
    assert!(library.get("good").unwrap().is_ready());
    // Importing it again does not replace the first.
    let again = library.import(&[elsewhere.join("good.rhai")]);
    assert_eq!(again.imported, ["good (2)"]);
}

#[test]
fn exporting_writes_the_text_where_asked() {
    let (guard, mut library) = library();
    library.create(Some("F"), "Out", SMA).unwrap();
    let dest = guard.path().join("exported");
    std::fs::create_dir_all(&dest).unwrap();
    let file = library.export("F/Out", &dest).unwrap();
    assert_eq!(std::fs::read_to_string(file).unwrap(), SMA);
    let all = guard.path().join("all");
    assert_eq!(library.export_all(&all).unwrap(), 1);
    assert!(all.join("F").join("Out.rhai").exists());
}

#[test]
fn a_huge_file_is_not_read() {
    let (_guard, mut library) = library();
    std::fs::write(
        library.path_of("Big"),
        vec![b'a'; (MAX_FILE_BYTES + 1) as usize],
    )
    .unwrap();
    library.refresh();
    let entry = library.get("Big").unwrap();
    assert!(!entry.is_ready());
    assert!(entry.problems[0].message.contains("bigger"));
}

#[test]
fn what_is_published_is_what_charts_look_up() {
    let (_guard, mut library) = library();
    library.create(None, "Published one", SMA).unwrap();
    let before = registry::revision();
    assert!(library.publish());
    assert!(registry::revision() > before);
    assert!(registry::get("Published one").is_some());
    assert!(
        !library.publish(),
        "publishing the same thing again changes nothing"
    );
}
