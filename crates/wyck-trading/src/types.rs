//! What an account holds: balance, positions, orders, deals.
//!
//! These are the read-only account messages of the Open API (`ProtoOATrader`, `ProtoOAPosition`,
//! `ProtoOAOrder`, `ProtoOADeal`, ...). Reading them needs no trading permission: a token of the
//! `accounts` scope is enough.
//!
//! # Units
//!
//! - **Money** (balance, swap, commission, used margin) is an integer scaled by `10^moneyDigits`,
//!   where `moneyDigits` comes with the message (2 when absent). [`money`] converts.
//! - **Volume** is an integer in hundredths of a unit ([`volume_units`] converts).
//! - **Prices of positions and orders** are ordinary decimals (`double` in the `.proto`), unlike
//!   the integer prices of ticks and bars.
//! - **Times** are Unix milliseconds.
//!
//! The enumerations (a side, a status, a type) arrive as numbers; [`crate::enums`] decodes them.

use serde::Deserialize;

use crate::enums::{
    AccessRights, AccountType, DealStatus, OrderStatus, OrderType, PositionStatus, TradeSide,
};
use wyck_core::flex;
use wyck_core::math::i64_to_f64;

/// An integer amount of money as a real number, given the account's `money_digits` (2 when the
/// server did not say).
///
/// ```
/// use wyck_trading::money;
///
/// assert_eq!(money(1_000_050, Some(2)), 10_000.5);
/// assert_eq!(money(500, None), 5.0); // two digits when the server did not say
/// ```
#[must_use]
pub fn money(raw: i64, money_digits: Option<u32>) -> f64 {
    i64_to_f64(raw) / 10f64.powi(i32::try_from(money_digits.unwrap_or(2)).unwrap_or(2))
}

/// The decimals of a money amount from the number the server sent; `None` when it sent none.
fn digits_of(raw: Option<i64>) -> Option<u32> {
    raw.and_then(|digits| u32::try_from(digits).ok())
}

/// A volume in units, from the server's hundredths of a unit.
#[must_use]
pub fn volume_units(raw: i64) -> f64 {
    i64_to_f64(raw) / 100.0
}

/// The account itself (`ProtoOATrader`): balance, leverage, rights.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trader {
    #[serde(deserialize_with = "flex::int")]
    pub ctid_trader_account_id: i64,
    /// The balance, scaled by `10^moneyDigits` (see [`money`]).
    #[serde(deserialize_with = "flex::int")]
    pub balance: i64,
    /// The asset the account is held in: see the asset list.
    #[serde(default, deserialize_with = "flex::opt")]
    pub deposit_asset_id: Option<i64>,
    /// What the account may do, as its number (see [`Trader::rights`]).
    #[serde(default, deserialize_with = "flex::opt")]
    pub access_rights: Option<i64>,
    /// Whether the account pays no swap.
    #[serde(default)]
    pub swap_free: Option<bool>,
    /// The leverage in hundredths (10000 is 1:100).
    #[serde(default, deserialize_with = "flex::opt")]
    pub leverage_in_cents: Option<i64>,
    /// The highest leverage the account may use.
    #[serde(default, deserialize_with = "flex::opt")]
    pub max_leverage: Option<i64>,
    /// The login number shown in the platform. For display only.
    #[serde(default, deserialize_with = "flex::opt")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `traderLogin` field of the wire message"
    )]
    pub trader_login: Option<i64>,
    /// How the account books positions, as its number (see [`Trader::kind`]).
    #[serde(default, deserialize_with = "flex::opt")]
    pub account_type: Option<i64>,
    #[serde(default)]
    pub broker_name: Option<String>,
    /// When the account was registered, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub registration_timestamp: Option<i64>,
    /// Decimals of every money amount of this account.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
}

impl Trader {
    /// The balance as a real number.
    #[must_use]
    pub fn balance_amount(&self) -> f64 {
        money(self.balance, self.digits())
    }

    /// The decimals of money amounts (2 when unknown).
    #[must_use]
    pub fn digits(&self) -> Option<u32> {
        digits_of(self.money_digits)
    }

    /// The leverage as a ratio (100.0 for 1:100).
    #[must_use]
    pub fn leverage(&self) -> Option<f64> {
        self.leverage_in_cents.map(|c| i64_to_f64(c) / 100.0)
    }

    /// What the account may do.
    #[must_use]
    pub fn rights(&self) -> Option<AccessRights> {
        self.access_rights.and_then(AccessRights::from_number)
    }

    /// How the account books positions.
    #[must_use]
    pub fn kind(&self) -> Option<AccountType> {
        self.account_type.and_then(AccountType::from_number)
    }
}

/// What an order or a position is about (`ProtoOATradeData`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeData {
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// The volume in hundredths of a unit (see [`volume_units`]).
    #[serde(deserialize_with = "flex::int")]
    pub volume: i64,
    /// Buy or sell, as its number (see [`TradeData::side`]).
    #[serde(deserialize_with = "flex::int")]
    pub trade_side: i64,
    /// When it opened, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub open_timestamp: Option<i64>,
    /// The label the order was sent with.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
    /// When it closed, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub close_timestamp: Option<i64>,
}

impl TradeData {
    /// Buy or sell.
    #[must_use]
    pub fn side(&self) -> Option<TradeSide> {
        TradeSide::from_number(self.trade_side)
    }

    /// Whether it is a buy. The account book only keeps what has a side it can read, so for a
    /// position or an order taken from it this is the whole answer.
    #[must_use]
    pub fn is_buy(&self) -> bool {
        self.side() == Some(TradeSide::Buy)
    }

    /// The volume in units.
    #[must_use]
    pub fn units(&self) -> f64 {
        volume_units(self.volume)
    }
}

/// An open or closed position (`ProtoOAPosition`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    #[serde(deserialize_with = "flex::int")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `positionId` field of the wire message"
    )]
    pub position_id: i64,
    /// Symbol, volume, side, times.
    pub trade_data: TradeData,
    /// Open, closed, ..., as its number (see [`Position::status`]).
    #[serde(deserialize_with = "flex::int")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `positionStatus` field of the wire message"
    )]
    pub position_status: i64,
    /// The swap charged so far, scaled by `10^moneyDigits`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub swap: Option<i64>,
    /// The entry price.
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub stop_loss: Option<f64>,
    #[serde(default)]
    pub take_profit: Option<f64>,
    /// The last update, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub utc_last_update_timestamp: Option<i64>,
    /// The commission, scaled by `10^moneyDigits`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub commission: Option<i64>,
    /// The margin the position uses, scaled by `10^moneyDigits`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub used_margin: Option<i64>,
    /// Decimals of the money amounts of this position.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
    /// Whether the stop loss trails the price.
    #[serde(default)]
    pub trailing_stop_loss: Option<bool>,
}

impl Position {
    /// Where the position stands.
    #[must_use]
    pub fn status(&self) -> Option<PositionStatus> {
        PositionStatus::from_number(self.position_status)
    }
}

/// An order (`ProtoOAOrder`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    #[serde(deserialize_with = "flex::int")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `orderId` field of the wire message"
    )]
    pub order_id: i64,
    /// Symbol, volume, side, times.
    pub trade_data: TradeData,
    /// The kind, as its number (see [`Order::kind`]).
    #[serde(deserialize_with = "flex::int")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `orderType` field of the wire message"
    )]
    pub order_type: i64,
    /// Where it stands, as its number (see [`Order::status`]).
    #[serde(deserialize_with = "flex::int")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `orderStatus` field of the wire message"
    )]
    pub order_status: i64,
    /// When it expires, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub expiration_timestamp: Option<i64>,
    #[serde(default)]
    pub execution_price: Option<f64>,
    /// The volume executed, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub executed_volume: Option<i64>,
    /// The limit price, for limit orders.
    #[serde(default)]
    pub limit_price: Option<f64>,
    #[serde(default)]
    pub stop_price: Option<f64>,
    #[serde(default)]
    pub stop_loss: Option<f64>,
    #[serde(default)]
    pub take_profit: Option<f64>,
    /// The client's own id for the order.
    #[serde(default)]
    pub client_order_id: Option<String>,
    /// The position the order belongs to.
    #[serde(default, deserialize_with = "flex::opt")]
    pub position_id: Option<i64>,
    /// The last update, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub utc_last_update_timestamp: Option<i64>,
}

impl Order {
    #[must_use]
    pub fn kind(&self) -> Option<OrderType> {
        OrderType::from_number(self.order_type)
    }

    /// Where the order stands.
    #[must_use]
    pub fn status(&self) -> Option<OrderStatus> {
        OrderStatus::from_number(self.order_status)
    }
}

/// An execution (`ProtoOADeal`): one fill of an order.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    #[serde(deserialize_with = "flex::int")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `dealId` field of the wire message"
    )]
    pub deal_id: i64,
    /// The order it fills.
    #[serde(deserialize_with = "flex::int")]
    pub order_id: i64,
    /// The position it belongs to.
    #[serde(deserialize_with = "flex::int")]
    pub position_id: i64,
    /// The volume asked, in hundredths of a unit.
    #[serde(deserialize_with = "flex::int")]
    pub volume: i64,
    /// The volume filled, in hundredths of a unit.
    #[serde(deserialize_with = "flex::int")]
    pub filled_volume: i64,
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// When the deal was created, in Unix milliseconds.
    #[serde(deserialize_with = "flex::int")]
    pub create_timestamp: i64,
    /// When it was executed, in Unix milliseconds.
    #[serde(deserialize_with = "flex::int")]
    pub execution_timestamp: i64,
    #[serde(default)]
    pub execution_price: Option<f64>,
    /// Buy or sell, as its number (see [`Deal::side`]).
    #[serde(deserialize_with = "flex::int")]
    pub trade_side: i64,
    /// How it ended, as its number (see [`Deal::status`]).
    #[serde(deserialize_with = "flex::int")]
    #[expect(
        clippy::struct_field_names,
        reason = "mirrors the `dealStatus` field of the wire message"
    )]
    pub deal_status: i64,
    /// The commission, scaled by `10^moneyDigits`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub commission: Option<i64>,
    /// Decimals of the money amounts of this deal.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
    /// The details of a closing deal (profit, swap, ...), kept as the server sent them.
    #[serde(default)]
    pub close_position_detail: Option<serde_json::Value>,
}

impl Deal {
    /// Buy or sell.
    #[must_use]
    pub fn side(&self) -> Option<TradeSide> {
        TradeSide::from_number(self.trade_side)
    }

    /// Whether it is a buy; see [`TradeData::is_buy`].
    #[must_use]
    pub fn is_buy(&self) -> bool {
        self.side() == Some(TradeSide::Buy)
    }

    #[must_use]
    pub fn status(&self) -> Option<DealStatus> {
        DealStatus::from_number(self.deal_status)
    }

    /// What a closing deal made or lost after swap and commission, in the account's money. `None`
    /// for a deal that opened a position (it has no closing details).
    #[must_use]
    pub fn realized_pnl(&self) -> Option<f64> {
        let detail = self.close_position_detail.as_ref()?;
        let number = |key: &str| {
            let value = detail.get(key)?;
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        };
        let digits = digits_of(number("moneyDigits").or(self.money_digits));
        let gross = number("grossProfit")?;
        let swap = number("swap").unwrap_or(0);
        let commission = number("commission").unwrap_or(0);
        Some(money(gross + swap + commission, digits))
    }
}

/// The unrealized profit or loss of one position (`ProtoOAPositionUnrealizedPnL`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionUnrealizedPnL {
    #[serde(deserialize_with = "flex::int")]
    pub position_id: i64,
    /// Gross unrealized profit or loss, scaled by `10^moneyDigits`. Renamed explicitly: the
    /// server's field is `grossUnrealizedPnL` (capital `L`).
    #[serde(deserialize_with = "flex::int", rename = "grossUnrealizedPnL")]
    pub gross_unrealized_pnl: i64,
    /// Net unrealized profit or loss (closing commission not included), scaled by `10^moneyDigits`.
    /// Renamed explicitly: the server's field is `netUnrealizedPnL` (capital `L`).
    #[serde(deserialize_with = "flex::int", rename = "netUnrealizedPnL")]
    pub net_unrealized_pnl: i64,
}

#[cfg(test)]
mod tests {
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
            "balance": 1_000_000,
            "depositAssetId": 15,
            "accessRights": 0,
            "leverageInCents": 10000,
            "accountType": 0,
            "brokerName": "Spotware",
            "moneyDigits": 2,
            "traderLogin": 900_001
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
            "tradeData": {"symbolId": 1, "volume": 100_000, "tradeSide": 1, "openTimestamp": 1_789_700_000_000_i64, "label": "wyck"},
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
    fn unrealized_pnl_is_read() {
        let pnl: PositionUnrealizedPnL = serde_json::from_value(json!({
            "positionId": 1, "grossUnrealizedPnL": 500, "netUnrealizedPnL": 450
        }))
        .unwrap();
        assert_eq!(pnl.net_unrealized_pnl, 450);
    }
}
