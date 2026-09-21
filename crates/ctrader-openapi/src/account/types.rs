//! What an account holds: balance, positions, orders, deals, and their enumerations.
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
//! # Enumerations
//!
//! Enumerations arrive as numbers. Each one here has a `from_number` that gives `None` for a
//! number it does not know, so a server that adds a value later shows up as a missing label and not
//! as a crash. The structs keep the raw number next to a method that decodes it.

use serde::Deserialize;

use crate::transport::wire::flex;

/// Defines an enumeration that maps to server numbers and back. Exported at the crate root (as
/// `crate::number_enum`) so [`crate::trading`] and [`crate::margin`] can build their own
/// enumerations with it; it is not meant to be used outside this crate.
#[doc(hidden)]
#[macro_export]
macro_rules! number_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$vmeta:meta])* $variant:ident = $number:literal => $label:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant = $number ),+
        }

        impl $name {
            /// The value with the server number `number`, or `None` for a number not known here.
            #[must_use]
            pub fn from_number(number: i64) -> Option<Self> {
                match number {
                    $( $number => Some(Self::$variant), )+
                    _ => None,
                }
            }

            /// The server number.
            #[must_use]
            pub fn number(self) -> i32 {
                self as i32
            }

            /// A short human readable name.
            #[must_use]
            pub fn label(self) -> &'static str {
                match self {
                    $( Self::$variant => $label, )+
                }
            }
        }
    };
}

number_enum! {
    /// Buy or sell.
    TradeSide {
        /// A purchase.
        Buy = 1 => "buy",
        /// A sale.
        Sell = 2 => "sell",
    }
}

number_enum! {
    /// The kind of an order.
    OrderType {
        /// At the market price.
        Market = 1 => "market",
        /// At a limit price or better.
        Limit = 2 => "limit",
        /// When a stop price is reached.
        Stop = 3 => "stop",
        /// The protective order of a position.
        StopLossTakeProfit = 4 => "stop loss / take profit",
        /// At the market within a price range.
        MarketRange = 5 => "market range",
        /// A stop that becomes a limit order.
        StopLimit = 6 => "stop limit",
    }
}

number_enum! {
    /// Where an order stands.
    OrderStatus {
        /// Accepted and working.
        Accepted = 1 => "accepted",
        /// Fully executed.
        Filled = 2 => "filled",
        /// Refused.
        Rejected = 3 => "rejected",
        /// Ran out of time.
        Expired = 4 => "expired",
        /// Cancelled.
        Cancelled = 5 => "cancelled",
    }
}

number_enum! {
    /// Where a position stands.
    PositionStatus {
        /// Open.
        Open = 1 => "open",
        /// Closed.
        Closed = 2 => "closed",
        /// Being created.
        Created = 3 => "created",
        /// In error.
        Error = 4 => "error",
    }
}

number_enum! {
    /// How a deal ended.
    DealStatus {
        /// Fully filled.
        Filled = 2 => "filled",
        /// Partly filled.
        PartiallyFilled = 3 => "partially filled",
        /// Refused by the server.
        Rejected = 4 => "rejected",
        /// Refused inside the platform.
        InternallyRejected = 5 => "internally rejected",
        /// Failed.
        Error = 6 => "error",
        /// Missed.
        Missed = 7 => "missed",
    }
}

number_enum! {
    /// How the account books positions.
    AccountType {
        /// Several positions per symbol, each on its own.
        Hedged = 0 => "hedged",
        /// One net position per symbol.
        Netted = 1 => "netted",
        /// Spread betting.
        SpreadBetting = 2 => "spread betting",
    }
}

number_enum! {
    /// How long an order stays working (`ProtoOATimeInForce`).
    TimeInForce {
        /// Cancelled at `expiration_timestamp` if not filled.
        GoodTillDate = 1 => "good till date",
        /// Stays until it is filled or cancelled.
        GoodTillCancel = 2 => "good till cancel",
        /// Filled at once, in full or in part; the rest is cancelled.
        ImmediateOrCancel = 3 => "immediate or cancel",
        /// Filled in full at once, or not at all.
        FillOrKill = 4 => "fill or kill",
        /// Held until the market next opens.
        MarketOnOpen = 5 => "market on open",
    }
}

number_enum! {
    /// What triggers a stop order or a stop loss (`ProtoOAOrderTriggerMethod`).
    OrderTriggerMethod {
        /// A buy triggers on the ask, a sell on the bid (a stop loss the other way round).
        Trade = 1 => "trade",
        /// The opposite side of [`Self::Trade`].
        Opposite = 2 => "opposite",
        /// Like [`Self::Trade`], but only after a second consecutive tick confirms it.
        DoubleTrade = 3 => "double trade",
        /// Like [`Self::Opposite`], but only after a second consecutive tick confirms it.
        DoubleOpposite = 4 => "double opposite",
    }
}

number_enum! {
    /// What the account may do.
    AccessRights {
        /// Everything.
        FullAccess = 0 => "full access",
        /// Positions may only be closed.
        CloseOnly = 1 => "close only",
        /// No trading.
        NoTrading = 2 => "no trading",
        /// No login.
        NoLogin = 3 => "no login",
    }
}

/// ```
/// use ctrader_openapi::account::money;
///
/// assert_eq!(money(1_000_050, Some(2)), 10_000.5);
/// assert_eq!(money(500, None), 5.0); // two digits when the server did not say
/// ```
///
/// An integer amount of money as a real number, given the account's `money_digits` (2 when the
/// server did not say).
#[must_use]
pub fn money(raw: i64, money_digits: Option<u32>) -> f64 {
    raw as f64 / 10f64.powi(i32::try_from(money_digits.unwrap_or(2)).unwrap_or(2))
}

/// A volume in units, from the server's hundredths of a unit.
#[must_use]
pub fn volume_units(raw: i64) -> f64 {
    raw as f64 / 100.0
}

/// The account itself (`ProtoOATrader`): balance, leverage, rights.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trader {
    /// The trading account id.
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
    pub trader_login: Option<i64>,
    /// How the account books positions, as its number (see [`Trader::kind`]).
    #[serde(default, deserialize_with = "flex::opt")]
    pub account_type: Option<i64>,
    /// The broker's name.
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
        self.money_digits.and_then(|d| u32::try_from(d).ok())
    }

    /// The leverage as a ratio (100.0 for 1:100).
    #[must_use]
    pub fn leverage(&self) -> Option<f64> {
        self.leverage_in_cents.map(|c| c as f64 / 100.0)
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
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeData {
    /// The symbol.
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
    /// A comment.
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
    /// The position id.
    #[serde(deserialize_with = "flex::int")]
    pub position_id: i64,
    /// Symbol, volume, side, times.
    pub trade_data: TradeData,
    /// Open, closed, ..., as its number (see [`Position::status`]).
    #[serde(deserialize_with = "flex::int")]
    pub position_status: i64,
    /// The swap charged so far, scaled by `10^moneyDigits`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub swap: Option<i64>,
    /// The entry price.
    #[serde(default)]
    pub price: Option<f64>,
    /// The stop loss price.
    #[serde(default)]
    pub stop_loss: Option<f64>,
    /// The take profit price.
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
    /// The order id.
    #[serde(deserialize_with = "flex::int")]
    pub order_id: i64,
    /// Symbol, volume, side, times.
    pub trade_data: TradeData,
    /// The kind, as its number (see [`Order::kind`]).
    #[serde(deserialize_with = "flex::int")]
    pub order_type: i64,
    /// Where it stands, as its number (see [`Order::status`]).
    #[serde(deserialize_with = "flex::int")]
    pub order_status: i64,
    /// When it expires, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub expiration_timestamp: Option<i64>,
    /// The price it was executed at.
    #[serde(default)]
    pub execution_price: Option<f64>,
    /// The volume executed, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub executed_volume: Option<i64>,
    /// The limit price, for limit orders.
    #[serde(default)]
    pub limit_price: Option<f64>,
    /// The stop price, for stop orders.
    #[serde(default)]
    pub stop_price: Option<f64>,
    /// The stop loss price.
    #[serde(default)]
    pub stop_loss: Option<f64>,
    /// The take profit price.
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
    /// The kind of order.
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
    /// The deal id.
    #[serde(deserialize_with = "flex::int")]
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
    /// The symbol.
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// When the deal was created, in Unix milliseconds.
    #[serde(deserialize_with = "flex::int")]
    pub create_timestamp: i64,
    /// When it was executed, in Unix milliseconds.
    #[serde(deserialize_with = "flex::int")]
    pub execution_timestamp: i64,
    /// The execution price.
    #[serde(default)]
    pub execution_price: Option<f64>,
    /// Buy or sell, as its number (see [`Deal::side`]).
    #[serde(deserialize_with = "flex::int")]
    pub trade_side: i64,
    /// How it ended, as its number (see [`Deal::status`]).
    #[serde(deserialize_with = "flex::int")]
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

    /// How the deal ended.
    #[must_use]
    pub fn status(&self) -> Option<DealStatus> {
        DealStatus::from_number(self.deal_status)
    }
}

/// A deposit or a withdrawal on the account's balance (`ProtoOADepositWithdraw`). `operation_type`
/// is kept as the server's raw number: `ProtoOAChangeBalanceType` has about thirty values (swaps,
/// commissions, rebates, transfers, ...) and this crate does not give each one a name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositWithdraw {
    /// The kind of operation, as `ProtoOAChangeBalanceType`'s number (0 deposit, 1 withdrawal, and
    /// about thirty more for swaps, commissions, rebates, transfers and the rest).
    #[serde(deserialize_with = "flex::int")]
    pub operation_type: i64,
    /// The unique id of the operation.
    #[serde(deserialize_with = "flex::int")]
    pub balance_history_id: i64,
    /// The balance after the operation, scaled by `10^moneyDigits`.
    #[serde(deserialize_with = "flex::int")]
    pub balance: i64,
    /// The amount of the operation, scaled by `10^moneyDigits`.
    #[serde(deserialize_with = "flex::int")]
    pub delta: i64,
    /// When it happened, in Unix milliseconds.
    #[serde(deserialize_with = "flex::int")]
    pub change_balance_timestamp: i64,
    /// A note visible to the trader.
    #[serde(default)]
    pub external_note: Option<String>,
    /// Decimals of the money amounts of this operation.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
}

impl DepositWithdraw {
    /// The amount of the operation as a real number.
    #[must_use]
    pub fn amount(&self) -> f64 {
        money(self.delta, self.digits())
    }

    /// The decimals of money amounts (2 when unknown).
    #[must_use]
    pub fn digits(&self) -> Option<u32> {
        self.money_digits.and_then(|d| u32::try_from(d).ok())
    }
}

/// A deal that offset, or was offset by, another deal (`ProtoOADealOffset`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DealOffset {
    /// The deal id.
    #[serde(deserialize_with = "flex::int")]
    pub deal_id: i64,
    /// The matched volume, in hundredths of a unit.
    #[serde(deserialize_with = "flex::int")]
    pub volume: i64,
    /// When it executed, in Unix milliseconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub execution_timestamp: Option<i64>,
    /// The execution price.
    #[serde(default)]
    pub execution_price: Option<f64>,
}

/// The unrealized profit or loss of one position (`ProtoOAPositionUnrealizedPnL`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionUnrealizedPnL {
    /// The position.
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
            serde_json::from_value(json!({"dealId": 1, "volume": 100, "executionPrice": 1.1}))
                .unwrap();
        assert_eq!(offset.deal_id, 1);
    }
}
