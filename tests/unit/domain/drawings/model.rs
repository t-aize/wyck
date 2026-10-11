use super::*;

#[test]
fn a_style_saved_before_opacity_is_fully_opaque_and_the_opacity_is_kept_in_range() {
    let old: Style = toml::from_str(
        "color = 255
",
    )
    .unwrap();
    assert_eq!(old.opacity, 1.0);
    let wild = Style {
        opacity: 7.0,
        ..Style::default()
    }
    .normalized();
    assert_eq!(wild.opacity, 1.0);
    let faint = Style {
        opacity: 0.0,
        ..Style::default()
    }
    .normalized();
    assert!(faint.opacity >= 0.05);
}

fn drawing(id: u64, tool: Tool, count: usize) -> Drawing {
    Drawing::new(
        id,
        tool,
        (0..count)
            .map(|i| Point {
                t: i as i64 * 60_000,
                p: 100.0 + i as f64,
            })
            .collect(),
    )
}

#[test]
fn every_tool_has_a_group_a_label_and_a_positive_anchor_count() {
    for tool in Tool::ALL {
        assert!(!tool.label().is_empty());
        assert!(tool.anchors() >= 1, "{tool:?}");
        let _ = tool.group();
    }
    let mut labels: Vec<_> = Tool::ALL.iter().map(|t| t.label()).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), Tool::ALL.len());
}

#[test]
fn a_drawing_needs_the_points_its_tool_asks_for() {
    assert!(drawing(1, Tool::TrendLine, 2).is_valid());
    assert!(!drawing(1, Tool::TrendLine, 1).is_valid());
    assert!(drawing(1, Tool::LongPosition, 4).is_valid());
    assert!(!drawing(1, Tool::LongPosition, 3).is_valid());
    assert!(drawing(1, Tool::Brush, 40).is_valid());
    assert!(!drawing(1, Tool::Brush, 1).is_valid());
    assert!(!drawing(1, Tool::Unknown, 2).is_valid());
    let mut nan = drawing(1, Tool::HorizontalLine, 1);
    nan.points[0].p = f64::NAN;
    assert!(!nan.is_valid());
}

#[test]
fn quick_color_is_only_for_drawings_with_a_primary_color() {
    assert!(Tool::TrendLine.has_quick_color());
    assert!(Tool::Rectangle.has_quick_color());
    for tool in [
        Tool::LongPosition,
        Tool::ShortPosition,
        Tool::FibRetracement,
        Tool::Measure,
        Tool::PriceRange,
    ] {
        assert!(!tool.has_quick_color(), "{}", tool.label());
    }
}

#[test]
fn a_saved_document_reads_back_and_unknown_tools_do_not_break_it() {
    let mut doc = DrawingsDoc::default();
    doc.symbols.insert(
        "US100.cash".into(),
        vec![drawing(1, Tool::FibRetracement, 2)],
    );
    doc.next_id = 2;
    let text = toml::to_string_pretty(&doc).unwrap();
    let back: DrawingsDoc = toml::from_str(&text).unwrap();
    assert_eq!(back, doc);

    let with_future_tool = r#"
        [[symbols."EURUSD"]]
        id = 5
        tool = "future_tool"
        points = [{ t = 1, p = 1.0 }, { t = 2, p = 2.0 }]
        [[symbols."EURUSD"]]
        id = 6
        tool = "trend_line"
        points = [{ t = 1, p = 1.0 }, { t = 2, p = 2.0 }]
    "#;
    let parsed: DrawingsDoc = toml::from_str(with_future_tool).unwrap();
    let parsed = parsed.normalized();
    let kept = &parsed.symbols["EURUSD"];
    assert_eq!(
        kept.len(),
        1,
        "the tool this version does not know is dropped"
    );
    assert_eq!(kept[0].tool, Tool::TrendLine);
    assert!(parsed.next_id > 6);
}

#[test]
fn normalizing_repairs_ids_and_drops_the_broken() {
    let mut doc = DrawingsDoc::default();
    doc.symbols.insert(
        "A".into(),
        vec![
            drawing(3, Tool::Rectangle, 2),
            drawing(3, Tool::Ellipse, 2),
            drawing(0, Tool::Ray, 2),
            drawing(9, Tool::Ray, 1),
        ],
    );
    doc.symbols.insert("Empty".into(), vec![]);
    doc.next_id = 1;
    let doc = doc.normalized();
    let ids: Vec<u64> = doc.symbols["A"].iter().map(|d| d.id).collect();
    assert_eq!(ids.len(), 3, "the one with too few points is gone");
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 3, "ids are unique: {ids:?}");
    assert!(doc.next_id > *ids.iter().max().unwrap());
    assert!(!doc.symbols.contains_key("Empty"));
}

#[test]
fn an_empty_or_partial_file_still_loads() {
    let doc: DrawingsDoc = toml::from_str("").unwrap();
    assert!(doc.symbols.is_empty());
    let one: Drawing =
        toml::from_str("id = 1\ntool = \"text\"\npoints = [{ t = 5, p = 2.5 }]\n").unwrap();
    assert_eq!(one.style, Style::default());
    assert!(!one.locked);
    assert!(one.is_valid());
}

#[test]
fn a_drawing_shows_on_the_timeframes_it_is_kept_for() {
    let mut d = drawing(1, Tool::TrendLine, 2);
    assert!(d.shows_on("M5"));
    d.timeframes = Some(vec!["H1".into(), "D1".into()]);
    assert!(!d.shows_on("M5"));
    assert!(d.shows_on("H1"));
    d.hidden = true;
    assert!(!d.shows_on("H1"));
}

#[test]
fn levels_default_to_the_tools_and_are_kept_sane() {
    let mut d = drawing(1, Tool::FibRetracement, 2);
    assert_eq!(d.levels(), Tool::FibRetracement.default_levels());
    d.levels = vec![Level {
        value: f64::NAN,
        color: 0xff00_0000,
        visible: true,
        ..Level::default()
    }];
    let d = d.normalized();
    assert!(d.levels.is_empty(), "the broken level is dropped");
    let mut line = drawing(2, Tool::TrendLine, 2);
    line.levels = vec![Level {
        value: 1.0,
        color: 0,
        visible: true,
        ..Level::default()
    }];
    assert!(line.normalized().levels.is_empty(), "a line has no levels");
}

#[test]
fn every_new_part_of_a_look_survives_the_saved_file() {
    use super::super::look::{Cap, HAlign, LabelSide, LevelText, VAlign};

    let mut d = drawing(1, Tool::FibRetracement, 2);
    d.text = "the label".to_owned();
    d.style.text_layout.align = HAlign::End;
    d.style.text_layout.valign = VAlign::Bottom;
    d.style.text_layout.background = true;
    d.style.text_layout.background_color = Some(0x123456);
    d.style.caps.start = Cap::Circle;
    d.style.caps.end = Cap::Arrow;
    d.style.measure.percent = false;
    d.style.measure.down_color = 0xabcdef;
    d.style.profile.rows = 60;
    d.style.profile.value_area = 0.5;
    d.style.level_text = LevelText::Percent;
    d.style.label_side = LabelSide::Right;
    d.style.scale = 2.5;
    d.style.position.risk = 3.0;
    let mut levels = d.levels();
    levels[0].width = 3.0;
    levels[1].dash = Some(Dash::Dotted);
    d.levels = levels;

    let mut doc = DrawingsDoc::default();
    doc.symbols.insert("EURUSD".to_owned(), vec![d.clone()]);
    doc.named_templates.push(NamedTemplate {
        name: "Mine".to_owned(),
        tool: Tool::FibRetracement.code(),
        style: d.style.clone(),
        levels: d.levels.clone(),
    });
    let text = toml::to_string_pretty(&doc).unwrap();
    let back: DrawingsDoc = toml::from_str(&text).unwrap();
    let back = back.normalized();
    assert_eq!(back.symbols["EURUSD"][0], d.clone().normalized());
    assert_eq!(back.named_templates.len(), 1);
    assert_eq!(back.named_templates[0].style, d.style.clone().normalized());
    assert_eq!(back.named_templates[0].levels[0].width, 3.0);

    // A drawing left at the defaults writes none of it.
    let plain = drawing(2, Tool::TrendLine, 2);
    let mut doc = DrawingsDoc::default();
    doc.symbols.insert("EURUSD".to_owned(), vec![plain]);
    let text = toml::to_string_pretty(&doc).unwrap();
    for key in [
        "text_layout",
        "caps",
        "measure",
        "profile",
        "level_text",
        "scale",
        "position",
    ] {
        assert!(!text.contains(key), "{key} is written for nothing:\n{text}");
    }
}

#[test]
fn every_tool_has_a_code_and_templates_of_unknown_tools_are_dropped() {
    assert_eq!(Tool::HeadAndShoulders.code(), "head_and_shoulders");
    let mut doc = DrawingsDoc::default();
    doc.templates.insert(
        "gann_fan".into(),
        Template {
            style: Style {
                width: -3.0,
                ..Style::default()
            },
            levels: Vec::new(),
        },
    );
    doc.templates.insert(
        "nothing".into(),
        Template {
            style: Style::default(),
            levels: Vec::new(),
        },
    );
    let text = toml::to_string_pretty(&doc).unwrap();
    let back: DrawingsDoc = toml::from_str(&text).unwrap();
    let back = back.normalized();
    assert_eq!(back.templates.len(), 1);
    assert_eq!(back.templates["gann_fan"].style.width, 1.5);
}

#[test]
fn waves_are_named_by_their_degree() {
    assert_eq!(wave_label("3", DEFAULT_DEGREE), "3");
    assert_eq!(wave_label("3", 0), "(III)");
    assert_eq!(wave_label("4", 1), "IV");
    assert_eq!(wave_label("B", 2), "((B))");
    assert_eq!(wave_label("5", 5), "v");
    assert_eq!(wave_label("C", 6), "(c)");
}
