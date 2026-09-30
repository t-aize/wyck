//! Placing, amending and cancelling orders, and closing positions, through `TradingClient`.
//!
//! These calls are not idempotent: after a timeout or a dropped connection the order may still
//! have reached the server. Check `open_positions_and_orders` (match on `label`) before sending
//! it again. Volumes are hundredths of a unit; prices are plain decimals.

pub mod contract;
pub mod events;
pub mod requests;

pub use events::{ExecutionEvent, ExecutionType, OrderErrorEvent, TrailingSlChangedEvent};
pub use requests::{
    AmendOrderReq, AmendPositionSlTpReq, CancelOrderReq, ClosePositionReq, NewOrderReq,
    NewOrderType,
};

mod client;
pub use client::TradingClient;
