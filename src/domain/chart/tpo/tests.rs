use super::*;

const MIN: i64 = 60_000;
/// Monday 2026-01-05 00:00 UTC.
const MONDAY: i64 = 1_767_571_200_000;

fn bar(time_ms: i64, low: i64, high: i64) -> Bar {
    Bar {
        time_ms,
        open: low,
        high,
        low,
        close: high,
        volume: 1,
    }
}

fn settings() -> TpoSettings {
    TpoSettings {
        row_units: 1,
        ..TpoSettings::default()
    }
}

/// One bar of a minute per period of thirty, so a bar is a letter.
fn letters(prices: &[(i64, i64)]) -> Vec<Bar> {
    prices
        .iter()
        .enumerate()
        .map(|(i, (low, high))| bar(MONDAY + i as i64 * 30 * MIN, *low, *high))
        .collect()
}

fn utc(bars: &[Bar], settings: &TpoSettings) -> Vec<Profile> {
    build(bars, settings, Zone::Utc, 1)
}

#[test]
fn each_period_marks_the_rows_its_bars_covered() {
    let bars = letters(&[(10, 12), (11, 13), (12, 12)]);
    let profile = &utc(&bars, &settings())[0];
    assert_eq!(profile.periods, 3);
    assert_eq!(profile.low_row, 10);
    // Rows 10 to 13: A covers 10-12, B 11-13, C only 12.
    assert_eq!(
        profile.rows,
        vec![vec![0], vec![0, 1], vec![0, 1, 2], vec![1]]
    );
    assert_eq!(profile.marks, 7);
    assert_eq!((profile.low, profile.high), (10, 13));
}

#[test]
fn several_bars_of_one_period_mark_a_row_once() {
    let bars = vec![
        bar(MONDAY, 10, 12),
        bar(MONDAY + 10 * MIN, 10, 12),
        bar(MONDAY + 20 * MIN, 11, 11),
    ];
    let profile = &utc(&bars, &settings())[0];
    assert_eq!(profile.periods, 1);
    assert!(
        profile.rows.iter().all(|r| r == &vec![0]),
        "{:?}",
        profile.rows
    );
}

#[test]
fn a_period_is_never_shorter_than_a_bar() {
    // Hour bars with a thirty minute period: a bar is a letter, not two.
    let bars: Vec<Bar> = (0..4).map(|i| bar(MONDAY + i * 60 * MIN, 10, 11)).collect();
    let profile = &utc(&bars, &settings())[0];
    assert_eq!(profile.periods, 4);
    assert_eq!(profile.rows[0], vec![0, 1, 2, 3]);
}

#[test]
fn the_point_of_control_is_the_busiest_row_and_ties_go_to_the_middle() {
    let rows = vec![vec![0], vec![0, 1], vec![0, 1, 2], vec![0, 1], vec![0]];
    assert_eq!(point_of_control(&rows), 2);
    // Two rows tie at the top: the one nearer the middle of the profile wins.
    let tie = vec![vec![0, 1], vec![0], vec![0], vec![0, 1], vec![0], vec![0]];
    assert_eq!(point_of_control(&tie), 3);
    // Equally near: the lower.
    let even = vec![vec![0], vec![0, 1], vec![0, 1], vec![0]];
    assert_eq!(point_of_control(&even), 1);
}

#[test]
fn the_value_area_grows_by_pairs_from_the_point_of_control() {
    // Counts 1, 2, 5, 3, 1: total 12, 70% needs 9. From the 5, the two rows above
    // (3 + 1) beat the two below (2 + 1), so 5 + 3 + 1 = 9 is enough.
    let rows: Vec<Vec<u16>> = [1usize, 2, 5, 3, 1]
        .iter()
        .map(|n| (0..*n as u16).collect())
        .collect();
    assert_eq!(point_of_control(&rows), 2);
    assert_eq!(value_area(&rows, 2, 70), (2, 4));
    // All of it.
    assert_eq!(value_area(&rows, 2, 95), (0, 4));
    // Equal sides go up.
    let flat: Vec<Vec<u16>> = [2usize, 2, 4, 2, 2]
        .iter()
        .map(|n| (0..*n as u16).collect())
        .collect();
    assert_eq!(value_area(&flat, 2, 70).1, 4);
}

#[test]
fn the_value_area_stays_inside_the_profile() {
    let one: Vec<Vec<u16>> = vec![vec![0, 1, 2]];
    assert_eq!(value_area(&one, 0, 70), (0, 0));
    let two: Vec<Vec<u16>> = vec![vec![0], vec![0, 1, 2, 3]];
    let (low, high) = value_area(&two, 1, 95);
    assert!(low <= 1 && high <= 1);
    // Near the bottom, with only one row below.
    let near: Vec<Vec<u16>> = vec![vec![0], vec![0, 1, 2], vec![0], vec![0], vec![0]];
    let (low, high) = value_area(&near, 1, 95);
    assert_eq!(low, 0);
    assert!(high <= 4);
}

#[test]
fn single_prints_are_runs_of_one_mark_and_the_ends_are_tails() {
    let rows: Vec<Vec<u16>> = [1usize, 1, 3, 2, 1, 1, 1, 4, 1]
        .iter()
        .map(|n| (0..*n as u16).collect())
        .collect();
    let found = single_prints(&rows, 2);
    assert_eq!(
        found,
        vec![
            SinglePrint {
                first: 0,
                last: 1,
                tail: true
            },
            SinglePrint {
                first: 4,
                last: 6,
                tail: false
            },
        ]
    );
    // A single row does not count when two are asked for, and does with one.
    assert_eq!(single_prints(&rows, 1).len(), 3);
    assert!(single_prints(&rows, 4).is_empty());
}

#[test]
fn poor_ends_are_the_ones_more_than_one_period_touched() {
    let bars = letters(&[(10, 14), (10, 13), (11, 14)]);
    let profile = &utc(&bars, &settings())[0];
    // The high 14 was touched by A and C, the low 10 by A and B.
    assert!(profile.poor_high);
    assert!(profile.poor_low);
    let clean = letters(&[(10, 14), (11, 13), (12, 13)]);
    let profile = &utc(&clean, &settings())[0];
    assert!(!profile.poor_high);
    assert!(!profile.poor_low);
}

#[test]
fn the_initial_balance_is_the_range_of_the_first_periods() {
    let bars = letters(&[(10, 12), (11, 15), (9, 20), (30, 31)]);
    let profile = &utc(&bars, &settings())[0];
    assert_eq!(profile.initial_balance, Some((10, 15)));
    let one = TpoSettings {
        ib_periods: 1,
        ..settings()
    };
    assert_eq!(utc(&bars, &one)[0].initial_balance, Some((10, 12)));
}

#[test]
fn open_and_close_are_where_the_session_started_and_ended() {
    let bars = vec![
        Bar {
            open: 12,
            close: 10,
            ..bar(MONDAY, 10, 14)
        },
        Bar {
            open: 11,
            close: 13,
            ..bar(MONDAY + 30 * MIN, 10, 14)
        },
    ];
    let profile = &utc(&bars, &settings())[0];
    assert_eq!(profile.open, 12);
    assert_eq!(profile.close, 13);
    assert_eq!(profile.open_row, 2);
    assert_eq!(profile.close_row, 3);
}

#[test]
fn a_new_day_is_a_new_profile_and_the_hours_make_shorter_ones() {
    let mut bars = Vec::new();
    for i in 0..96 {
        bars.push(bar(MONDAY + i * 30 * MIN, 10, 12));
    }
    assert_eq!(utc(&bars, &settings()).len(), 2);
    let hours = TpoSettings {
        session: SessionKind::Hours,
        session_hours: 12,
        ..settings()
    };
    assert_eq!(utc(&bars, &hours).len(), 4);
}

#[test]
fn weeks_start_on_monday_and_months_on_the_first() {
    let day = 86_400_000;
    let week = TpoSettings {
        session: SessionKind::Week,
        ..settings()
    };
    // Sunday and Monday are different weeks; Monday to Sunday is one.
    let sunday = MONDAY - day;
    let bars = vec![
        bar(sunday, 1, 2),
        bar(MONDAY, 1, 2),
        bar(MONDAY + 6 * day, 1, 2),
        bar(MONDAY + 7 * day, 1, 2),
    ];
    assert_eq!(utc(&bars, &week).len(), 3);
    let month = TpoSettings {
        session: SessionKind::Month,
        ..settings()
    };
    // January and February of 2026.
    let bars = vec![
        bar(MONDAY, 1, 2),
        bar(MONDAY + 20 * day, 1, 2),
        bar(MONDAY + 30 * day, 1, 2),
    ];
    assert_eq!(utc(&bars, &month).len(), 2);
}

#[test]
fn a_session_can_start_at_another_time_of_day() {
    // Two bars at 21:00 and 23:00 UTC: one day, or two when the day starts at 22:00.
    let bars = vec![
        bar(MONDAY + 21 * 60 * MIN, 1, 2),
        bar(MONDAY + 23 * 60 * MIN, 1, 2),
    ];
    assert_eq!(utc(&bars, &settings()).len(), 1);
    let late = TpoSettings {
        session_start: 22 * 60,
        ..settings()
    };
    assert_eq!(utc(&bars, &late).len(), 2);
}

#[test]
fn the_rows_are_taller_when_asked_and_when_the_session_is_too_wide() {
    let bars = letters(&[(0, 100)]);
    let coarse = TpoSettings {
        row_units: 10,
        ..settings()
    };
    let profile = &utc(&bars, &coarse)[0];
    assert_eq!(profile.row_size, 10);
    assert_eq!(profile.rows.len(), 11);
    let wide = letters(&[(0, 100_000)]);
    let profile = &utc(&wide, &settings())[0];
    assert!(
        profile.rows.len() as i64 <= MAX_ROWS + 1,
        "{}",
        profile.rows.len()
    );
    assert!(profile.row_size > 1);
}

#[test]
fn an_automatic_row_gives_about_forty_rows_and_a_round_height() {
    let bars = letters(&[(0, 2_000), (0, 2_000)]);
    let auto = TpoSettings::default();
    let profile = &utc(&bars, &auto)[0];
    assert!(profile.row_size >= 50, "{}", profile.row_size);
    assert!((profile.rows.len() as i64) < 60, "{}", profile.rows.len());
    assert_eq!(nice_ceiling(0.3), 1);
    assert_eq!(nice_ceiling(3.0), 5);
    assert_eq!(nice_ceiling(51.0), 100);
    assert_eq!(nice_ceiling(f64::NAN), 1);
}

#[test]
fn a_profile_is_one_element_of_the_series() {
    let bars = letters(&[(10, 20), (12, 25)]);
    let profile = &utc(&bars, &settings())[0];
    let e = element(profile);
    assert_eq!((e.time_ms, e.low, e.high), (MONDAY, 10, 25));
    assert_eq!(e.volume, 2);
    assert_eq!(e.close, bars[1].close);
}

#[test]
fn a_time_of_day_is_read_and_written_plainly() {
    assert_eq!(parse_clock("09:30"), Some(570));
    assert_eq!(parse_clock(" 9:30 "), Some(570));
    assert_eq!(parse_clock("22"), Some(1320));
    assert_eq!(parse_clock("930"), Some(570));
    assert_eq!(parse_clock("0"), Some(0));
    assert_eq!(parse_clock("24:00"), None);
    assert_eq!(parse_clock("12:75"), None);
    assert_eq!(parse_clock("noon"), None);
    assert_eq!(parse_clock(""), None);
    assert_eq!(format_clock(570), "09:30");
    assert_eq!(format_clock(0), "00:00");
    assert_eq!(parse_clock(&format_clock(1_439)), Some(1_439));
}

#[test]
fn periods_are_lettered_and_wrap() {
    assert_eq!(letter(0), 'A');
    assert_eq!(letter(25), 'Z');
    assert_eq!(letter(26), 'a');
    assert_eq!(letter(51), 'z');
    assert_eq!(letter(52), 'A');
}

#[test]
fn nothing_in_gives_nothing_out() {
    assert!(utc(&[], &settings()).is_empty());
}

#[test]
fn the_settings_are_repaired_and_saved() {
    let broken = TpoSettings {
        session_hours: 0,
        session_start: 99_999,
        period_minutes: 0,
        value_area_percent: 5,
        ib_periods: 0,
        single_min_rows: 0,
        ..TpoSettings::default()
    }
    .normalized();
    assert_eq!(broken.session_hours, 1);
    assert_eq!(broken.session_start, 24 * 60 - 1);
    assert_eq!(broken.period_minutes, 1);
    assert_eq!(broken.value_area_percent, 50);
    assert_eq!(broken.ib_periods, 1);
    assert_eq!(broken.single_min_rows, 1);
    let text = toml::to_string(&broken).unwrap();
    assert_eq!(toml::from_str::<TpoSettings>(&text).unwrap(), broken);
    assert_eq!(
        toml::from_str::<TpoSettings>("").unwrap(),
        TpoSettings::default(),
        "a file from before the chart type reads with the defaults"
    );
}
