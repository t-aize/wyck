#![cfg(test)]

use super::*;

#[test]
fn a_saved_look_is_what_new_drawings_of_the_tool_start_with() {
    let (mut book, first, _) = two_lines();
    let mut changed = book.get(SYMBOL, first).unwrap().clone();
    changed.style.color = 0xabcdef;
    changed.style.extend_right = true;
    book.preview(SYMBOL, changed);
    assert!(book.save_template(SYMBOL, first));
    assert!(book.has_template(Tool::TrendLine));
    book.set_tool(Some(Tool::TrendLine));
    click(&mut book, 60.0, 300.0);
    click(&mut book, 300.0, 350.0);
    let made = book.drawings(SYMBOL).last().unwrap();
    assert_eq!(made.style.color, 0xabcdef);
    assert!(made.style.extend_right);

    // The saved look survives a save and a load.
    let reloaded = Book::from_doc(book.to_doc());
    assert!(reloaded.has_template(Tool::TrendLine));
    assert!(book.forget_template(Tool::TrendLine));
    assert_eq!(
        book.starting_style(Tool::TrendLine).0,
        Tool::TrendLine.default_style()
    );
}

#[test]
fn a_look_can_be_saved_under_a_name_replaced_deleted_and_kept_in_the_file() {
    let (mut book, first, second) = two_lines();
    let mut red = book.get(SYMBOL, first).unwrap().clone();
    red.style.color = 0xff0000;
    red.style.text_layout.background = true;
    book.preview(SYMBOL, red);
    let before = book.revision();
    assert!(book.save_named_template(SYMBOL, first, "  Resistance  "));
    assert!(book.revision() > before, "worth saving");
    assert!(
        !book.save_named_template(SYMBOL, first, "   "),
        "no name, no look"
    );
    assert!(
        !book.save_named_template(SYMBOL, 9_999, "Ghost"),
        "no such drawing"
    );

    let listed = book.named_templates(Tool::TrendLine);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Resistance", "trimmed");
    assert_eq!(listed[0].style.color, 0xff0000);
    assert!(
        book.named_templates(Tool::Rectangle).is_empty(),
        "each tool has its own"
    );

    // The same name replaces the look, another name adds one.
    let mut blue = book.get(SYMBOL, second).unwrap().clone();
    blue.style.color = 0x0000ff;
    book.preview(SYMBOL, blue);
    assert!(book.save_named_template(SYMBOL, second, "Resistance"));
    assert!(book.save_named_template(SYMBOL, second, "Support"));
    let names: Vec<&str> = book
        .named_templates(Tool::TrendLine)
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, vec!["Resistance", "Support"]);
    assert_eq!(
        book.named_templates(Tool::TrendLine)[0].style.color,
        0x0000ff
    );

    // They survive a save and a load, and a name is forgotten on demand.
    let reloaded = Book::from_doc(book.to_doc());
    assert_eq!(reloaded.named_templates(Tool::TrendLine).len(), 2);
    assert!(book.delete_named_template(Tool::TrendLine, "Support"));
    assert!(!book.delete_named_template(Tool::TrendLine, "Support"));
    assert_eq!(book.named_templates(Tool::TrendLine).len(), 1);
}

#[test]
fn a_book_holds_a_limited_number_of_named_looks() {
    let (mut book, first, _) = two_lines();
    for n in 0..MAX_NAMED_TEMPLATES {
        assert!(book.save_named_template(SYMBOL, first, &format!("look {n}")));
    }
    assert!(!book.save_named_template(SYMBOL, first, "one too many"));
    assert!(
        book.save_named_template(SYMBOL, first, "look 0"),
        "a name that exists is replaced, not added"
    );
    let long = "x".repeat(200);
    assert!(book.delete_named_template(Tool::TrendLine, "look 1"));
    assert!(book.save_named_template(SYMBOL, first, &long));
    assert!(
        book.named_templates(Tool::TrendLine)
            .iter()
            .any(|t| t.name.chars().count() == MAX_TEMPLATE_NAME)
    );
}
