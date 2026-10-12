#![cfg(test)]

use super::*;

/// The words of the labels of a position, from top to bottom of the list.
fn position_labels(d: &Drawing, selected: bool) -> Vec<String> {
    prims_with(d, &Linear, selected)
        .into_iter()
        .filter_map(|s| match s {
            Prim::Label { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

fn long_position() -> Drawing {
    // Entry 100, stop 90, target 130: with the defaults, 1% of 10 000 risked over 10 is 10
    // units, so the stop loses 100 and the target gains 300.
    drawing(
        Tool::LongPosition,
        &[(100, 100.0), (100, 90.0), (100, 130.0), (400, 100.0)],
    )
}

#[test]
fn a_position_shows_its_risk_and_reward() {
    // Long: entry 100, stop 90, target 130.
    let mut d = drawing(
        Tool::LongPosition,
        &[(100, 100.0), (100, 90.0), (100, 130.0), (400, 100.0)],
    );
    d.style.position.compact = false;
    let labels: Vec<String> = prims_with(&d, &Linear, true)
        .into_iter()
        .filter_map(|s| match s {
            Prim::Label { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert!(labels.iter().any(|t| t.contains("R/R 3.00")), "{labels:?}");
    assert!(
        labels
            .iter()
            .any(|t| t.starts_with("Target 130.0 (+30.00%)")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|t| t.starts_with("Stop 90.0 (-10.00%)")),
        "{labels:?}"
    );

    // A long with the stop above the entry is said to be wrong, not given a ratio.
    let mut wrong = drawing(
        Tool::LongPosition,
        &[(100, 100.0), (100, 120.0), (100, 130.0), (400, 100.0)],
    );
    wrong.style.position.compact = false;
    assert!(prims_with(&wrong, &Linear, true).iter().any(|s| matches!(
        s, Prim::Label { text, .. } if text.contains("wrong sides")
    )));
    // A short mirrors it.
    let mut short = drawing(
        Tool::ShortPosition,
        &[(100, 100.0), (100, 110.0), (100, 80.0), (400, 100.0)],
    );
    short.style.position.compact = false;
    assert!(prims_with(&short, &Linear, true).iter().any(|s| matches!(
        s, Prim::Label { text, .. } if text.starts_with("Short | Qty") && text.contains("R/R 2.00")
    )));
}

#[test]
fn a_position_writes_its_size_and_its_money() {
    let mut d = long_position();
    d.style.position.compact = false;
    let labels = position_labels(&d, true);
    assert!(
        labels
            .iter()
            .any(|t| t == "Long | Qty 10 | R/R 3.00 | Risk 100.00"),
        "{labels:?}"
    );
    assert!(labels.iter().any(|t| t.ends_with("+300.00")), "{labels:?}");
    assert!(labels.iter().any(|t| t.ends_with("-100.00")), "{labels:?}");
}

#[test]
fn the_switches_choose_what_a_position_writes() {
    let mut d = long_position();
    d.style.position.compact = false;
    d.style.position.show_qty = false;
    d.style.position.show_risk = false;
    d.style.position.show_amounts = false;
    d.style.position.show_price = false;
    let labels = position_labels(&d, true);
    assert!(labels.iter().any(|t| t == "Long | R/R 3.00"), "{labels:?}");
    assert!(labels.iter().any(|t| t == "Target (+30.00%)"), "{labels:?}");

    // Compact: one short figure per tag.
    let mut d = long_position();
    d.style.position.compact = true;
    let labels = position_labels(&d, true);
    assert!(labels.iter().any(|t| t == "Long | 10 | 3.0R"), "{labels:?}");
    assert!(labels.iter().any(|t| t == "+300.00"), "{labels:?}");
    assert!(labels.iter().any(|t| t == "-100.00"), "{labels:?}");

    // A currency follows the amounts.
    let mut d = long_position();
    d.style.position.currency = "USD".to_owned();
    assert!(
        position_labels(&d, true)
            .iter()
            .any(|t| t.ends_with("+300.00 USD"))
    );
}

#[test]
fn visible_tags_show_the_full_stats_when_requested() {
    let mut d = long_position();
    d.style.position.always_stats = true;
    d.style.position.compact = false;
    let idle = position_labels(&d, false);
    assert!(idle.iter().any(|t| t.contains("Qty 10")), "{idle:?}");
    let picked = position_labels(&d, true);
    assert_eq!(picked, idle);
}

#[test]
fn position_tags_show_only_when_selected_by_default() {
    for tool in [Tool::LongPosition, Tool::ShortPosition] {
        let mut d = long_position();
        d.tool = tool;
        assert!(position_labels(&d, false).is_empty());
        assert!(
            prims_with(&d, &Linear, false)
                .iter()
                .any(|prim| matches!(prim, Prim::Rect { .. }))
        );
        assert_eq!(position_labels(&d, true).len(), 3);
        d.style.position.always_stats = true;
        assert_eq!(position_labels(&d, false).len(), 3);
        d.style.labels = false;
        assert!(position_labels(&d, true).is_empty());
    }
}

#[test]
fn positions_can_show_pips_in_compact_and_full_tags() {
    for (tool, stop, target) in [
        (Tool::LongPosition, 99.9, 100.3),
        (Tool::ShortPosition, 100.1, 99.7),
    ] {
        let mut d = drawing(
            tool,
            &[(100, 100.0), (100, stop), (100, target), (400, 100.0)],
        );
        d.style.position.show_pips = true;
        let compact = position_labels(&d, true);
        assert!(
            compact.iter().any(|text| text.contains("30.0 pips")),
            "{compact:?}"
        );
        assert!(
            compact.iter().any(|text| text.contains("10.0 pips")),
            "{compact:?}"
        );
        d.style.position.compact = false;
        let full = position_labels(&d, true);
        assert!(
            full.iter().any(|text| text.contains("30.0 pips")),
            "{full:?}"
        );
        assert!(
            full.iter().any(|text| text.contains("10.0 pips")),
            "{full:?}"
        );
    }
}

#[test]
fn positions_move_from_the_zones_but_not_their_tags() {
    for (tool, stop, target, stop_y, target_y) in [
        (Tool::LongPosition, 90.0, 130.0, 405.0, 385.0),
        (Tool::ShortPosition, 110.0, 80.0, 395.0, 415.0),
    ] {
        let mut d = drawing(
            tool,
            &[(100, 100.0), (100, stop), (100, target), (400, 100.0)],
        );
        assert_eq!(hit(&d, &Linear, (150.0, stop_y), true), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (150.0, target_y), true), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (250.0, 400.0), true), None);
        assert_eq!(
            hit(&d, &Linear, (100.0, 400.0), true),
            Some(Part::Handle(0))
        );
        d.style.position.always_stats = true;
        assert_eq!(hit(&d, &Linear, (250.0, 400.0), false), None);
    }
}

#[test]
fn a_position_takes_its_colors_and_lines_from_its_settings() {
    let mut d = long_position();
    d.style.position.target_color = 0x123456;
    d.style.position.stop_color = 0x654321;
    d.style.width = 3.0;
    d.style.fill_opacity = 0.5;
    let shapes = prims(&d, &Linear);
    let fills: Vec<(u32, f32)> = shapes
        .iter()
        .filter_map(|s| match s {
            Prim::Rect {
                fill: Some(fill), ..
            } => Some(*fill),
            _ => None,
        })
        .collect();
    assert_eq!(fills, vec![(0x123456, 0.5), (0x654321, 0.5)]);
    assert!(shapes.iter().any(|s| matches!(
        s, Prim::Segment { color: 0x123456, width, .. } if *width == 3.0
    )));

    // No background, no tags.
    d.style.fill = false;
    d.style.labels = false;
    let bare = prims(&d, &Linear);
    assert!(
        !bare
            .iter()
            .any(|s| matches!(s, Prim::Rect { .. } | Prim::Label { .. }))
    );
    assert_eq!(bare.len(), 3, "the three lines are left");
}
