#![cfg(test)]

use super::*;

#[test]
fn the_json_is_valid_and_typed() {
    let f = Fixture::bars(3);
    let o = options(|o| {
        o.format = Format::Json;
        o.header_case = HeaderCase::Lower;
        o.columns = vec![
            ColumnKey::Time,
            ColumnKey::Close,
            ColumnKey::Volume,
            ColumnKey::Direction,
            ColumnKey::Change,
        ];
    });
    let text = out(&f, &o);
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    let rows = value.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["time"], "2026-01-05T00:00:00Z");
    assert_eq!(rows[0]["close"], 1.0001);
    assert_eq!(rows[0]["volume"], 1);
    assert_eq!(rows[0]["direction"], "up");
    assert!(rows[0]["change"].is_null(), "nothing before the first bar");
    let unix = options(|o| {
        o.format = Format::Json;
        o.time_format = TimeFormat::UnixSeconds;
        o.decimal = Decimal::Comma;
        o.notes = true;
    });
    let value: serde_json::Value = serde_json::from_str(&out(&f, &unix)).unwrap();
    assert_eq!(value["meta"]["symbol"], "EURUSD");
    assert_eq!(value["data"][0]["Time"], 1_767_571_200);
    assert_eq!(
        value["data"][0]["Close"], 1.0001,
        "JSON always uses a point"
    );
    let compact = options(|o| {
        o.format = Format::Json;
        o.json_pretty = false;
    });
    assert_eq!(lines(&out(&f, &compact)).len(), 1);
}

#[test]
fn json_lines_are_one_object_a_line() {
    let f = Fixture::bars(4);
    let o = options(|o| o.format = Format::JsonLines);
    let text = out(&f, &o);
    assert_eq!(lines(&text).len(), 4);
    for line in lines(&text) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(v.is_object());
    }
    assert_eq!(o.extension(), "jsonl");
    let empty = options(|o| o.format = Format::Json);
    let none = Fixture::new(Series::Bars(Vec::new()), ChartSettings::default());
    assert_eq!(out(&none, &empty).trim(), "[]");
}

#[test]
fn the_markdown_table_has_a_header_an_alignment_row_and_escaped_pipes() {
    let f = Fixture::bars(2);
    let o = options(|o| {
        o.format = Format::Markdown;
        o.columns = vec![ColumnKey::Time, ColumnKey::Close, ColumnKey::Direction];
        o.renames
            .insert("close".to_owned(), "Last | price".to_owned());
    });
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(l[0], "| Time | Last \\| price | Direction |");
    assert_eq!(l[1], "| :-- | --: | :-- |");
    assert!(l[2].starts_with("| 2026-01-05T00:00:00Z | 1.00010 | up |"));
    assert_eq!(o.extension(), "md");
}

#[test]
fn the_sql_is_batched_quoted_and_uses_the_table_asked() {
    let f = Fixture::bars(SQL_BATCH + 3);
    let o = options(|o| {
        o.format = Format::Sql;
        o.table_name = "my bars".to_owned();
        o.columns = vec![
            ColumnKey::Time,
            ColumnKey::Close,
            ColumnKey::Change,
            ColumnKey::Symbol,
        ];
        o.renames.insert("symbol".to_owned(), "sym".to_owned());
        o.notes = true;
    });
    let text = out(&f, &o);
    assert!(text.contains("-- symbol: EURUSD"));
    assert_eq!(
        text.matches("INSERT INTO my_bars (time, close, change, sym) VALUES")
            .count(),
        2
    );
    assert!(text.contains("('2026-01-05T00:00:00Z', 1.00010, NULL, 'EURUSD'),"));
    assert_eq!(
        text.matches("EURUSD'),\n").count() + text.matches("EURUSD');\n").count(),
        SQL_BATCH + 3
    );
    assert_eq!(text.matches("');\n").count(), 2, "each statement ends once");
    assert_eq!(identifier("9lives", "x"), "x");
    assert_eq!(identifier("a b-c", "x"), "a_b_c");
    // A quote in a value is doubled.
    let quoted = Cell::Text("O'Brien".to_owned());
    let writer = Writer {
        options: &o,
        digits: 5,
        zone: Zone::Utc,
        decimal: '.',
    };
    assert_eq!(sql_literal(&quoted, &writer), "'O''Brien'");
}

#[test]
fn notes_open_a_delimited_file_as_comments() {
    let f = Fixture::bars(3);
    let o = options(|o| o.notes = true);
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(l[0], "# symbol: EURUSD");
    assert!(l.contains(&"# timeframe: M5"));
    assert!(l.contains(&"# rows: 3"));
    assert!(l.contains(&"# from: 2026-01-05 00:00:00"));
    assert!(l.iter().any(|x| x.starts_with("# zone: UTC")));
}

#[test]
fn what_is_not_there_is_written_as_asked() {
    let f = Fixture::bars(2);
    for (empty, expected) in [
        (Empty::Blank, "1,,"),
        (Empty::Nan, "1,NaN,NaN"),
        (Empty::Null, "1,null,null"),
        (Empty::Zero, "1,0,0"),
    ] {
        let o = options(|o| {
            o.columns = vec![
                ColumnKey::Index,
                ColumnKey::Change,
                ColumnKey::ChangePercent,
            ];
            o.empty = empty;
            o.header = false;
        });
        assert_eq!(lines(&out(&f, &o))[0], expected, "{empty:?}");
    }
}
