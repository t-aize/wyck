use super::*;
use crate::domain::indicators::StudyKind;

#[test]
fn a_lower_alert_limit_blocks_new_alerts_without_removing_saved_ones() {
    let mut book = AlertBook::default();
    assert!(
        book.add_with_limit(7, "EURUSD", 1.1, Condition::Crossing, 0, 2)
            .is_some()
    );
    assert!(
        book.add_with_limit(7, "EURUSD", 1.2, Condition::Crossing, 0, 2)
            .is_some()
    );
    assert!(
        book.add_with_limit(7, "EURUSD", 1.3, Condition::Crossing, 0, 2)
            .is_none()
    );
    assert_eq!(book.normalized().alerts.len(), 2);
}

#[test]
fn crossings_are_told_by_the_price_before_and_now() {
    assert!(Condition::CrossingUp.crossed(1.0, 2.0, 1.5));
    assert!(!Condition::CrossingUp.crossed(2.0, 1.0, 1.5));
    assert!(Condition::CrossingDown.crossed(2.0, 1.0, 1.5));
    assert!(
        Condition::Crossing.crossed(1.0, 1.5, 1.5),
        "touching counts"
    );
    assert!(!Condition::Crossing.crossed(1.5, 1.6, 1.5));
}

#[test]
fn a_v1_file_loads_as_a_price_alert_and_a_repeat_becomes_a_trigger() {
    let old = "[[alerts]]\nid = 3\nsymbol_id = 7\nsymbol = \"EURUSD\"\nprice = 1.085\n\
               condition = \"crossing_up\"\nrepeat = true\nmessage = \"Breakout\"\n\
               [[alerts]]\nid = 4\nsymbol_id = 7\nsymbol = \"EURUSD\"\nprice = 1.09\n";
    let book: AlertBook = toml::from_str(old).unwrap();
    let book = book.normalized();
    assert_eq!(book.schema_version, SCHEMA_VERSION);
    let first = &book.alerts[0];
    assert_eq!(first.source, Source::default());
    assert_eq!(first.condition, Condition::CrossingUp);
    assert_eq!(first.trigger, Trigger::EveryTime);
    assert!(first.repeats() && !book.alerts[1].repeats());
    assert!(!first.needs_bars());
    assert_eq!(first.text(5), "Breakout");
}

#[test]
fn a_saved_book_reads_back_with_an_indicator_and_a_drawing() {
    let mut book = AlertBook::default();
    let mut rsi = Alert::price(0, 7, "EURUSD", 70.0, 5);
    rsi.source = Source::Indicator {
        study: Box::new(StudyConfig::new(StudyKind::Rsi)),
        plot: 0,
    };
    rsi.timeframe = "H1".into();
    rsi.trigger = Trigger::OncePerBarClose;
    book.insert(rsi, 10);
    let mut line = Alert::price(0, 7, "EURUSD", 0.0, 6);
    line.versus = Some(Source::Drawing { id: 12 });
    book.insert(line, 10);
    let text = toml::to_string_pretty(&book).unwrap();
    let back: AlertBook = toml::from_str(&text).unwrap();
    let back = back.normalized();
    assert_eq!(back.alerts, book.alerts);
    assert!(back.alerts[0].needs_bars());
    assert_eq!(back.alerts[1].drawing(), Some(12));
    assert!(!back.alerts[1].needs_bars());
}

#[test]
fn a_broken_book_is_repaired() {
    let broken: AlertBook = toml::from_str(
        "[[alerts]]\nid = 3\nsymbol_id = 7\nsymbol = \"A\"\nprice = 1.0\n\
         [[alerts]]\nid = 3\nsymbol_id = 7\nsymbol = \"B\"\nprice = 2.0\n\
         [[alerts]]\nid = 4\nsymbol_id = 7\nsymbol = \"C\"\nprice = -1.0\n",
    )
    .unwrap();
    let repaired = broken.normalized();
    assert_eq!(repaired.alerts.len(), 2);
    assert_ne!(repaired.alerts[0].id, repaired.alerts[1].id);
    assert!(repaired.next_id > repaired.alerts.iter().map(|a| a.id).max().unwrap());
}

#[test]
fn an_alert_says_what_it_watches_and_fills_in_its_message() {
    let mut book = AlertBook::default();
    let id = book
        .add(7, "EURUSD", 1.085, Condition::CrossingUp, 0)
        .unwrap();
    assert_eq!(book.alerts[0].text(5), "EURUSD crossing up 1.08500");
    book.get_mut(id).unwrap().message = "{symbol} at {value}, level {level}".into();
    assert_eq!(
        book.alerts[0].render(Some(1.0851), 5),
        "EURUSD at 1.08510, level 1.08500"
    );
    let mut rsi = Alert::price(1, 7, "EURUSD", 70.0, 0);
    rsi.source = Source::Indicator {
        study: Box::new(StudyConfig::new(StudyKind::Rsi)),
        plot: 0,
    };
    rsi.condition = Condition::CrossingUp;
    assert_eq!(rsi.text(5), "EURUSD RSI crossing up 70");
    let mut moving = Alert::price(2, 7, "EURUSD", 1.0, 0);
    moving.condition = Condition::MovesBy;
    moving.amount = 0.5;
    moving.minutes = 15;
    assert_eq!(moving.text(5), "EURUSD moves by 0.5% in 15 min");
}

#[test]
fn firing_counts_stops_a_one_time_alert_and_keeps_a_history() {
    let mut book = AlertBook::default();
    let once = book.add(7, "EURUSD", 1.1, Condition::Crossing, 0).unwrap();
    let again = book.add(7, "EURUSD", 1.2, Condition::Crossing, 0).unwrap();
    book.get_mut(again).unwrap().trigger = Trigger::EveryTime;
    let fired = book.fire(once, "one".into(), Some(1.1), 5, 10).unwrap();
    assert!(!fired.active && fired.fired_count == 1);
    assert!(book.fire(again, "two".into(), None, 5, 11).unwrap().active);
    assert_eq!(book.history[0].text, "two");
    let first = &book.history[1];
    assert_eq!(
        (first.symbol_id, first.watched.as_str(), first.value),
        (7, "Bid", Some(1.1))
    );
    assert_eq!(first.condition, "Crossing");
    assert_eq!(book.firings(once).count(), 1);
    assert_eq!(book.watched(), vec![7]);
    for n in 0..(MAX_HISTORY + 20) {
        book.fire(again, n.to_string(), None, 5, 20 + n as i64);
    }
    assert_eq!(book.history.len(), MAX_HISTORY);
}

#[test]
fn an_expired_alert_stops_and_a_snoozed_one_waits() {
    let mut book = AlertBook::default();
    let id = book.add(7, "EURUSD", 1.1, Condition::Crossing, 0).unwrap();
    {
        let a = book.get_mut(id).unwrap();
        a.expires_at = Some(100);
        a.snoozed_until = Some(50);
        assert!(a.is_snoozed(10) && !a.is_snoozed(50));
    }
    assert!(book.expire(99).is_empty());
    assert_eq!(book.expire(100), vec![id]);
    assert!(book.watched().is_empty());
}

#[test]
fn a_spread_alert_reads_in_pips_and_a_profit_alert_in_money() {
    let spread = Alert::spread(1, 7, "EURUSD", 0.0001, 2.5, 0);
    assert_eq!(
        spread.describe(5),
        "EURUSD spread becomes greater than 2.5 pips"
    );
    assert_eq!(spread.value_text(3.04, 5), "3.0");
    let mut profit = Alert::pnl(2, 0, "", PnlScope::Account, -100.0, 0);
    assert_eq!(
        profit.describe(5),
        "account profit becomes smaller than -100.00"
    );
    assert_eq!(profit.value_text(-100.456, 5), "-100.46");
    profit.symbol = "GBPUSD".into();
    profit.source = Source::Pnl {
        scope: PnlScope::Symbol,
    };
    profit.symbol_id = 9;
    assert_eq!(
        profit.describe(5),
        "GBPUSD profit becomes smaller than -100.00"
    );
    profit.message = "{symbol}: {value} ({level})".into();
    assert_eq!(profit.render(Some(-101.0), 5), "GBPUSD: -101.00 (-100.00)");
}

#[test]
fn spread_and_profit_alerts_survive_a_round_trip_and_a_repair() {
    let mut book = AlertBook::default();
    let spread = Alert::spread(0, 7, "EURUSD", 0.0001, 2.0, 0);
    let account = Alert::pnl(0, 0, "", PnlScope::Account, -50.0, 0);
    let position = Alert::pnl(0, 7, "EURUSD", PnlScope::Position { id: 41 }, 0.0, 0);
    for alert in [spread, account, position] {
        book.insert(alert, 100).unwrap();
    }
    let text = toml::to_string_pretty(&book).unwrap();
    let back: AlertBook = toml::from_str(&text).unwrap();
    assert_eq!(back.normalized(), book);

    // What cannot mean anything is put right: no direction for a profit, no bars for a
    // spread, a pip that is not a size.
    let mut wild = book.clone();
    wild.alerts[0].source = Source::Spread { pip: f64::NAN };
    wild.alerts[0].condition = Condition::ChangesDirection;
    wild.alerts[0].trigger = Trigger::OncePerBarClose;
    wild.alerts[1].condition = Condition::MovesBy;
    wild.alerts[1].versus = Some(Source::default());
    let wild = wild.normalized();
    assert_eq!(wild.alerts[0].condition, Condition::Crossing);
    assert_eq!(wild.alerts[0].trigger, Trigger::EveryTime);
    assert!(matches!(wild.alerts[0].source, Source::Spread { pip } if pip == 0.0001));
    assert_eq!(wild.alerts[1].condition, Condition::Crossing);
    assert!(wild.alerts[1].versus.is_none());
    assert_eq!(
        wild.alerts.len(),
        3,
        "a level of 0 and no symbol are fine for a profit"
    );
}

#[test]
fn only_alerts_on_prices_and_spreads_ask_for_a_symbol_feed() {
    let mut book = AlertBook::default();
    book.insert(Alert::spread(0, 7, "EURUSD", 0.0001, 2.0, 0), 100);
    book.insert(Alert::pnl(0, 8, "GBPUSD", PnlScope::Symbol, 5.0, 0), 100);
    book.insert(Alert::pnl(0, 0, "", PnlScope::Account, 5.0, 0), 100);
    assert_eq!(book.watched(), vec![7]);
}

#[test]
fn a_history_written_before_the_log_columns_still_loads() {
    let old = r#"
        schema_version = 2
        next_id = 1
        [[history]]
        alert = 3
        symbol = "EURUSD"
        at = 1000
        text = "EURUSD crossed 1.1"
    "#;
    let book: AlertBook = toml::from_str(old).unwrap();
    let firing = &book.history[0];
    assert_eq!(firing.symbol_id, 0);
    assert!(firing.watched.is_empty() && firing.value.is_none());
}
