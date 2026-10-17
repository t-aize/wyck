//! The enumerations of the account messages, which arrive as numbers.
//!
//! Each one has a `from_number` that gives `None` for a number it does not know, so a server that
//! adds a value later shows up as a missing label and not as a crash. The structs of [`crate::types`]
//! keep the raw number next to a method that decodes it.

/// Defines an enumeration that maps to server numbers and back. For the files of this folder only.
#[doc(hidden)]
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

pub(crate) use number_enum;

number_enum! {
    /// Buy or sell.
    TradeSide {
        Buy = 1 => "buy",
        Sell = 2 => "sell",
    }
}

number_enum! {
    OrderType {
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
    /// The kind of an order request (`ProtoOAOrderType`, the subset a caller chooses between when
    /// placing one; [`OrderType`] decodes any order the server sends back, including
    /// `StopLossTakeProfit`, which a caller never asks for directly).
    NewOrderType {
        /// At the current market price.
        Market = 1 => "market",
        /// At `limit_price` or better.
        Limit = 2 => "limit",
        /// Becomes a market order once `stop_price` is reached.
        Stop = 3 => "stop",
        /// At the market, within `slippage_in_points` of `base_slippage_price`.
        MarketRange = 5 => "market range",
        /// Becomes a limit order once `stop_price` is reached.
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
        Rejected = 3 => "rejected",
        /// Ran out of time.
        Expired = 4 => "expired",
        Cancelled = 5 => "cancelled",
    }
}

number_enum! {
    /// Where a position stands.
    PositionStatus {
        Open = 1 => "open",
        Closed = 2 => "closed",
        Created = 3 => "created",
        Error = 4 => "error",
    }
}

number_enum! {
    DealStatus {
        Filled = 2 => "filled",
        PartiallyFilled = 3 => "partially filled",
        /// Refused by the server.
        Rejected = 4 => "rejected",
        /// Refused inside the platform.
        InternallyRejected = 5 => "internally rejected",
        Error = 6 => "error",
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
        FullAccess = 0 => "full access",
        /// Positions may only be closed.
        CloseOnly = 1 => "close only",
        NoTrading = 2 => "no trading",
        NoLogin = 3 => "no login",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
