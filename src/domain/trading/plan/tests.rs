use super::*;

#[test]
fn a_volume_is_cut_by_shares_in_whole_steps() {
    // 1 lot = 10_000_000 hundredths of a unit on a forex pair; min 0.01 lot, step 0.01 lot.
    let (min, step) = (100_000, 100_000);
    let (v, kept) = split_volume(10_000_000, &[50.0, 30.0, 20.0], min, step);
    assert_eq!(v, vec![5_000_000, 3_000_000, 2_000_000]);
    assert_eq!(kept, vec![0, 1, 2]);
    // A volume that does not divide: the rest goes to the first leg.
    let (v, _) = split_volume(1_000_000, &[50.0, 30.0, 20.0], min, step);
    assert_eq!(v.iter().sum::<i64>(), 1_000_000);
    assert!(v.iter().all(|x| x % step == 0));
    assert!(v[0] >= v[1]);
}

#[test]
fn legs_too_small_to_trade_are_merged_into_the_one_before() {
    let (min, step) = (100_000, 100_000);
    // 0.02 lot cannot make three legs of 0.01: two legs remain.
    let (v, kept) = split_volume(200_000, &[50.0, 30.0, 20.0], min, step);
    assert_eq!(v.iter().sum::<i64>(), 200_000);
    assert!(v.iter().all(|x| *x >= min));
    assert_eq!(v.len(), kept.len());
    assert!(kept.len() < 3 && kept[0] == 0);
    // One minimum lot cannot be split at all.
    let (v, kept) = split_volume(100_000, &[50.0, 50.0], min, step);
    assert_eq!(v, vec![100_000]);
    assert_eq!(kept.len(), 1);
    // Nothing to split.
    assert!(split_volume(0, &[100.0], min, step).0.is_empty());
}

#[test]
fn a_plan_needs_shares_that_add_up_and_targets_that_climb() {
    let plan = ExitPlan::default();
    assert_eq!(plan.problem(), None);
    let mut bad = plan.clone();
    bad.legs[2].share = 30.0;
    assert!(matches!(bad.problem(), Some(ExitsProblem::SharesTotal(_))));
    let mut bad = plan.clone();
    bad.legs[1].target_r = 0.5;
    assert_eq!(bad.problem(), Some(ExitsProblem::TargetsNotRising));
    let mut zero = plan;
    zero.legs[0].share = 0.0;
    assert!(zero.problem().is_some());
}

#[test]
fn a_wild_plan_is_put_back_in_range() {
    let plan = ExitPlan {
        legs: vec![
            Leg {
                share: f64::NAN,
                target_r: -1.0,
            };
            7
        ],
        break_even: BreakEven {
            on: true,
            after_leg: 9,
            offset_pips: -3.0,
        },
        oco_pips: f64::INFINITY,
        ..ExitPlan::default()
    }
    .normalized();
    assert_eq!(plan.legs.len(), MAX_LEGS);
    assert!(
        plan.legs
            .iter()
            .all(|l| l.share == 0.0 && l.target_r == 0.0)
    );
    assert!(plan.break_even.after_leg <= 2);
    assert_eq!(plan.break_even.offset_pips, 0.0);
    assert_eq!(plan.oco_pips, 0.0);
    let empty = ExitPlan {
        legs: Vec::new(),
        ..ExitPlan::default()
    }
    .normalized();
    assert_eq!(empty.legs.len(), 3);
}

#[test]
fn a_label_reads_back_as_it_was_written() {
    let label = Label {
        group: "k3j9x2".into(),
        leg: 2,
        of: 3,
        break_even: Some((1, 25)),
        oco: false,
        time_stop: None,
    };
    let text = label.encode();
    assert_eq!(text, "wyck:k3j9x2:2/3:b1,25");
    assert!(text.len() < 100);
    assert_eq!(Label::decode(&text), Some(label));
    let oco = Label::decode("wyck:ab:1/1:oco").unwrap();
    assert!(oco.oco && oco.break_even.is_none());
    assert_eq!(Label::decode("wyck"), None);
    assert_eq!(Label::decode("other:ab:1/1"), None);
    assert_eq!(Label::decode("wyck:ab:x/1"), None);
    assert_eq!(new_group(1_700_000_000_000).len(), 8);
}

#[test]
fn a_time_stop_is_written_in_the_label_and_read_back() {
    let mut label = Label::decode("wyck:k3j9x2:1/2:b1,0:oco").unwrap();
    assert_eq!(label.time_stop, None);
    for (only, tail) in [
        (Only::Always, ":t90"),
        (Only::Winning, ":t90w"),
        (Only::Losing, ":t90l"),
    ] {
        label.time_stop = Some(TimeStop { minutes: 90, only });
        let text = label.encode();
        assert!(text.ends_with(tail), "{text}");
        assert!(text.len() < 100);
        assert_eq!(Label::decode(&text), Some(label.clone()));
    }
    // A time stop alone, on a single order.
    let single = Label {
        group: "abc".into(),
        leg: 1,
        of: 1,
        break_even: None,
        oco: false,
        time_stop: Some(TimeStop {
            minutes: 1_440,
            only: Only::Always,
        }),
    };
    assert_eq!(single.encode(), "wyck:abc:1/1:t1440");
    assert_eq!(Label::decode("wyck:abc:1/1:t1440"), Some(single));
    // What is not a time is not read as one.
    for bad in [
        "wyck:abc:1/1:t",
        "wyck:abc:1/1:t0",
        "wyck:abc:1/1:tx",
        "wyck:abc:1/1:t999999999",
    ] {
        assert_eq!(Label::decode(bad), None, "{bad}");
    }
}

#[test]
fn a_time_stop_is_due_once_the_time_has_passed_and_the_profit_agrees() {
    let minute = 60_000;
    let rule = |only| TimeStop { minutes: 60, only };
    let opened = 1_000_000;
    let at = |m: i64| opened + m * minute;
    assert!(!rule(Only::Always).due(opened, at(59), None));
    assert!(rule(Only::Always).due(opened, at(60), None));
    assert!(rule(Only::Always).due(opened, at(600), Some(-3.0)));
    // A rule on the profit waits for the profit to be known.
    assert!(!rule(Only::Winning).due(opened, at(90), None));
    assert!(rule(Only::Winning).due(opened, at(90), Some(0.5)));
    assert!(!rule(Only::Winning).due(opened, at(90), Some(-0.5)));
    assert!(!rule(Only::Winning).due(opened, at(90), Some(0.0)));
    assert!(rule(Only::Losing).due(opened, at(90), Some(-0.5)));
    assert!(!rule(Only::Losing).due(opened, at(90), Some(0.5)));
    // Nothing overflows.
    assert!(!rule(Only::Always).due(i64::MAX, 0, None));
}

#[test]
fn a_time_stop_reads_in_words() {
    let words = |minutes, only| TimeStop { minutes, only }.describe();
    assert_eq!(words(90, Only::Always), "after 90 min");
    assert_eq!(words(240, Only::Winning), "after 4 h if in profit");
    assert_eq!(words(2_880, Only::Losing), "after 2 d if in loss");
}

fn open(position: i64, leg: u8, buy: bool, entry: f64, sl: Option<f64>) -> Open {
    Open {
        position,
        label: Label {
            group: "g".into(),
            leg,
            of: 3,
            break_even: Some((1, 0)),
            oco: false,
            time_stop: None,
        },
        buy,
        entry,
        stop_loss: sl,
    }
}

#[test]
fn the_first_target_moves_the_other_stops_to_the_entry() {
    let closed = open(1, 1, true, 1.1000, Some(1.0950)).label;
    let legs = [
        open(2, 2, true, 1.1000, Some(1.0950)),
        open(3, 3, true, 1.1000, Some(1.0950)),
        open(9, 2, true, 1.2, None),
    ];
    // A leg of another plan is left alone.
    let mut other = legs[2].clone();
    other.label.group = "elsewhere".into();
    let moves = break_even_moves(
        &closed,
        12.0,
        &[legs[0].clone(), legs[1].clone(), other],
        0.0001,
    );
    assert_eq!(moves, vec![(2, 1.1000), (3, 1.1000)]);
    // A loss, or another leg closing, moves nothing.
    assert!(break_even_moves(&closed, -1.0, &legs, 0.0001).is_empty());
    let mut second = closed.clone();
    second.leg = 2;
    assert!(break_even_moves(&second, 5.0, &legs, 0.0001).is_empty());
}

#[test]
fn break_even_only_tightens_and_respects_the_offset_and_the_side() {
    let mut closed = open(1, 1, false, 1.1000, Some(1.1050)).label;
    closed.break_even = Some((1, 20));
    // A sell: the stop goes below the entry by the offset (2 pips), and only if it was above.
    let moves = break_even_moves(
        &closed,
        3.0,
        &[
            open(2, 2, false, 1.1000, Some(1.1050)),
            open(3, 3, false, 1.1000, Some(1.0990)),
            open(4, 3, false, 1.1000, None),
        ],
        0.0001,
    );
    assert_eq!(moves.len(), 2);
    assert_eq!(moves[0].0, 2);
    assert!((moves[0].1 - 1.0998).abs() < 1e-9);
    assert_eq!(moves[1].0, 4, "a leg with no stop gets one");
}

#[test]
fn a_filled_order_of_a_pair_cancels_its_sibling_only() {
    let label = |group: &str, oco: bool| Label {
        group: group.into(),
        leg: 1,
        of: 1,
        break_even: None,
        oco,
        time_stop: None,
    };
    let working = [
        (10, label("a", true), false),
        (11, label("a", true), false),
        (12, label("b", true), false),
        (13, label("a", false), false),
        (14, label("a", true), true),
    ];
    // A buy of the pair filled: the sells of the pair go, its own legs and other plans stay.
    assert_eq!(
        oco_siblings(&label("a", true), true, &working),
        vec![10, 11]
    );
    assert!(oco_siblings(&label("a", false), true, &working).is_empty());
}

fn past(position: i64, leg: u8, oco: bool, buy: bool, filled: bool) -> Past {
    Past {
        label: Label {
            group: "g".into(),
            leg,
            of: 3,
            break_even: Some((1, 0)),
            oco,
            time_stop: None,
        },
        buy,
        filled,
        position: Some(position),
        pip: 0.0001,
    }
}

#[test]
fn a_break_even_missed_offline_is_caught_up_once() {
    let open = [
        open(2, 2, true, 1.1000, Some(1.0950)),
        open(3, 3, true, 1.1000, Some(1.0950)),
    ];
    let history = [past(1, 1, false, true, true)];
    let profits = std::collections::HashMap::from([(1, 12.0)]);
    let moves = catch_up(&history, &profits, &open, &[]);
    assert_eq!(
        moves,
        vec![
            Catch::Stop {
                position: 2,
                stop: 1.1000
            },
            Catch::Stop {
                position: 3,
                stop: 1.1000
            },
        ]
    );
    // Once the stops are at the entry, a second pass finds nothing to do.
    let done = [open_at(2, 2, Some(1.1000)), open_at(3, 3, Some(1.1000))];
    assert!(catch_up(&history, &profits, &done, &[]).is_empty());
}

fn open_at(position: i64, leg: u8, sl: Option<f64>) -> Open {
    open(position, leg, true, 1.1000, sl)
}

#[test]
fn a_leg_that_lost_or_is_still_open_moves_nothing() {
    let open = [open_at(2, 2, Some(1.0950))];
    let history = [past(1, 1, false, true, true)];
    let loss = std::collections::HashMap::from([(1, -4.0)]);
    assert!(catch_up(&history, &loss, &open, &[]).is_empty());
    // No closing deal known for the position: nothing is assumed.
    assert!(catch_up(&history, &std::collections::HashMap::new(), &open, &[]).is_empty());
    // The leg is still open (only part of it closed): it is not a leg that closed.
    let still = [open_at(1, 1, Some(1.0950)), open_at(2, 2, Some(1.0950))];
    let profit = std::collections::HashMap::from([(1, 9.0)]);
    assert!(catch_up(&history, &profit, &still, &[]).is_empty());
    // An order that never filled proves nothing.
    let unfilled = [past(1, 1, false, true, false)];
    assert!(catch_up(&unfilled, &profit, &open, &[]).is_empty());
}

#[test]
fn an_oco_fill_missed_offline_cancels_the_other_side() {
    let label = |oco: bool| Label {
        group: "g".into(),
        leg: 1,
        of: 1,
        break_even: None,
        oco,
        time_stop: None,
    };
    let working = [(10, label(true), false), (11, label(true), true)];
    let mut filled = past(5, 1, true, true, true);
    filled.label = label(true);
    let moves = catch_up(
        &[filled.clone(), filled],
        &std::collections::HashMap::new(),
        &[],
        &working,
    );
    assert_eq!(
        moves,
        vec![Catch::Cancel { order: 10 }],
        "the same order is cancelled once"
    );
}
