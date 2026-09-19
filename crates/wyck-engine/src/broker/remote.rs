//! The [`Broker`] adapter for cTrader's Remote MCP server.
//!
//! Remote speaks integers: prices are pipettes (`price * 10^pipDigits`), volumes are cents
//! of units, money is scaled by `moneyDigits`, and every symbol is a numeric `symbolId`.
//! This adapter decodes all of that into the engine's display-value model and encodes
//! orders back, applying the server quirks `ctrader-mcp` documents:
//!
//! - **`Q-R4`**: a MARKET order rejects absolute stop loss and take profit, so protection is
//!   sent as *relative* distances in points in the same call (`P-REMOTE-MARKET-RELATIVE`).
//! - **`Q-R10`**: amending a position with an omitted leg silently removes it, so
//!   protection changes go through `amend_position_preserving_legs`, which re-reads the
//!   position and always sends both legs.
//! - **`Q-R8`**: one unknown id in a `get_spot_prices` batch empties the whole response, so
//!   only ids known from the session's symbol list are ever sent.
//!
//! What Remote does **not** publish is any per-symbol volume rule (lot size, minimum,
//! step). Instruments built here take those from the engine's `AssumedSpecs` and are marked
//! [`SpecsSource::Assumed`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use ctrader_mcp::common::{money_from_raw, price_from_pipettes, price_to_pipettes};
use ctrader_mcp::remote::dto::{
    ClosePositionParams, CreateOrderParams, RemoteOrder, RemotePosition,
};
use ctrader_mcp::workflows::{RemoteSessionContext, bootstrap_remote};
use ctrader_mcp::{ConnectionConfig, RemoteClient, quirks};
use secrecy::ExposeSecret;

use super::{Broker, ConnectRequest, MarketOrder, PlacedOrder, ServiceKind, parse_server_time};
use crate::config::AssumedSpecs;
use crate::domain::{
    AccountKind, AccountSnapshot, Instrument, OrderKind, PendingOrder, Position, Quote, Side,
    SpecsSource, UnixMillis, Volume, VolumeSpecs, now_millis,
};
use crate::error::{BrokerErrorKind, EngineError, Result};
use crate::ids::{AccountId, OrderId, PositionId};

/// The size of one pip for a symbol with `price_digits` decimals: one pipette-order coarser
/// on 3-digit and longer symbols (`0.0001` for EURUSD at 5 digits, `0.01` for USDJPY at 3),
/// equal to the last digit on shorter ones (`0.01` for a 2-digit metal). Mirrors
/// `ctrader_mcp::math::pip::pips_to_points`.
pub(crate) fn pip_size_for_digits(price_digits: u32) -> f64 {
    let one = 10f64.powi(-i32::try_from(price_digits).unwrap_or(5));
    if matches!(price_digits, 3 | 5) {
        one * 10.0
    } else {
        one
    }
}

/// Remote's [`Broker`]. Built by [`RemoteBroker::connect`].
pub struct RemoteBroker {
    client: RemoteClient,
    session: RemoteSessionContext,
    account_id: AccountId,
    kind: AccountKind,
    instruments: Vec<Instrument>,
    by_name: HashMap<String, usize>,
    by_id: HashMap<i64, usize>,
    money_digits: u32,
    closed: AtomicBool,
}

impl std::fmt::Debug for RemoteBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteBroker")
            .field("account_id", &self.account_id)
            .field("instruments", &self.instruments.len())
            .finish_non_exhaustive()
    }
}

impl RemoteBroker {
    /// Connects to the server and bootstraps the session (build id, account, assets,
    /// symbols, trading profile).
    ///
    /// # Errors
    ///
    /// A [`EngineError::Broker`] if the connection or any bootstrap read fails.
    pub async fn connect(request: &ConnectRequest, assumed: &AssumedSpecs) -> Result<Self> {
        let mut config = ConnectionConfig::new(&request.endpoint);
        if let Some(token) = &request.token {
            config = config.with_bearer_token(token.expose_secret());
        }
        let client = RemoteClient::connect(&config).await?;
        let session = bootstrap_remote(&client).await?;
        let kind = request
            .token
            .as_ref()
            .map_or(AccountKind::Unknown, AccountKind::from_token);
        Ok(Self::from_session(client, session, kind, assumed))
    }

    fn from_session(
        client: RemoteClient,
        session: RemoteSessionContext,
        kind: AccountKind,
        assumed: &AssumedSpecs,
    ) -> Self {
        let account_id = AccountId::new(
            session
                .trader_id
                .map_or_else(|| "remote".to_owned(), |id| id.to_string()),
        );
        let instruments: Vec<Instrument> = session
            .symbols
            .iter()
            .map(|symbol| {
                let digits = symbol.pip_digits.unwrap_or(5);
                Instrument {
                    symbol: symbol.symbol_name.clone(),
                    symbol_id: Some(symbol.symbol_id),
                    price_digits: digits,
                    pip_size: pip_size_for_digits(digits),
                    base_currency: symbol
                        .base_asset_id
                        .and_then(|id| session.find_asset_name(id))
                        .map(str::to_owned),
                    quote_currency: symbol
                        .quote_asset_id
                        .and_then(|id| session.find_asset_name(id))
                        .map(str::to_owned),
                    enabled: symbol.enabled.unwrap_or(true),
                    volume: VolumeSpecs {
                        lot_size: assumed.lot_size,
                        min: Volume::from_units(assumed.min_volume_units),
                        step: Volume::from_units(assumed.volume_step_units),
                        max: None,
                    },
                    specs_source: SpecsSource::Assumed,
                }
            })
            .collect();
        let by_name = instruments
            .iter()
            .enumerate()
            .map(|(i, ins)| (ins.symbol.to_ascii_uppercase(), i))
            .collect();
        let by_id = instruments
            .iter()
            .enumerate()
            .filter_map(|(i, ins)| ins.symbol_id.map(|id| (id, i)))
            .collect();
        Self {
            money_digits: session.money_digits.unwrap_or(2),
            client,
            session,
            account_id,
            kind,
            instruments,
            by_name,
            by_id,
            closed: AtomicBool::new(false),
        }
    }

    fn instrument_by_name(&self, symbol: &str) -> Result<&Instrument> {
        self.by_name
            .get(&symbol.to_ascii_uppercase())
            .map(|&i| &self.instruments[i])
            .ok_or_else(|| EngineError::Invalid(format!("unknown symbol `{symbol}`")))
    }

    fn instrument_by_id(&self, id: i64) -> Option<&Instrument> {
        self.by_id.get(&id).map(|&i| &self.instruments[i])
    }

    fn price(&self, instrument: &Instrument, pipettes: i64) -> f64 {
        price_from_pipettes(pipettes, instrument.price_digits)
    }

    fn money(&self, raw: i64) -> f64 {
        money_from_raw(raw, self.money_digits)
    }

    fn decode_position(&self, raw: &RemotePosition) -> Option<Position> {
        let id = raw.position_id?;
        let instrument = self.instrument_by_id(raw.symbol_id?)?;
        let side = Side::parse(raw.trade_side.as_deref()?)?;
        let price = |v: Option<i64>| v.map(|p| self.price(instrument, p));
        Some(Position {
            id: PositionId(id),
            symbol: instrument.symbol.clone(),
            side,
            volume: Volume::from_cents(raw.volume?),
            entry_price: price(raw.entry_price),
            stop_loss: price(raw.stop_loss),
            take_profit: price(raw.take_profit),
            swap: raw.swap.map(|v| self.money(v)),
            commission: raw.commission.map(|v| self.money(v)),
            unrealized_pnl: raw.unrealized_pnl.map(|v| self.money(v)),
            label: raw
                .extra
                .get("label")
                .or_else(|| raw.extra.get("comment"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        })
    }

    fn decode_order(&self, raw: &RemoteOrder) -> Option<PendingOrder> {
        let id = raw.order_id?;
        let instrument = self.instrument_by_id(raw.symbol_id?)?;
        let kind = match raw
            .order_type
            .as_deref()
            .map(str::to_ascii_uppercase)
            .as_deref()
        {
            Some("LIMIT") => OrderKind::Limit,
            Some("STOP") => OrderKind::Stop,
            Some("STOP_LIMIT") => OrderKind::StopLimit,
            _ => OrderKind::Other,
        };
        let pipettes = |k: &str| raw.extra.get(k).and_then(serde_json::Value::as_i64);
        Some(PendingOrder {
            id: OrderId(id),
            symbol: instrument.symbol.clone(),
            side: Side::parse(raw.trade_side.as_deref()?)?,
            kind,
            volume: Volume::from_cents(raw.volume?),
            price: raw
                .limit_price
                .or(raw.stop_price)
                .map(|p| self.price(instrument, p)),
            stop_loss: pipettes("stopLoss").map(|p| self.price(instrument, p)),
            take_profit: pipettes("takeProfit").map(|p| self.price(instrument, p)),
        })
    }
}

#[async_trait]
impl Broker for RemoteBroker {
    fn service(&self) -> ServiceKind {
        ServiceKind::CtraderRemote
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn can_trade(&self) -> bool {
        self.session.has_trading_profile
    }

    fn stop_granularity(&self, instrument: &Instrument) -> f64 {
        // Relative distances travel as integer points, one pipette each.
        10f64.powi(-i32::try_from(instrument.price_digits).unwrap_or(5))
    }

    fn label_prefix(&self) -> String {
        self.session.idempotency_prefix.clone()
    }

    async fn account(&self) -> Result<AccountSnapshot> {
        let balance = self.client.get_balance().await?;
        Ok(AccountSnapshot {
            account_id: self.account_id.clone(),
            kind: self.kind,
            currency: self.session.account_currency.clone(),
            balance: balance.display_balance(),
            equity: balance.display_equity(),
            free_margin: balance.display_free_margin(),
            used_margin: None,
            margin_level_pct: None,
            server_version: self.session.version.clone(),
            captured_at: now_millis(),
        })
    }

    async fn symbols(&self) -> Result<Vec<String>> {
        Ok(self.instruments.iter().map(|i| i.symbol.clone()).collect())
    }

    async fn instrument(&self, symbol: &str) -> Result<Instrument> {
        self.instrument_by_name(symbol).cloned()
    }

    async fn positions(&self) -> Result<Vec<Position>> {
        let response = self.client.get_positions().await?;
        Ok(response
            .positions
            .iter()
            .filter_map(|p| {
                let decoded = self.decode_position(p);
                if decoded.is_none() {
                    tracing::warn!(position_id = ?p.position_id, "skipping a position that could not be decoded");
                }
                decoded
            })
            .collect())
    }

    async fn pending_orders(&self) -> Result<Vec<PendingOrder>> {
        let response = self.client.get_positions().await?;
        Ok(response
            .orders
            .iter()
            .filter_map(|o| self.decode_order(o))
            .collect())
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        let mut ids = Vec::new();
        for symbol in symbols {
            // Only ids this session knows: one unknown id would empty the whole batch (Q-R8).
            if let Ok(instrument) = self.instrument_by_name(symbol)
                && let Some(id) = instrument.symbol_id
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let response = self.client.get_spot_prices(ids).await?;
        Ok(response
            .prices
            .iter()
            .filter_map(|p| {
                let instrument = self.instrument_by_id(p.symbol_id?)?;
                Some(Quote {
                    symbol: instrument.symbol.clone(),
                    bid: self.price(instrument, p.bid?),
                    ask: self.price(instrument, p.ask?),
                    timestamp: p.timestamp,
                })
            })
            .collect())
    }

    async fn server_time(&self) -> Result<UnixMillis> {
        let value = self.client.get_server_time().await?;
        parse_server_time(&value).ok_or_else(|| EngineError::Broker {
            kind: BrokerErrorKind::Protocol,
            retryable: false,
            message: "get_server_time returned no recognizable timestamp".to_owned(),
        })
    }

    async fn ping(&self) -> Result<()> {
        Ok(self.client.ping().await?)
    }

    async fn place_market(&self, order: &MarketOrder) -> Result<PlacedOrder> {
        if !self.can_trade() {
            return Err(EngineError::TradingUnavailable(
                "the Remote session is bound to the read-only `data` profile".to_owned(),
            ));
        }
        let instrument = self.instrument_by_name(&order.symbol)?;
        let symbol_id = instrument
            .symbol_id
            .ok_or_else(|| EngineError::Internal("remote instrument without an id".to_owned()))?;
        let side = order.side.to_trade_side();

        let mut params = CreateOrderParams::market(symbol_id, side, order.volume.to_cents());
        // Distances travel as integer points (one pipette each), at least 1.
        let points = |distance: f64| price_to_pipettes(distance, instrument.price_digits).max(1);
        params.relative_stop_loss = order.stop_loss_distance.map(points);
        params.relative_take_profit = order.take_profit_distance.map(points);
        params.slippage_in_points = order.slippage_points;
        params = params.with_label(order.label.clone());

        let response = self.client.create_order(params).await?;
        Ok(PlacedOrder {
            position_id: response
                .position
                .as_ref()
                .and_then(|p| p.position_id)
                .map(PositionId),
            order_id: response
                .order
                .as_ref()
                .and_then(|o| o.order_id)
                .map(OrderId),
        })
    }

    async fn set_protection(
        &self,
        position: &Position,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<()> {
        if !self.can_trade() {
            return Err(EngineError::TradingUnavailable(
                "the Remote session is bound to the read-only `data` profile".to_owned(),
            ));
        }
        let instrument = self.instrument_by_name(&position.symbol)?;
        let pipettes = |price: f64| price_to_pipettes(price, instrument.price_digits);
        // Q-R10: an omitted leg is silently removed, so re-read and send both.
        quirks::amend_position_preserving_legs(
            &self.client,
            position.id.get(),
            stop_loss.map(pipettes),
            take_profit.map(pipettes),
        )
        .await?;
        Ok(())
    }

    async fn close_position(&self, position: &Position, volume: Option<Volume>) -> Result<()> {
        if !self.can_trade() {
            return Err(EngineError::TradingUnavailable(
                "the Remote session is bound to the read-only `data` profile".to_owned(),
            ));
        }
        // Remote has no close-without-volume form: always pass a volume.
        let volume = volume.unwrap_or(position.volume).min(position.volume);
        self.client
            .close_position(ClosePositionParams {
                position_id: position.id.get(),
                volume: volume.to_cents(),
            })
            .await?;
        Ok(())
    }

    async fn cancel_order(&self, order: OrderId) -> Result<()> {
        if !self.can_trade() {
            return Err(EngineError::TradingUnavailable(
                "the Remote session is bound to the read-only `data` profile".to_owned(),
            ));
        }
        self.client.cancel_order(order.get()).await?;
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        // The MCP session closes when the client is dropped, which happens when the last
        // reference to this broker goes away. `close` only records the intent so callers
        // and tests can observe it.
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pip_size_follows_the_pip_digits_convention() {
        assert!((pip_size_for_digits(5) - 0.0001).abs() < 1e-15, "EURUSD");
        assert!((pip_size_for_digits(3) - 0.01).abs() < 1e-15, "USDJPY");
        assert!((pip_size_for_digits(4) - 0.0001).abs() < 1e-15, "4 digits");
        assert!(
            (pip_size_for_digits(2) - 0.01).abs() < 1e-15,
            "2-digit metal"
        );
        assert!((pip_size_for_digits(1) - 0.1).abs() < 1e-15);
    }
}
