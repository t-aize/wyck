use super::*;
use serde_json::json;

#[test]
fn a_closing_deal_reports_its_profit_after_costs() {
    let mut deal: Deal = serde_json::from_value(json!({
        "dealId": 1, "orderId": 2, "positionId": 3, "volume": 100, "filledVolume": 100,
        "symbolId": 4, "createTimestamp": 0, "executionTimestamp": 0,
        "tradeSide": 1, "dealStatus": 2, "moneyDigits": 2
    }))
    .unwrap();
    assert_eq!(deal.realized_pnl(), None, "an opening deal has no profit");
    deal.close_position_detail = Some(json!({
        "grossProfit": -1500, "swap": -50, "commission": "-100", "moneyDigits": 2
    }));
    assert_eq!(deal.realized_pnl(), Some(-16.5));
}

#[test]
fn enumerations_map_numbers_both_ways_and_refuse_unknown_ones() {
    assert_eq!(TradeSide::from_number(1), Some(TradeSide::Buy));
    assert_eq!(TradeSide::Sell.number(), 2);
    assert_eq!(TradeSide::from_number(0), None);
    assert_eq!(OrderType::from_number(6), Some(OrderType::StopLimit));
    assert_eq!(OrderType::from_number(7), None);
    assert_eq!(OrderStatus::from_number(5), Some(OrderStatus::Cancelled));
    assert_eq!(PositionStatus::from_number(1), Some(PositionStatus::Open));
    assert_eq!(DealStatus::from_number(2), Some(DealStatus::Filled));
    assert_eq!(DealStatus::from_number(1), None, "the enum has no 1");
    assert_eq!(AccountType::from_number(0), Some(AccountType::Hedged));
    assert_eq!(AccessRights::from_number(3), Some(AccessRights::NoLogin));
    assert_eq!(AccessRights::CloseOnly.label(), "close only");
    assert_eq!(TimeInForce::from_number(4), Some(TimeInForce::FillOrKill));
    assert_eq!(
        OrderTriggerMethod::from_number(3),
        Some(OrderTriggerMethod::DoubleTrade)
    );
    assert_eq!(OrderTriggerMethod::from_number(0), None);
}

#[test]
fn every_enumeration_value_round_trips() {
    for side in [TradeSide::Buy, TradeSide::Sell] {
        assert_eq!(TradeSide::from_number(i64::from(side.number())), Some(side));
    }
    for kind in [
        OrderType::Market,
        OrderType::Limit,
        OrderType::Stop,
        OrderType::StopLossTakeProfit,
        OrderType::MarketRange,
        OrderType::StopLimit,
    ] {
        assert_eq!(OrderType::from_number(i64::from(kind.number())), Some(kind));
    }
}

#[test]
fn money_and_volume_conversions() {
    assert_eq!(money(1_000_050, Some(2)), 10_000.5);
    assert_eq!(money(12_345, Some(4)), 1.2345);
    assert_eq!(
        money(500, None),
        5.0,
        "two digits when the server does not say"
    );
    assert_eq!(volume_units(100_000), 1_000.0);
    assert_eq!(volume_units(1), 0.01);
}

#[test]
fn a_trader_is_read_with_its_helpers() {
    let trader: Trader = serde_json::from_value(json!({
        "ctidTraderAccountId": "48332955",
        "balance": 1000000,
        "depositAssetId": 15,
        "accessRights": 0,
        "leverageInCents": 10000,
        "accountType": 0,
        "brokerName": "Spotware",
        "moneyDigits": 2,
        "traderLogin": 900001
    }))
    .unwrap();
    assert_eq!(trader.ctid_trader_account_id, 48_332_955);
    assert_eq!(trader.balance_amount(), 10_000.0);
    assert_eq!(trader.leverage(), Some(100.0));
    assert_eq!(trader.rights(), Some(AccessRights::FullAccess));
    assert_eq!(trader.kind(), Some(AccountType::Hedged));
}

#[test]
fn a_position_and_its_trade_data_are_read() {
    let position: Position = serde_json::from_value(json!({
        "positionId": 77,
        "tradeData": {"symbolId": 1, "volume": 100000, "tradeSide": 1, "openTimestamp": 1789700000000_i64, "label": "wyck"},
        "positionStatus": 1,
        "swap": -25,
        "price": 1.14879,
        "stopLoss": 1.1400,
        "usedMargin": 5000,
        "moneyDigits": 2
    }))
    .unwrap();
    assert_eq!(position.status(), Some(PositionStatus::Open));
    assert_eq!(position.trade_data.side(), Some(TradeSide::Buy));
    assert_eq!(position.trade_data.units(), 1_000.0);
    assert_eq!(position.price, Some(1.14879));
    assert_eq!(position.take_profit, None);
    assert_eq!(position.trade_data.label.as_deref(), Some("wyck"));
}

#[test]
fn an_order_and_a_deal_are_read() {
    let order: Order = serde_json::from_value(json!({
        "orderId": 5, "tradeData": {"symbolId": 2, "volume": 50, "tradeSide": 2},
        "orderType": 2, "orderStatus": 1, "limitPrice": 2650.5, "clientOrderId": "c-1"
    }))
    .unwrap();
    assert_eq!(order.kind(), Some(OrderType::Limit));
    assert_eq!(order.status(), Some(OrderStatus::Accepted));
    assert_eq!(order.limit_price, Some(2650.5));

    let deal: Deal = serde_json::from_value(json!({
        "dealId": 9, "orderId": 5, "positionId": 77, "volume": 50, "filledVolume": 50,
        "symbolId": 2, "createTimestamp": 1, "executionTimestamp": 2,
        "tradeSide": 2, "dealStatus": 2, "executionPrice": 2650.5,
        "closePositionDetail": {"entryPrice": 2640.0, "profit": 1050}
    }))
    .unwrap();
    assert_eq!(deal.status(), Some(DealStatus::Filled));
    assert_eq!(deal.side(), Some(TradeSide::Sell));
    assert!(deal.close_position_detail.is_some());
}

#[test]
fn unknown_enumeration_values_do_not_break_reading() {
    let order: Order = serde_json::from_value(json!({
        "orderId": 5, "tradeData": {"symbolId": 2, "volume": 50, "tradeSide": 9},
        "orderType": 99, "orderStatus": 42
    }))
    .unwrap();
    assert_eq!(order.kind(), None);
    assert_eq!(order.status(), None);
    assert_eq!(order.trade_data.side(), None);
}

#[test]
fn a_deposit_withdraw_converts_its_amount() {
    let entry: DepositWithdraw = serde_json::from_value(json!({
        "operationType": 0, "balanceHistoryId": 1, "balance": 110_000, "delta": 10_000,
        "changeBalanceTimestamp": 5, "moneyDigits": 2
    }))
    .unwrap();
    assert_eq!(entry.amount(), 100.0);
    assert_eq!(entry.operation_type, 0);
}

#[test]
fn deal_offsets_and_unrealized_pnl_are_read() {
    let pnl: PositionUnrealizedPnL = serde_json::from_value(json!({
        "positionId": 1, "grossUnrealizedPnL": 500, "netUnrealizedPnL": 450
    }))
    .unwrap();
    assert_eq!(pnl.net_unrealized_pnl, 450);

    let offset: DealOffset =
        serde_json::from_value(json!({"dealId": 1, "volume": 100, "executionPrice": 1.1})).unwrap();
    assert_eq!(offset.deal_id, 1);
}
