use super::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn lots_become_volumes_the_broker_accepts() {
    let c = Contract::default();
    assert_eq!(c.volume_of_lots(1.0), 10_000_000);
    assert_eq!(c.volume_of_lots(0.013), 100_000, "rounded to the 0.01 step");
    assert_eq!(c.volume_of_lots(0.0), c.min_volume);
    assert_eq!(c.volume_of_lots(f64::NAN), c.min_volume);
    assert_eq!(c.volume_of_lots(1e9), c.max_volume);
    assert!(close(c.lots_of_volume(2_500_000), 0.25));
}

#[test]
fn pips_follow_the_pip_position() {
    let fx = Contract::default();
    assert!(close(fx.pip(), 0.0001));
    assert!((fx.pips(0.0025) - 25.0).abs() < 1e-6);
    let jpy = Contract {
        digits: 3,
        pip_position: 2,
        ..Contract::default()
    };
    assert!((jpy.pips(0.25) - 25.0).abs() < 1e-6);
    assert_eq!(jpy.format_price(151.2), "151.200");
    assert!(close(fx.round_price(1.084_123_7), 1.08412));
}

#[test]
fn a_price_makes_a_limit_or_a_stop_depending_on_the_side() {
    let (bid, ask) = (Some(1.1000), Some(1.1002));
    assert_eq!(pending_kind(true, 1.0990, bid, ask), Pending::Limit);
    assert_eq!(pending_kind(true, 1.1010, bid, ask), Pending::Stop);
    assert_eq!(pending_kind(false, 1.1010, bid, ask), Pending::Limit);
    assert_eq!(pending_kind(false, 1.0990, bid, ask), Pending::Stop);
    assert_eq!(pending_kind(true, 1.0, None, None), Pending::Limit);
}

#[test]
fn protection_must_sit_on_the_right_side() {
    assert!(check_protection(true, 1.1, Some(1.09), Some(1.12)).is_ok());
    assert_eq!(
        check_protection(true, 1.1, Some(1.11), None),
        Err(TicketProblem::StopLossWrongSide)
    );
    assert_eq!(
        check_protection(false, 1.1, None, Some(1.11)),
        Err(TicketProblem::TakeProfitWrongSide)
    );
    assert!(check_protection(false, 1.1, Some(1.11), Some(1.08)).is_ok());
    assert!(check_protection(true, 1.1, None, None).is_ok());
}

#[test]
fn a_distance_is_counted_in_the_servers_units() {
    assert_eq!(relative_distance(0.0025, 5), 250);
    assert_eq!(relative_distance(-0.0025, 5), 250);
    // A three digit symbol takes multiples of a hundred units (a thousandth).
    assert_eq!(relative_distance(0.1234, 3), 12_300);
    assert_eq!(relative_distance(0.12351, 3), 12_400);
    // A two digit symbol: multiples of a thousand units (a hundredth).
    assert_eq!(relative_distance(0.2549, 2), 25_000);
    // Never nothing, and more digits than the scale has change nothing.
    assert_eq!(relative_distance(0.00001, 2), 1_000);
    assert_eq!(relative_distance(0.00012, 7), 12);
}

#[test]
fn the_profit_follows_the_price_at_the_rate_it_is_converted_at() {
    // Long 100 000 units from 1.1000; the server said +20 USD gross, +18 net.
    let units = 100_000.0;
    let quote_then = quote_profit(true, 1.1000, 1.1002, units);
    assert!((quote_then - 20.0).abs() < 1e-6);
    let mark = PnlMark {
        gross: 20.0,
        net: 18.0,
        rate: Some(1.0),
    };
    let now = quote_profit(true, 1.1000, 1.1010, units);
    assert!((live_net(&mark, now) - 98.0).abs() < 1e-6);
    // Profit in another currency: gross 10 for a quote profit of 20 is a rate of 0.5.
    let rate = implied_rate(10.0, 20.0, 1.0);
    assert_eq!(rate, Some(0.5));
    let converted = PnlMark {
        gross: 10.0,
        net: 9.0,
        rate,
    };
    assert!((live_net(&converted, 100.0) - 49.0).abs() < 1e-6);
    // Too small a move, or one of the other sign, says nothing about the rate.
    assert_eq!(implied_rate(0.3, 0.5, 1.0), None);
    assert_eq!(implied_rate(-10.0, 20.0, 1.0), None);
    let unknown = PnlMark {
        gross: 0.0,
        net: -2.0,
        rate: None,
    };
    assert!(close(live_net(&unknown, 50.0), -2.0));
    assert!(close(quote_profit(false, 1.1, 1.09, 10.0), 0.1));
}

#[test]
fn the_totals_of_an_account_add_up() {
    let s = summary(10_000.0, 250.0, 500.0);
    assert!(close(s.equity, 10_250.0));
    assert!(close(s.free_margin, 9_750.0));
    assert!(close(s.margin_level.unwrap(), 2_050.0));
    assert_eq!(summary(1.0, 0.0, 0.0).margin_level, None);
}

#[test]
fn lots_are_written_plainly() {
    assert_eq!(format_lots(0.10), "0.1");
    assert_eq!(format_lots(2.0), "2");
}
#[test]
fn the_quantity_for_a_risk_and_what_it_loses_are_the_same_formula() {
    let (risk, distance, value) = (100.0, 0.0020, 50_000.0);
    let quantity = quantity_for_risk(risk, distance, value).unwrap();
    assert!((move_amount(quantity, distance, value) - risk).abs() < 1e-9);
    for bad in [
        (0.0, 1.0, 1.0),
        (1.0, 0.0, 1.0),
        (1.0, 1.0, 0.0),
        (1.0, f64::NAN, 1.0),
    ] {
        assert_eq!(quantity_for_risk(bad.0, bad.1, bad.2), None, "{bad:?}");
    }
}

#[test]
fn a_risk_makes_the_volume_that_loses_it_at_the_stop() {
    let c = Contract::default();
    // 100 USD over 20 pips of EURUSD: 10 USD a pip, so 0.5 lots.
    let lots = lots_for_risk(100.0, 0.0020, 1.0, &c).unwrap();
    assert!((lots - 0.5).abs() < 1e-9);
    assert_eq!(c.volume_at_most(lots).volume, 5_000_000);
    // Stepped down, never up: 0.567 lots is 0.56.
    assert_eq!(c.volume_at_most(0.567).volume, 5_600_000);
    assert_eq!(c.volume_at_most(0.3).volume, 3_000_000);
    assert_eq!(c.volume_at_most(0.001).limit, Some(Limit::Min));
    assert_eq!(c.volume_at_most(1e9).limit, Some(Limit::Max));
    assert_eq!(c.volume_near(0.567).volume, 5_700_000);
    assert_eq!(c.volume_near(0.567).limit, None);
    assert_eq!(c.volume_near(0.001).limit, Some(Limit::Min));
    assert_eq!(lots_for_risk(100.0, 0.0, 1.0, &c), None);
    assert_eq!(lots_for_risk(0.0, 0.002, 1.0, &c), None);
    // A quote currency worth half the deposit one needs twice the lots.
    let half = lots_for_risk(100.0, 0.0020, 0.5, &c).unwrap();
    assert!((half - 1.0).abs() < 1e-9);
}

#[test]
fn a_conversion_chain_is_walked_either_way() {
    // Assets: 1 EUR, 2 USD, 3 JPY, 4 GBP. Symbols: 10 EURUSD, 11 USDJPY, 12 GBPUSD.
    let prices = |id: i64| match id {
        10 => Some(1.10),
        11 => Some(150.0),
        12 => Some(1.25),
        _ => None,
    };
    let eurusd = Link {
        symbol_id: 10,
        base: 1,
        quote: 2,
    };
    let usdjpy = Link {
        symbol_id: 11,
        base: 2,
        quote: 3,
    };
    let gbpusd = Link {
        symbol_id: 12,
        base: 4,
        quote: 2,
    };
    assert_eq!(chain_rate(2, 2, &[], &prices), Some(1.0));
    assert!((chain_rate(1, 2, &[eurusd], &prices).unwrap() - 1.10).abs() < 1e-12);
    // One JPY in USD, then in EUR.
    let jpy_usd = chain_rate(3, 2, &[usdjpy], &prices).unwrap();
    assert!((jpy_usd - 1.0 / 150.0).abs() < 1e-12);
    let jpy_eur = chain_rate(3, 1, &[usdjpy, eurusd], &prices).unwrap();
    assert!((jpy_eur - 1.0 / 150.0 / 1.10).abs() < 1e-12);
    let gbp_eur = chain_rate(4, 1, &[gbpusd, eurusd], &prices).unwrap();
    assert!((gbp_eur - 1.25 / 1.10).abs() < 1e-12);
    // A missing price, or a chain that leads elsewhere, gives no rate.
    let unpriced = Link {
        symbol_id: 99,
        base: 3,
        quote: 2,
    };
    assert_eq!(chain_rate(3, 2, &[unpriced], &prices), None);
    assert_eq!(chain_rate(3, 1, &[usdjpy], &prices), None);
    assert_eq!(mid(Some(1.0), Some(1.2)), Some(1.1));
    assert_eq!(mid(None, Some(1.2)), Some(1.2));
}
