#![cfg(test)]

use super::*;

#[test]
fn ticks_export_a_price_and_no_volume() {
    let ticks: Vec<Tick> = (0..5)
        .map(|i| Tick {
            time_ms: MONDAY + i * 1_000,
            price: 100_000 + i * 5,
        })
        .collect();
    let f = Fixture::new(Series::Ticks(ticks), ChartSettings::default());
    let source = f.source();
    let a = source.available(Content::Prices);
    assert!(a.contains(&ColumnKey::Price));
    assert!(!a.contains(&ColumnKey::Volume) && !a.contains(&ColumnKey::Open));
    let o = options(|o| {
        o.columns = vec![ColumnKey::Time, ColumnKey::Price];
        o.millis = true;
    });
    let text = out(&f, &o);
    assert_eq!(lines(&text)[1], "2026-01-05T00:00:00.000Z,1.00000");
    assert_eq!(lines(&text)[3], "2026-01-05T00:00:02.000Z,1.00010");
}

#[test]
fn indicator_lines_are_columns_written_like_the_prices_they_follow() {
    let mut settings = ChartSettings::default();
    settings.studies.push(StudyConfig::new(StudyKind::Sma));
    settings.studies.push(StudyConfig::new(StudyKind::Rsi));
    let f = Fixture::new(Series::Bars(bars(60)), settings);
    let source = f.source();
    let available = source.available(Content::Prices);
    let studies: Vec<ColumnKey> = available
        .iter()
        .copied()
        .filter(|k| matches!(k, ColumnKey::Study { .. }))
        .collect();
    assert!(studies.len() >= 2, "{studies:?}");
    let sma = studies[0];
    assert!(
        source.column_name(sma).starts_with("SMA"),
        "{}",
        source.column_name(sma)
    );
    let o = options(|o| {
        o.columns = vec![ColumnKey::Index, sma, studies[1]];
        o.value_digits = 2;
        o.empty = Empty::Nan;
    });
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(l[1], "1,NaN,NaN", "no average yet on the first bar");
    let last: Vec<&str> = l.last().unwrap().split(',').collect();
    // The moving average is a price and reads like one; the RSI is a plain number.
    let average: f64 = last[1].parse().unwrap();
    assert!((0.99..1.01).contains(&average), "{last:?}");
    assert!(
        last[1].split('.').nth(1).is_some_and(|d| d.len() == 5),
        "{last:?}"
    );
    let rsi: f64 = last[2].parse().unwrap();
    assert!((0.0..=100.0).contains(&rsi), "{last:?}");
    assert!(
        last[2].split('.').nth(1).is_some_and(|d| d.len() == 2),
        "{last:?}"
    );
}

#[test]
fn a_renko_chart_can_export_its_bricks_with_their_size_and_direction() {
    let mut settings = ChartSettings {
        kind: ChartKind::Renko,
        ..ChartSettings::default()
    };
    settings.transform.renko_box = BoxSize::Fixed { price: 0.0002 };
    let f = Fixture::new(Series::Bars(bars(300)), settings);
    let source = f.source();
    let shown = source.available(Content::Shown);
    assert!(shown.contains(&ColumnKey::BoxSize));
    assert!(
        !source
            .available(Content::Prices)
            .contains(&ColumnKey::BoxSize)
    );
    let bricks = f.display.shown(&f.raw).len();
    let o = options(|o| {
        o.content = Content::Shown;
        o.columns = vec![ColumnKey::Time, ColumnKey::Direction, ColumnKey::BoxSize];
    });
    let text = out(&f, &o);
    assert_eq!(lines(&text).len(), bricks + 1);
    assert!(lines(&text)[1].ends_with(",0.00020"), "{}", lines(&text)[1]);
    assert!(text.contains(",up,") && text.contains(",down,"));
    // The prices are still the bars.
    assert_eq!(table(&source, &options(|_| {}), None).rows.len(), 300);
}

#[test]
fn a_tpo_chart_exports_the_levels_of_each_session() {
    let mut settings = ChartSettings {
        kind: ChartKind::Tpo,
        zone: Zone::Utc,
        ..ChartSettings::default()
    };
    settings.tpo.row_units = 10;
    let f = Fixture::new(Series::Bars(bars(600)), settings);
    let source = f.source();
    assert!(source.available(Content::Shown).contains(&ColumnKey::Poc));
    let o = options(|o| {
        o.content = Content::Shown;
        o.columns = vec![
            ColumnKey::Time,
            ColumnKey::Poc,
            ColumnKey::ValueAreaHigh,
            ColumnKey::ValueAreaLow,
            ColumnKey::InitialBalanceHigh,
            ColumnKey::Marks,
            ColumnKey::Periods,
            ColumnKey::PoorHigh,
            ColumnKey::BoxSize,
        ];
    });
    let t = table(&source, &o, None);
    assert_eq!(t.rows.len(), f.display.tpo.len());
    assert!(!t.rows.is_empty());
    for row in &t.rows {
        assert!(row.iter().all(|c| *c != Cell::Empty), "{row:?}");
        let (Cell::Price(poc), Cell::Price(vah), Cell::Price(val)) = (&row[1], &row[2], &row[3])
        else {
            panic!("{row:?}");
        };
        assert!(val <= poc && poc <= vah, "{val} {poc} {vah}");
    }
}

#[test]
fn the_columns_derived_from_the_prices_are_worked_out() {
    let f = Fixture::bars(3);
    let o = options(|o| {
        o.columns = vec![
            ColumnKey::Index,
            ColumnKey::Change,
            ColumnKey::ChangePercent,
            ColumnKey::Range,
            ColumnKey::Body,
            ColumnKey::Typical,
            ColumnKey::Median,
            ColumnKey::Direction,
            ColumnKey::Symbol,
            ColumnKey::Timeframe,
        ];
        o.value_digits = 4;
    });
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(
        l[0],
        "N,Change,Change %,Range,Body,Typical,Median,Direction,Symbol,Timeframe"
    );
    // The first row has nothing before it. Its typical price is 300020 / 3 in whole raw units.
    assert_eq!(l[1], "1,,,0.00050,0.00010,1.00006,1.00005,up,EURUSD,M5");
    // 10 raw units on a close of 100010 is 0.01 percent.
    assert_eq!(
        l[2],
        "2,0.00010,0.0100,0.00050,0.00010,1.00016,1.00015,up,EURUSD,M5"
    );
}
