use super::*;

#[test]
fn the_defaults_list_every_section_once() {
    let layout = Layout::default();
    assert_eq!(layout.sections.len(), Section::ALL.len());
    assert!(layout.shows(Section::Send));
    assert!(layout.shows(Section::Options));
    assert_eq!(layout, layout.clone().normalized());
}

#[test]
fn a_list_is_mended_and_lost_sections_come_back_at_the_end() {
    let mut layout = Layout {
        sections: vec![
            Placed {
                item: Section::Summary,
                shown: true,
            },
            Placed {
                item: Section::Summary,
                shown: false,
            },
            Placed {
                item: Section::Size,
                shown: false,
            },
        ],
        ..Layout::default()
    };
    layout = layout.normalized();
    assert_eq!(layout.sections.len(), Section::ALL.len());
    assert_eq!(layout.sections[0].item, Section::Summary);
    assert!(layout.sections[0].shown, "the first of two copies wins");
    assert_eq!(layout.sections[1].item, Section::Size);
    assert!(!layout.sections[1].shown);
}

#[test]
fn there_is_always_a_way_to_send() {
    let mut layout = Layout::default();
    for placed in &mut layout.sections {
        if matches!(placed.item, Section::Send | Section::Sides) {
            placed.shown = false;
        }
    }
    let layout = layout.normalized();
    assert!(layout.shows(Section::Send));
}

#[test]
fn wild_values_are_brought_back_in_range() {
    let mut layout = Layout {
        width: 9_000.0,
        high_risk: f64::NAN,
        ..Layout::default()
    };
    layout.defaults.stop_pips = -3.0;
    layout.presets.lots = vec![0.0, -1.0, f64::NAN, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
    let layout = layout.normalized();
    assert_eq!(layout.width, WIDTH_MAX);
    assert_eq!(layout.high_risk, 5.0);
    assert_eq!(layout.defaults.stop_pips, 0.1);
    assert_eq!(layout.presets.lots.len(), MAX_PRESETS);
    assert_eq!(layout.presets.lots[0], 1.0);
    assert_eq!(
        Layout {
            width: f32::NAN,
            ..Layout::default()
        }
        .normalized()
        .width,
        WIDTH_DEFAULT
    );
}

#[test]
fn a_slot_moves_along_its_list_and_stops_at_the_ends() {
    let mut list = default_list::<Section>();
    let first = list[0].item;
    assert!(!shift(&mut list, 0, -1));
    assert!(shift(&mut list, 0, 1));
    assert_eq!(list[1].item, first);
    let last = list.len() - 1;
    assert!(!shift(&mut list, last, 1));
    assert!(!shift(&mut list, last + 5, -1));
}

#[test]
fn prefs_saved_before_the_layout_existed_load_with_the_defaults() {
    let old: TicketPrefs = toml::from_str("size_mode = \"risk_balance\"\nsize = 1.5\n").unwrap();
    assert_eq!(old.size_mode, SizeMode::RiskBalance);
    assert_eq!(old.size, 1.5);
    assert_eq!(old.layout, Layout::default());
}

#[test]
fn the_layout_survives_a_round_trip() {
    let mut prefs = TicketPrefs::default();
    prefs.layout.dock = Dock::Left;
    prefs.layout.sections.swap(0, 3);
    prefs.layout.presets.lots = vec![0.02, 0.2];
    let text = toml::to_string(&prefs).unwrap();
    let back: TicketPrefs = toml::from_str(&text).unwrap();
    assert_eq!(back, prefs);
}

#[test]
fn presets_are_read_by_the_way_of_sizing() {
    let presets = Presets::default();
    assert_eq!(presets.of(SizeMode::Lots), &[0.01, 0.1, 0.5, 1.0]);
    assert_eq!(
        presets.of(SizeMode::RiskEquity),
        presets.of(SizeMode::RiskBalance)
    );
    assert!(presets.of(SizeMode::RiskMoney).is_empty());
}
