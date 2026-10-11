use super::*;
use crate::domain::market::Period;

#[test]
fn an_empty_file_gives_the_defaults() {
    let prefs: Preferences = toml::from_str("").unwrap();
    assert_eq!(prefs.normalized(), Preferences::default().normalized());
}

#[test]
fn safety_limits_and_exits_load_from_old_files_and_round_trip() {
    let old: Preferences = toml::from_str("magnet = true").unwrap();
    assert!(!old.ticket.exits.on);
    assert_eq!(old.normalized().risk, RiskPrefs::default());
    let mut prefs = Preferences::default();
    prefs.risk.max_daily_loss = 250.0;
    prefs.risk.kill_switch = true;
    prefs.ticket.exits.on = true;
    prefs.ticket.exits.oco_pips = 12.5;
    let text = toml::to_string(&prefs).unwrap();
    let back: Preferences = toml::from_str(&text).unwrap();
    let back = back.normalized();
    assert_eq!(back.risk, prefs.risk);
    assert_eq!(back.ticket.exits, prefs.ticket.exits);
}

#[test]
fn a_file_from_before_replay_was_removed_still_loads() {
    let old: Preferences =
        toml::from_str("magnet = true\n\n[replay]\ndefault_speed = 2.0\n").unwrap();
    assert!(old.magnet);
}

#[test]
fn usage_limits_load_from_old_files_and_stay_in_supported_ranges() {
    let old: Preferences = toml::from_str("magnet = true").unwrap();
    assert_eq!(old.normalized().limits, UsageLimits::default());
    let prefs = Preferences {
        limits: UsageLimits {
            studies_per_chart: 0,
            alerts: usize::MAX,
            drawings_per_symbol: 3_000,
        },
        ..Preferences::default()
    };
    let prefs = prefs.normalized();
    assert_eq!(prefs.limits.studies_per_chart, 1);
    assert_eq!(prefs.limits.alerts, MAX_SAVED_ALERTS);
    assert_eq!(prefs.limits.drawings_per_symbol, 3_000);
    let saved = toml::to_string(&prefs).unwrap();
    let loaded: Preferences = toml::from_str(&saved).unwrap();
    assert_eq!(loaded.normalized().limits, prefs.limits);
}

#[test]
fn a_saved_arrangement_is_read_back_exactly() {
    let mut prefs = Preferences {
        zone: Zone::Utc,
        magnet: true,
        ..Preferences::default()
    };
    prefs.links.interval = true;
    let state = |timeframe, kind, symbol: Option<&str>| ChartState {
        timeframe,
        settings: ChartSettings {
            kind,
            ..ChartSettings::default()
        },
        symbol: symbol.map(str::to_owned),
    };
    prefs.set_arrangement(
        LayoutKey {
            count: 4,
            variant: 0,
        },
        &[
            state(Timeframe::Bars(Period::M5), ChartKind::Candles, None),
            state(
                Timeframe::Seconds(15),
                ChartKind::HeikinAshi,
                Some("XAUUSD"),
            ),
            state(Timeframe::Ticks, ChartKind::Line, None),
            state(Timeframe::Bars(Period::D1), ChartKind::Renko, None),
        ],
        2,
    );
    let text = toml::to_string_pretty(&prefs).unwrap();
    let back: Preferences = toml::from_str(&text).unwrap();
    assert_eq!(back.clone().normalized(), prefs);
    let second = back.chart(1);
    assert_eq!(second.timeframe, Timeframe::Seconds(15));
    assert_eq!(second.settings.kind, ChartKind::HeikinAshi);
    assert_eq!(second.symbol.as_deref(), Some("XAUUSD"));
    assert_eq!(back.chart(3).settings.kind, ChartKind::Renko);
    assert_eq!(back.active_chart, 2);
}

#[test]
fn unknown_values_are_repaired_not_refused() {
    let text = r#"
        favorite_timeframes = ["M5", "M5", "X9", "H1"]
        active_chart = 40
        [layout]
        count = 3
        variant = 99
        [[charts]]
        timeframe = "M9999"
        kind = "spiral"
    "#;
    let prefs: Preferences = toml::from_str(text).unwrap();
    let prefs = prefs.normalized();
    assert_eq!(prefs.favorite_timeframes, vec!["M5", "H1"]);
    assert_eq!(prefs.layout, LayoutPref::default(), "a layout that is gone");
    assert_eq!(prefs.charts.len(), 1);
    assert_eq!(prefs.chart(0).timeframe, Timeframe::DEFAULT);
    assert_eq!(prefs.chart(0).settings.kind, ChartKind::Candles);
    assert_eq!(prefs.active_chart, 0);
}

#[test]
fn a_file_from_before_chart_settings_keeps_its_types_and_links() {
    let text = r#"
        [links]
        interval = true
        crosshair = false
        time = false
        range = true
        [[charts]]
        timeframe = "H1"
        kind = "hollow"
    "#;
    let prefs: Preferences = toml::from_str(text).unwrap();
    let prefs = prefs.normalized();
    assert!(prefs.links.symbol, "the symbol link is new and on");
    assert!(prefs.links.interval && prefs.links.range && !prefs.links.time);
    assert_eq!(prefs.chart(0).settings.kind, ChartKind::Hollow);
    assert!(prefs.panel_open);
}

#[test]
fn a_0_1_preferences_fixture_keeps_its_layout_and_chart_state() {
    let text = include_str!("../../../fixtures/preferences-0.1.toml");
    let prefs: Preferences = toml::from_str(text).unwrap();
    let prefs = prefs.normalized();

    assert_eq!(prefs.schema_version, 1);
    assert_eq!(
        prefs.layout,
        LayoutPref {
            count: 4,
            variant: 0
        }
    );
    assert_eq!(prefs.active_chart, 2);
    assert_eq!(prefs.favorite_timeframes, vec!["M5", "H1"]);
    assert_eq!(prefs.custom_timeframes, vec!["M7"]);
    assert_eq!(prefs.charts.len(), 4);
    assert_eq!(prefs.chart(1).symbol.as_deref(), Some("XAUUSD"));
    assert_eq!(prefs.chart(1).settings.kind, ChartKind::Hollow);
    assert!(prefs.magnet && prefs.ticket_open);
    assert!(!prefs.keep_drawing);
    assert_eq!(prefs.panel_height, 280.0);
    assert_eq!(prefs.saved_colors, vec![16711680, 65280]);
}

#[test]
fn the_charts_always_match_the_layout() {
    let prefs = Preferences {
        layout: LayoutPref {
            count: 6,
            variant: 0,
        },
        ..Preferences::default()
    }
    .normalized();
    assert_eq!(prefs.charts.len(), 6);
    // Added charts do not all start on the same timeframe.
    assert_ne!(prefs.charts[1].timeframe, prefs.charts[2].timeframe);

    let mut prefs = prefs;
    prefs.layout = LayoutPref {
        count: 2,
        variant: 0,
    };
    prefs.active_chart = 5;
    let prefs = prefs.normalized();
    assert_eq!(prefs.charts.len(), 2);
    assert_eq!(prefs.active_chart, 1);
}

#[test]
fn favorite_timeframes_keep_menu_order_and_never_run_out() {
    let mut prefs = Preferences {
        favorite_timeframes: vec!["D1".into(), "M1".into()],
        ..Preferences::default()
    }
    .normalized();
    assert_eq!(prefs.favorite_timeframes, vec!["M1", "D1"]);

    prefs.toggle_favorite_timeframe(Timeframe::Seconds(5));
    assert_eq!(prefs.favorite_timeframes, vec!["S5", "M1", "D1"]);
    prefs.toggle_favorite_timeframe(Timeframe::Bars(Period::M1));
    prefs.toggle_favorite_timeframe(Timeframe::Seconds(5));
    assert_eq!(prefs.favorite_timeframes, vec!["D1"]);
    prefs.toggle_favorite_timeframe(Timeframe::Bars(Period::D1));
    assert_eq!(
        prefs.favorite_timeframes,
        vec!["D1"],
        "the last one stays, so the header keeps a button"
    );
    assert!(prefs.is_favorite_timeframe(Timeframe::Bars(Period::D1)));
}

#[test]
fn custom_timeframes_are_kept_once_and_can_be_starred() {
    let mut prefs = Preferences::default().normalized();
    let h2 = Timeframe::from_code("H2").unwrap();
    let m7 = Timeframe::from_code("M7").unwrap();
    prefs.add_custom_timeframe(m7);
    prefs.add_custom_timeframe(Timeframe::from_code("120m").unwrap());
    prefs.add_custom_timeframe(h2);
    // H2 is offered by the menu already; M7 is the one added.
    assert_eq!(prefs.custom_timeframes, vec!["M7"]);
    prefs.add_custom_timeframe(Timeframe::from_code("M9").unwrap());
    assert_eq!(prefs.custom_timeframes, vec!["M7", "M9"]);
    prefs.toggle_favorite_timeframe(m7);
    assert!(prefs.favorite_timeframes.contains(&"M7".to_owned()));
    // Saved and read back, it keeps its place in the header and the menu.
    let text = toml::to_string(&prefs).unwrap();
    let back: Preferences = toml::from_str(&text).unwrap();
    let back = back.normalized();
    assert_eq!(
        back.customs(),
        vec![m7, Timeframe::from_code("M9").unwrap()]
    );
    assert!(back.favorites().contains(&m7));
    // Taking it out of the menu takes it off the header too.
    prefs.remove_custom_timeframe(m7);
    assert_eq!(prefs.custom_timeframes, vec!["M9"]);
    assert!(!prefs.is_favorite_timeframe(m7));
    // A favorite from nowhere (a file written by hand) is added to the menu.
    let prefs = Preferences {
        favorite_timeframes: vec!["M1".into(), "M13".into()],
        ..Preferences::default()
    }
    .normalized();
    assert_eq!(prefs.custom_timeframes, vec!["M13"]);
}

#[test]
fn favorites_of_nothing_fall_back_to_the_defaults() {
    let prefs = Preferences {
        favorite_timeframes: vec![],
        ..Preferences::default()
    };
    assert_eq!(prefs.normalized().favorites(), QUICK.to_vec());
}

#[test]
fn watchlist_names_are_checked() {
    let mut lists = Watchlists::default();
    assert_eq!(lists.create_list("  Majors "), Ok(0));
    assert_eq!(lists.lists[0].name, "Majors");
    assert_eq!(lists.create_list("majors"), Err(NameError::Taken));
    assert_eq!(lists.create_list("   "), Err(NameError::Empty));
    assert_eq!(
        lists.create_list(&"x".repeat(MAX_LIST_NAME + 1)),
        Err(NameError::TooLong)
    );
    assert_eq!(lists.create_list("Metals"), Ok(1));
    assert_eq!(lists.rename_list(1, "Majors"), Err(NameError::Taken));
    assert_eq!(
        lists.rename_list(1, "METALS"),
        Ok(()),
        "its own name is fine"
    );
    assert_eq!(lists.rename_list(1, "Gold and silver"), Ok(()));
}

#[test]
fn symbols_move_in_and_out_of_lists_and_favorites() {
    let mut lists = Watchlists::default();
    lists.create_list("A").unwrap();
    lists.toggle_in_list(0, "EURUSD");
    lists.toggle_in_list(0, "XAUUSD");
    assert!(lists.in_list(0, "EURUSD"));
    lists.toggle_in_list(0, "EURUSD");
    assert!(!lists.in_list(0, "EURUSD"));
    assert_eq!(lists.lists[0].symbols, vec!["XAUUSD"]);
    lists.toggle_in_list(7, "X");
    assert!(!lists.in_list(7, "X"));

    assert!(!lists.is_favorite("US100.cash"));
    lists.toggle_favorite("US100.cash");
    assert!(lists.is_favorite("US100.cash"));
    lists.toggle_favorite("US100.cash");
    assert!(!lists.is_favorite("US100.cash"));

    lists.delete_list(0);
    lists.delete_list(0);
    assert!(lists.lists.is_empty());
}

#[test]
fn a_hand_edited_watchlist_file_is_cleaned_up() {
    let text = r#"
        favorites = ["A", "B", "A"]
        [[lists]]
        name = "One"
        symbols = ["X", "X", "Y"]
        [[lists]]
        name = " one "
        [[lists]]
        name = ""
    "#;
    let lists: Watchlists = toml::from_str(text).unwrap();
    let lists = lists.normalized();
    assert_eq!(lists.favorites, vec!["A", "B"]);
    assert_eq!(lists.lists.len(), 1);
    assert_eq!(lists.lists[0].symbols, vec!["X", "Y"]);
}

#[test]
fn a_0_1_watchlist_fixture_is_preserved_and_normalized() {
    let text = include_str!("../../../fixtures/watchlists-0.1.toml");
    let lists: Watchlists = toml::from_str(text).unwrap();
    let lists = lists.normalized();

    assert_eq!(lists.schema_version, 1);
    assert_eq!(lists.favorites, vec!["EURUSD", "XAUUSD"]);
    assert_eq!(lists.lists.len(), 2);
    assert_eq!(lists.lists[0].name, "Majors");
    assert_eq!(lists.lists[0].symbols, vec!["EURUSD", "GBPUSD"]);
    assert_eq!(lists.lists[1].symbols, vec!["XAUUSD", "XAGUSD"]);
}

#[test]
fn watchlists_survive_awkward_symbol_names_through_toml() {
    let mut lists = Watchlists::default();
    lists.create_list("Odd \"names\" & more").unwrap();
    lists.toggle_in_list(0, "US100.cash");
    lists.toggle_in_list(0, "EUR/USD #1");
    let text = toml::to_string_pretty(&lists).unwrap();
    let back: Watchlists = toml::from_str(&text).unwrap();
    assert_eq!(back, lists);
}

#[test]
fn the_favorite_tools_start_with_the_usual_ones_and_old_files_get_them() {
    let fresh = Preferences::default();
    let list = fresh.favorite_tool_list();
    assert_eq!(list[0], Tool::TrendLine);
    assert!(list.contains(&Tool::LongPosition));
    assert!(fresh.favorites_bar && !fresh.favorites_labels);

    // A file from before the bar has none of these keys.
    let old: Preferences = toml::from_str(
        "magnet = true
",
    )
    .unwrap();
    assert_eq!(old.favorite_tool_list(), list);
    assert!(old.favorites_bar);
}

#[test]
fn a_tool_is_pinned_and_unpinned_by_the_same_call() {
    let mut prefs = Preferences::default();
    assert!(
        !prefs.toggle_favorite_tool(Tool::TrendLine),
        "it was pinned: now it is not"
    );
    assert!(!prefs.favorite_tool_list().contains(&Tool::TrendLine));
    assert!(prefs.toggle_favorite_tool(Tool::Ellipse));
    assert_eq!(prefs.favorite_tool_list().last(), Some(&Tool::Ellipse));
    assert!(!prefs.toggle_favorite_tool(Tool::Ellipse));
}

#[test]
fn the_bar_holds_a_limited_number_and_an_empty_bar_stays_empty() {
    let mut prefs = Preferences::default();
    prefs.favorite_tools.clear();
    for tool in Tool::ALL.into_iter().filter(|t| *t != Tool::Unknown) {
        prefs.toggle_favorite_tool(tool);
    }
    assert_eq!(prefs.favorite_tools.len(), MAX_FAVORITE_TOOLS);
    let extra = Tool::ALL
        .into_iter()
        .find(|t| !prefs.favorite_tool_list().contains(t))
        .unwrap();
    assert!(
        !prefs.toggle_favorite_tool(extra),
        "a full bar takes no more"
    );
    assert!(!prefs.favorite_tool_list().contains(&extra));

    prefs.favorite_tools.clear();
    let prefs = prefs.normalized();
    assert!(
        prefs.favorite_tools.is_empty(),
        "the choice of none is kept"
    );
}

#[test]
fn normalizing_drops_unknown_and_repeated_favorites_and_keeps_the_order() {
    let prefs = Preferences {
        favorite_tools: vec![
            Tool::Rectangle.code(),
            "no_such_tool".to_owned(),
            Tool::TrendLine.code(),
            Tool::Rectangle.code(),
        ],
        ..Preferences::default()
    }
    .normalized();
    assert_eq!(
        prefs.favorite_tool_list(),
        vec![Tool::Rectangle, Tool::TrendLine]
    );
}

#[test]
fn favorites_move_along_the_bar_and_stop_at_its_ends() {
    let mut prefs = Preferences {
        favorite_tools: vec![Tool::Ray.code(), Tool::Arrow.code(), Tool::Text.code()],
        ..Preferences::default()
    };
    prefs.move_favorite_tool(2, -1);
    assert_eq!(
        prefs.favorite_tool_list(),
        vec![Tool::Ray, Tool::Text, Tool::Arrow]
    );
    prefs.move_favorite_tool(0, -1);
    prefs.move_favorite_tool(2, 1);
    prefs.move_favorite_tool(9, 1);
    assert_eq!(
        prefs.favorite_tool_list(),
        vec![Tool::Ray, Tool::Text, Tool::Arrow]
    );
    prefs.move_favorite_tool(0, 5);
    assert_eq!(prefs.favorite_tool_list().last(), Some(&Tool::Ray));
}
