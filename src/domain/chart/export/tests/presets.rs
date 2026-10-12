#![cfg(test)]

use super::*;

#[test]
fn file_names_are_built_from_the_pattern_and_kept_safe() {
    let f = Fixture::bars(10);
    let source = f.source();
    let o = options(|o| o.file_name = "{symbol}-{timeframe}_{type}_{from}_{to}".to_owned());
    let t = table(&source, &o, None);
    assert_eq!(
        file_name(&source, &o, &t),
        "EURUSD-M5_prices_20260105_20260105.csv"
    );
    let dated = options(|o| {
        o.file_name = "{date}/..\\x:y".to_owned();
        o.format = Format::Json;
    });
    let t = table(&source, &dated, None);
    let name = file_name(&source, &dated, &t);
    assert!(
        !name.contains('/') && !name.contains('\\') && !name.contains(':'),
        "{name}"
    );
    assert!(name.ends_with(".json"));
    let empty = ExportOptions {
        file_name: "  ".to_owned(),
        ..ExportOptions::default()
    }
    .normalized();
    assert_eq!(empty.file_name, "{symbol}_{timeframe}_{from}_{to}");
}

#[test]
fn every_built_in_preset_writes_something_and_survives_being_saved() {
    let f = Fixture::bars(20);
    let presets = builtin_presets();
    assert!(presets.len() >= 6);
    let mut names: Vec<_> = presets.iter().map(|p| p.name.clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), presets.len());
    for preset in &presets {
        let o = preset.options.clone().normalized();
        let text = out(&f, &o);
        assert!(!text.trim().is_empty(), "{}", preset.name);
        let saved = toml::to_string(preset).unwrap();
        let back: Preset = toml::from_str(&saved).unwrap();
        assert_eq!(back.options.normalized(), o, "{}", preset.name);
    }
}

#[test]
fn the_trading_platform_presets_write_what_those_platforms_read() {
    let f = Fixture::bars(2);
    let find = |name: &str| {
        builtin_presets()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
            .options
            .normalized()
    };
    let nt = out(&f, &find("NinjaTrader"));
    assert_eq!(
        lines(&nt)[0],
        "20260105 000000;1.00000;1.00030;0.99980;1.00010;1"
    );
    let mt = out(&f, &find("MetaTrader"));
    assert_eq!(
        lines(&mt)[0],
        "2026.01.05,00:00,1.00000,1.00030,0.99980,1.00010,1"
    );
    let sheet = out(&f, &find("Spreadsheet"));
    assert!(sheet.starts_with('\u{feff}'));
    assert!(
        lines(&sheet)[1].starts_with("2026-01-05 00:00:00;1,00000;"),
        "{sheet}"
    );
}

#[test]
fn nothing_loaded_exports_only_the_header() {
    let f = Fixture::new(Series::Bars(Vec::new()), ChartSettings::default());
    let text = out(&f, &options(|_| {}));
    assert_eq!(lines(&text), vec!["Time,Open,High,Low,Close,Volume"]);
    let sql = out(&f, &options(|o| o.format = Format::Sql));
    assert!(sql.is_empty());
}

#[test]
fn the_notes_of_a_preview_describe_every_row_not_only_the_ones_built() {
    let f = Fixture::bars(100);
    let o = options(|_| {});
    let full = table(&f.source(), &o, None);
    let preview = table(&f.source(), &o, Some(3));
    assert_eq!(preview.rows.len(), 3);
    assert_eq!(preview.notes, full.notes);
    let name = |t: &Table| file_name(&f.source(), &o, t);
    assert_eq!(name(&preview), name(&full));
}

#[test]
fn a_preview_builds_few_rows_and_says_how_many_there_are() {
    let f = Fixture::bars(500);
    let t = table(&f.source(), &options(|_| {}), Some(PREVIEW_ROWS));
    assert_eq!(t.rows.len(), PREVIEW_ROWS);
    assert_eq!(t.total_rows, 500);
}
