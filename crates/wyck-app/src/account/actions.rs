//! What the user asks of the broker for a position or an order: close, reverse, move, protect.

use super::{Account, Busy};
use crate::broker::trading::{AmendOrderReq, AmendPositionSlTpReq, NewOrderReq};
use gpui::Context;
use wyck_trading::TradeSide;

impl Account {
    /// Moves the stop loss of a position to its entry price, keeping the take profit.
    pub fn break_even(&mut self, position_id: i64, cx: &mut Context<Self>) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let Some(entry) = position.price else { return };
        let take_profit = position.take_profit;
        self.protect_position(position_id, Some(entry), take_profit, cx);
    }

    /// A market order of `volume` on a side.
    pub fn market(&mut self, symbol_id: i64, buy: bool, volume: i64, cx: &mut Context<Self>) {
        let side = if buy { TradeSide::Buy } else { TradeSide::Sell };
        self.place(NewOrderReq::market(symbol_id, side, volume), cx);
    }

    /// Closes a position, all of it or `volume` of it.
    pub fn close_position(
        &mut self,
        position_id: i64,
        volume: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let volume = volume.unwrap_or(position.trade_data.volume);
        self.trade(Busy::Closing(position_id), cx, move |trading| async move {
            trading.close_position(position_id, volume).await
        });
    }

    /// Closes every open position (optionally only those of a symbol).
    pub fn close_all(&mut self, symbol: Option<i64>, cx: &mut Context<Self>) {
        let ids: Vec<i64> = self
            .book
            .positions
            .values()
            .filter(|p| symbol.is_none_or(|s| p.trade_data.symbol_id == s))
            .map(|p| p.position_id)
            .collect();
        for id in ids {
            self.close_position(id, None, cx);
        }
    }

    pub fn cancel_order(&mut self, order_id: i64, cx: &mut Context<Self>) {
        self.trade(Busy::Cancelling(order_id), cx, move |trading| async move {
            trading.cancel_order(order_id).await
        });
    }

    /// Moves a working order to a new price (its limit or stop price).
    pub fn move_order(&mut self, order_id: i64, price: f64, cx: &mut Context<Self>) {
        let Some(order) = self.book.orders.get(&order_id) else {
            return;
        };
        let contract = self.book.contract(order.trade_data.symbol_id);
        let price = contract.round_price(price);
        let mut request = AmendOrderReq::new(order_id);
        if order.stop_price.is_some() && order.limit_price.is_none() {
            request.stop_price = Some(price);
        } else {
            request.limit_price = Some(price);
        }
        // The protection stays as it is.
        request.stop_loss = order.stop_loss;
        request.take_profit = order.take_profit;
        self.trade(Busy::Amending(order_id), cx, move |trading| async move {
            trading.amend_order(request).await
        });
    }

    /// Sets the stop loss or take profit of a working order.
    pub fn protect_order(
        &mut self,
        order_id: i64,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        let Some(order) = self.book.orders.get(&order_id) else {
            return;
        };
        let contract = self.book.contract(order.trade_data.symbol_id);
        let mut request = AmendOrderReq::new(order_id);
        request.limit_price = order.limit_price;
        request.stop_price = order.stop_price;
        request.stop_loss = stop_loss.map(|p| contract.round_price(p));
        request.take_profit = take_profit.map(|p| contract.round_price(p));
        self.trade(Busy::Amending(order_id), cx, move |trading| async move {
            trading.amend_order(request).await
        });
    }

    /// Turns the trailing of a position's stop loss on or off, keeping the levels as they are.
    pub fn trail_position(&mut self, position_id: i64, on: bool, cx: &mut Context<Self>) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let contract = self.book.contract(position.trade_data.symbol_id);
        let mut request = AmendPositionSlTpReq::new(position_id);
        request.stop_loss = position.stop_loss.map(|p| contract.round_price(p));
        request.take_profit = position.take_profit.map(|p| contract.round_price(p));
        request.trailing_stop_loss = Some(on);
        self.trade(Busy::Amending(position_id), cx, move |trading| async move {
            trading.amend_position_sl_tp(request).await
        });
    }

    /// Sets the stop loss and take profit of a position (both are sent, so neither is lost).
    pub fn protect_position(
        &mut self,
        position_id: i64,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let contract = self.book.contract(position.trade_data.symbol_id);
        let mut request = AmendPositionSlTpReq::new(position_id);
        request.stop_loss = stop_loss.map(|p| contract.round_price(p));
        request.take_profit = take_profit.map(|p| contract.round_price(p));
        request.trailing_stop_loss = position.trailing_stop_loss;
        self.trade(Busy::Amending(position_id), cx, move |trading| async move {
            trading.amend_position_sl_tp(request).await
        });
    }
}
