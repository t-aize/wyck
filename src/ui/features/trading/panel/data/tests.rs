use super::*;

fn row(value: Option<f64>) -> Row {
    Row {
        key: String::new(),
        symbol: None,
        profit: None,
        kind: RowKind::Deal,
        cells: vec![value.map_or_else(Cell::empty, |v| Cell::num(v.to_string(), v))],
    }
}

fn values(rows: &[Row]) -> Vec<Option<f64>> {
    rows.iter()
        .map(|r| match r.cells[0].key {
            Key::Num(v) => Some(v),
            _ => None,
        })
        .collect()
}

#[test]
fn rows_sort_by_number_and_a_missing_value_stays_last() {
    let mut rows = vec![row(Some(2.0)), row(None), row(Some(-1.0)), row(Some(5.0))];
    sort_rows(&mut rows, 0, false);
    assert_eq!(values(&rows), [Some(-1.0), Some(2.0), Some(5.0), None]);
    sort_rows(&mut rows, 0, true);
    assert_eq!(values(&rows), [Some(5.0), Some(2.0), Some(-1.0), None]);
}

#[test]
fn text_sorts_without_regard_to_case() {
    let mut rows: Vec<Row> = ["b", "A", "c"]
        .iter()
        .map(|t| Row {
            cells: vec![Cell::text(*t)],
            ..row(None)
        })
        .collect();
    sort_rows(&mut rows, 0, false);
    let texts: Vec<&str> = rows.iter().map(|r| r.cells[0].text.as_str()).collect();
    assert_eq!(texts, ["A", "b", "c"]);
}

#[test]
fn a_profit_is_written_in_the_unit_chosen() {
    let money = |unit| profit_cell(unit, Some(12.5), Some(6.25), 1_000.0, "USD").text;
    assert_eq!(money(ProfitUnit::Money), "12.50 USD");
    assert_eq!(money(ProfitUnit::Pips), "+6.2 pips");
    assert_eq!(money(ProfitUnit::Percent), "+1.25%");
    assert_eq!(money(ProfitUnit::MoneyPips), "12.50 USD (+6.2 pips)");
    let waiting = profit_cell(ProfitUnit::Money, None, None, 1_000.0, "USD");
    assert_eq!(waiting.text, "...");
    assert_eq!(waiting.sum, None);
}

#[test]
fn totals_are_written_for_their_unit() {
    assert_eq!(total_text(1.5, Unit::Lots, "USD", 100.0), "1.5");
    assert_eq!(total_text(-3.0, Unit::Money, "USD", 100.0), "-3.00 USD");
    assert_eq!(total_text(2.0, Unit::Percent, "USD", 100.0), "+2.00%");
    assert_eq!(total_text(2.0, Unit::Percent, "USD", 0.0), "");
    assert_eq!(total_text(2.0, Unit::None, "USD", 100.0), "");
}

#[test]
fn a_table_is_written_as_csv_with_the_columns_that_show() {
    let table = Table {
        tab: Tab::History,
        columns: vec![
            ColView {
                index: 0,
                slot: 0,
                label: "Symbol",
                width: 100.0,
                right: false,
                sorted: None,
            },
            ColView {
                index: 1,
                slot: 1,
                label: "Note, long",
                width: 100.0,
                right: false,
                sorted: None,
            },
        ],
        rows: vec![Row {
            cells: vec![Cell::text("EURUSD"), Cell::text("a \"b\"")],
            ..row(None)
        }],
        totals: Vec::new(),
        unfiltered: 1,
        stats: None,
    };
    assert_eq!(
        to_csv(&table),
        "Symbol,\"Note, long\"\nEURUSD,\"a \"\"b\"\"\""
    );
}

#[test]
fn tones_follow_the_sign() {
    assert_eq!(Tone::of(1.0), Tone::Up);
    assert_eq!(Tone::of(-1.0), Tone::Down);
    assert_eq!(Tone::of(0.0), Tone::Plain);
}
