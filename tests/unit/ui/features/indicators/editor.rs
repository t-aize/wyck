use super::*;

#[test]
fn unsaved_multiline_script_survives_the_draft_document() {
    let drafts = Drafts {
        directory: "C:/indicators".into(),
        active: 0,
        tabs: vec![DraftTab {
            id: "SMC/Gap".into(),
            buffer: Some("indicator(#{ name: \"Gap\" });\n// unsaved\n".into()),
            base: Some("indicator(#{ name: \"Gap\" });\n".into()),
        }],
    };
    let text = toml::to_string(&drafts).unwrap();
    let restored: Drafts = toml::from_str(&text).unwrap();
    assert_eq!(restored.tabs[0].buffer, drafts.tabs[0].buffer);
    assert_eq!(restored.tabs[0].base, drafts.tabs[0].base);
}
