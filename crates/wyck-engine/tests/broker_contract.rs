//! One behavioral contract, run against every `Broker` implementation that can run without
//! a real server: the in-memory `MockBroker`, and the real `RemoteBroker` talking to a
//! scripted in-process Remote MCP server.

mod support;

use wyck_engine::broker::{
    Broker, ConnectRequest, MarketOrder, MockBroker, RemoteBroker, ServiceKind,
};
use wyck_engine::config::AssumedSpecs;
use wyck_engine::domain::{Side, Volume};
use wyck_engine::{EngineError, ErrorKind};

fn order(label: &str) -> MarketOrder {
    MarketOrder {
        symbol: "EURUSD".to_owned(),
        side: Side::Buy,
        volume: Volume::from_units(10_000),
        stop_loss_distance: Some(0.0030),
        take_profit_distance: Some(0.0060),
        label: label.to_owned(),
        slippage_points: None,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The contract. Every check here must hold for every broker.
async fn contract(broker: &dyn Broker) {
    // Account
    let account = broker.account().await.unwrap();
    assert_eq!(account.balance, Some(10_000.0));
    assert_eq!(account.currency.as_deref(), Some("USD"));
    assert_eq!(&account.account_id, broker.account_id());

    // Instruments
    let symbols = broker.symbols().await.unwrap();
    assert!(symbols.iter().any(|s| s == "EURUSD"));
    let eurusd = broker.instrument("eurusd").await.unwrap();
    assert_eq!(eurusd.symbol, "EURUSD");
    assert_eq!(eurusd.price_digits, 5);
    assert!(close(eurusd.pip_size, 0.0001));
    assert!(eurusd.volume.min.is_positive() && eurusd.volume.step.is_positive());
    let missing = broker.instrument("NOPE").await.unwrap_err();
    assert_eq!(missing.kind(), ErrorKind::Validation);

    // Quotes: known symbols come back, unknown ones are simply absent, empty is empty.
    let quotes = broker
        .quotes(&["EURUSD".to_owned(), "NOPE".to_owned()])
        .await
        .unwrap();
    assert_eq!(
        quotes.len(),
        1,
        "an unknown symbol must not blank the batch"
    );
    let q = &quotes[0];
    assert_eq!(q.symbol, "EURUSD");
    assert!(q.is_valid() && q.ask > q.bid);
    assert!(broker.quotes(&[]).await.unwrap().is_empty());

    // Liveness
    broker.ping().await.unwrap();

    // Flat to start
    assert!(broker.positions().await.unwrap().is_empty());

    // Open with protection
    let placed = broker
        .place_market(&order("wyck-contract-1"))
        .await
        .unwrap();
    let positions = broker.positions().await.unwrap();
    assert_eq!(positions.len(), 1);
    let p = positions[0].clone();
    if let Some(id) = placed.position_id {
        assert_eq!(p.id, id);
    }
    assert_eq!(p.side, Side::Buy);
    assert_eq!(p.volume, Volume::from_units(10_000));
    assert_eq!(p.label.as_deref(), Some("wyck-contract-1"));
    let entry = p.entry_price.unwrap();
    assert!(
        close(p.stop_loss.unwrap(), entry - 0.0030),
        "sl {:?} entry {entry}",
        p.stop_loss
    );
    assert!(close(p.take_profit.unwrap(), entry + 0.0060));

    // Changing one leg must leave the other alone (Q-R10 on Remote).
    let new_sl = entry - 0.0020;
    broker.set_protection(&p, Some(new_sl), None).await.unwrap();
    let p = broker.positions().await.unwrap().remove(0);
    assert!(close(p.stop_loss.unwrap(), new_sl));
    assert!(
        close(p.take_profit.unwrap(), entry + 0.0060),
        "take profit was lost"
    );

    // Partial then full close.
    broker
        .close_position(&p, Some(Volume::from_units(4_000)))
        .await
        .unwrap();
    let p = broker.positions().await.unwrap().remove(0);
    assert_eq!(p.volume, Volume::from_units(6_000));
    broker.close_position(&p, None).await.unwrap();
    assert!(broker.positions().await.unwrap().is_empty());

    broker.close().await.unwrap();
    broker.close().await.unwrap();
}

#[tokio::test]
async fn mock_broker_meets_the_contract() {
    contract(&MockBroker::new()).await;
}

async fn connect_remote(trading: bool) -> (RemoteBroker, support::RemoteScenario) {
    let scenario = support::remote_scenario(trading).await;
    let request = ConnectRequest::new(ServiceKind::CtraderRemote, scenario.url.clone(), None);
    let broker = RemoteBroker::connect(&request, &AssumedSpecs::default())
        .await
        .expect("connect and bootstrap");
    (broker, scenario)
}

#[tokio::test]
async fn remote_broker_meets_the_contract() {
    let (broker, scenario) = connect_remote(true).await;
    assert_eq!(broker.service(), ServiceKind::CtraderRemote);
    assert!(broker.can_trade());
    contract(&broker).await;

    // Wire-level facts the contract cannot see.
    let mutations = scenario.mutations();
    let create = mutations.iter().find(|(t, _)| t == "create_order").unwrap();
    assert_eq!(create.1["orderType"], "MARKET");
    assert_eq!(
        create.1["volume"], 1_000_000,
        "10,000 units are 1,000,000 cents"
    );
    assert_eq!(
        create.1["relativeStopLoss"], 300,
        "0.0030 at 5 digits is 300 points"
    );
    assert_eq!(create.1["relativeTakeProfit"], 600);
    assert!(
        create.1.get("stopLoss").is_none(),
        "Q-R4: no absolute SL on a MARKET order"
    );
    let amend = mutations
        .iter()
        .find(|(t, _)| t == "amend_position")
        .unwrap();
    assert!(
        amend.1["stopLoss"].is_f64() && amend.1["takeProfit"].is_f64(),
        "Q-R10: both legs are always sent, as display prices"
    );
}

#[tokio::test]
async fn remote_instruments_are_assumed_and_learn_their_precision_from_quotes() {
    let (broker, scenario) = connect_remote(true).await;
    let usdjpy = broker.instrument("USDJPY").await.unwrap();
    assert_eq!(
        usdjpy.specs_source,
        wyck_engine::domain::SpecsSource::Assumed
    );
    // The server sends no pipDigits. The quote 15012300 (units of 1e-5) shows 3 decimals.
    assert_eq!(usdjpy.price_digits, 3);
    assert!(close(usdjpy.pip_size, 0.01));
    assert_eq!(usdjpy.base_currency.as_deref(), Some("USD"));
    assert_eq!(usdjpy.quote_currency.as_deref(), Some("JPY"));
    assert_eq!(scenario.world.lock().unwrap().last_price_ids, vec![2]);

    let quotes = broker.quotes(&["USDJPY".to_owned()]).await.unwrap();
    assert!(close(quotes[0].bid, 150.123), "raw prices are 1e-5 units");
    assert!(close(quotes[0].ask, 150.125));
}

#[tokio::test]
async fn remote_has_no_server_clock() {
    let (broker, _scenario) = connect_remote(true).await;
    let error = broker.server_time().await.unwrap_err();
    assert!(matches!(error, EngineError::Broker { .. }), "{error:?}");
}

#[tokio::test]
async fn remote_only_asks_for_prices_of_symbols_it_knows() {
    let (broker, scenario) = connect_remote(true).await;
    let quotes = broker.quotes(&["NOPE".to_owned()]).await.unwrap();
    assert!(quotes.is_empty());
    assert!(
        scenario.world.lock().unwrap().last_price_ids.is_empty(),
        "no request at all for an unknown symbol (Q-R8 would blank a whole batch)"
    );
}

#[tokio::test]
async fn a_read_only_remote_session_refuses_every_mutation_without_calling_the_server() {
    let (broker, scenario) = connect_remote(false).await;
    assert!(!broker.can_trade());
    let refused = broker.place_market(&order("x")).await.unwrap_err();
    assert!(
        matches!(refused, EngineError::TradingUnavailable(_)),
        "{refused:?}"
    );
    assert_eq!(scenario.mutation_count(), 0);
}
