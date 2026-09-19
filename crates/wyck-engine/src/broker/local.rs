//! The [`Broker`] adapter for the MCP server inside cTrader Desktop (Local).
//!
//! Local speaks floating point: display prices, ticker names instead of ids, stop loss and
//! take profit on market orders as *pip distances* (`Q-L2`), and volumes in a unit the
//! broker defines.
//!
//! # Volume unit (read this before trading through Local)
//!
//! The skill documents Local volumes as broker defined: `get_symbol_details` returns
//! `lotSize`, `minVolume` and `volumeStep`, and the same symbol can have a different
//! `lotSize` at different brokers. This adapter reads a raw Local volume `v` as `v` lots of
//! `lotSize` base units, so the engine's [`Volume`] is `round(v * lotSize)` units. If a
//! symbol reports a lot size that would make the smallest tradable volume round to zero
//! units, the adapter refuses that symbol with a clear error instead of guessing. This
//! mapping has **not** been verified against a live server; the live validation milestone
//! (`TODO.md`) must confirm it before real orders go through Local.
//!
//! Local can only fetch symbol details one call at a time, so they are loaded on first use
//! and cached for the session.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use ctrader_mcp::local::dto::{AmendPositionParams, PlaceMarketOrderParams, SymbolDetails};
use ctrader_mcp::time::local_iso8601_to_epoch_millis;
use ctrader_mcp::{ConnectionConfig, LocalClient};

use super::{Broker, ConnectRequest, MarketOrder, PlacedOrder, ServiceKind, parse_server_time};
use crate::config::AssumedSpecs;
use crate::domain::{
    AccountKind, AccountSnapshot, Instrument, OrderKind, PendingOrder, Position, Quote, Side,
    SpecsSource, UnixMillis, Volume, VolumeSpecs, now_millis,
};
use crate::error::{BrokerErrorKind, EngineError, Result};
use crate::ids::{AccountId, OrderId, PositionId};

/// Local's [`Broker`]. Built by [`LocalBroker::connect`].
pub struct LocalBroker {
    client: LocalClient,
    account_id: AccountId,
    kind: AccountKind,
    assumed: AssumedSpecs,
    label_prefix: String,
    instruments: Mutex<HashMap<String, Instrument>>,
    closed: AtomicBool,
}

impl std::fmt::Debug for LocalBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalBroker")
            .field("account_id", &self.account_id)
            .finish_non_exhaustive()
    }
}

/// Builds an [`Instrument`] from Local's `get_symbol_details`.
///
/// Fails closed when the details cannot produce a sound volume model. See the
/// [module docs](self).
fn instrument_from_details(
    symbol: &str,
    details: &SymbolDetails,
    assumed: &AssumedSpecs,
) -> Result<Instrument> {
    let invalid = |why: &str| {
        EngineError::Invalid(format!(
            "cannot model the volume of {symbol} on Local: {why}"
        ))
    };
    let broker_lot = details.lot_size.filter(|v| v.is_finite() && *v > 0.0);
    let lot_size = broker_lot.unwrap_or(assumed.lot_size);
    let to_units = |raw: f64| Volume::from_lots(raw, lot_size);

    let (min, step, source) = match (details.min_volume, details.volume_step) {
        (Some(min), Some(step)) if min > 0.0 && step > 0.0 && broker_lot.is_some() => {
            (to_units(min), to_units(step), SpecsSource::Broker)
        }
        _ => (
            Volume::from_units(assumed.min_volume_units),
            Volume::from_units(assumed.volume_step_units),
            SpecsSource::Assumed,
        ),
    };
    if !min.is_positive() || !step.is_positive() {
        return Err(invalid(
            "the smallest volume or the step rounds to zero units at the reported lot size",
        ));
    }
    let digits = details.digits.unwrap_or(5);
    let pip_size = details
        .pip_size
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or_else(|| super::remote::pip_size_for_digits(digits));
    let (base, quote) = split_pair(symbol);
    Ok(Instrument {
        symbol: symbol.to_owned(),
        symbol_id: None,
        price_digits: digits,
        pip_size,
        base_currency: base,
        quote_currency: quote,
        enabled: true,
        volume: VolumeSpecs {
            lot_size,
            min,
            step,
            max: None,
        },
        specs_source: source,
    })
}

/// Splits a six-letter pair such as `EURUSD` into currencies. Anything else is `(None,
/// None)`; the engine never guesses currencies for indices or metals here.
fn split_pair(symbol: &str) -> (Option<String>, Option<String>) {
    let letters: String = symbol
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if symbol.len() == 6 && letters.len() == 6 {
        (Some(letters[..3].to_owned()), Some(letters[3..].to_owned()))
    } else {
        (None, None)
    }
}

impl LocalBroker {
    /// Connects to the server and reads the account.
    ///
    /// # Errors
    ///
    /// A [`EngineError::Broker`] if the connection or the balance read fails.
    pub async fn connect(request: &ConnectRequest, assumed: &AssumedSpecs) -> Result<Self> {
        let client = LocalClient::connect(&ConnectionConfig::new(&request.endpoint)).await?;
        let balance = client.get_balance().await?;
        let kind = match balance
            .account_type
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some(t) if t.contains("demo") => AccountKind::Demo,
            Some(t) if t.contains("live") || t.contains("real") => AccountKind::Live,
            _ => AccountKind::Unknown,
        };
        let account_id = AccountId::new(
            balance
                .trader_id
                .map_or_else(|| "local".to_owned(), |id| id.to_string()),
        );
        Ok(Self {
            client,
            account_id,
            kind,
            assumed: assumed.clone(),
            label_prefix: format!("wyck-{}", &uuid::Uuid::new_v4().to_string()[..8]),
            instruments: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
        })
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, HashMap<String, Instrument>> {
        self.instruments
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn instrument_cached(&self, symbol: &str) -> Result<Instrument> {
        let key = symbol.to_ascii_uppercase();
        if let Some(found) = self.cache().get(&key) {
            return Ok(found.clone());
        }
        let details = self.client.get_symbol_details(symbol).await?;
        let name = details.symbol_name.as_deref().unwrap_or(symbol);
        let instrument = instrument_from_details(name, &details, &self.assumed)?;
        self.cache().insert(key, instrument.clone());
        Ok(instrument)
    }
}

#[async_trait]
impl Broker for LocalBroker {
    fn service(&self) -> ServiceKind {
        ServiceKind::CtraderLocal
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn can_trade(&self) -> bool {
        true
    }

    fn stop_granularity(&self, instrument: &Instrument) -> f64 {
        // Local takes whole pips for market-order protection (Q-L2).
        instrument.pip_size
    }

    fn label_prefix(&self) -> String {
        self.label_prefix.clone()
    }

    async fn account(&self) -> Result<AccountSnapshot> {
        let b = self.client.get_balance().await?;
        Ok(AccountSnapshot {
            account_id: self.account_id.clone(),
            kind: self.kind,
            currency: b.currency,
            balance: b.balance,
            equity: b.equity,
            free_margin: b.free_margin,
            used_margin: b.margin,
            margin_level_pct: b.margin_level,
            server_version: None,
            captured_at: now_millis(),
        })
    }

    async fn symbols(&self) -> Result<Vec<String>> {
        let response = self.client.get_symbols(None).await?;
        Ok(response
            .symbols
            .into_iter()
            .map(|s| s.symbol_name)
            .collect())
    }

    async fn instrument(&self, symbol: &str) -> Result<Instrument> {
        self.instrument_cached(symbol).await
    }

    async fn positions(&self) -> Result<Vec<Position>> {
        let response = self.client.get_positions().await?;
        let mut out = Vec::with_capacity(response.positions.len());
        for raw in &response.positions {
            let (Some(id), Some(symbol), Some(side), Some(volume)) = (
                raw.id,
                raw.symbol_name.as_deref(),
                raw.trade_side.as_deref().and_then(Side::parse),
                raw.volume,
            ) else {
                tracing::warn!(position_id = ?raw.id, "skipping a position that could not be decoded");
                continue;
            };
            let instrument = self.instrument_cached(symbol).await?;
            out.push(Position {
                id: PositionId(id),
                symbol: instrument.symbol.clone(),
                side,
                volume: Volume::from_lots(volume, instrument.volume.lot_size),
                entry_price: raw.entry_price,
                stop_loss: raw.stop_loss,
                take_profit: raw.take_profit,
                swap: raw.swap,
                commission: raw.commission,
                unrealized_pnl: raw
                    .extra
                    .get("unrealizedPnl")
                    .or_else(|| raw.extra.get("netProfit"))
                    .and_then(serde_json::Value::as_f64),
                label: raw
                    .extra
                    .get("label")
                    .or_else(|| raw.extra.get("comment"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            });
        }
        Ok(out)
    }

    async fn pending_orders(&self) -> Result<Vec<PendingOrder>> {
        let response = self.client.get_pending_orders().await?;
        let mut out = Vec::with_capacity(response.orders.len());
        for raw in &response.orders {
            let (Some(id), Some(symbol), Some(side), Some(volume)) = (
                raw.id,
                raw.symbol_name.as_deref(),
                raw.trade_side.as_deref().and_then(Side::parse),
                raw.volume,
            ) else {
                continue;
            };
            let instrument = self.instrument_cached(symbol).await?;
            let kind = match raw
                .order_type
                .as_deref()
                .map(str::to_ascii_uppercase)
                .as_deref()
            {
                Some("LIMIT") => OrderKind::Limit,
                Some("STOP") => OrderKind::Stop,
                Some("STOP_LIMIT" | "STOPLIMIT") => OrderKind::StopLimit,
                _ => OrderKind::Other,
            };
            out.push(PendingOrder {
                id: OrderId(id),
                symbol: instrument.symbol.clone(),
                side,
                kind,
                volume: Volume::from_lots(volume, instrument.volume.lot_size),
                price: raw.target_price.or(raw.entry_price),
                stop_loss: raw.stop_loss,
                take_profit: raw.take_profit,
            });
        }
        Ok(out)
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        let mut out = Vec::with_capacity(symbols.len());
        for symbol in symbols {
            match self.client.get_spot_prices(symbol).await {
                Ok(price) => {
                    if let (Some(bid), Some(ask)) = (price.bid, price.ask) {
                        out.push(Quote {
                            symbol: symbol.clone(),
                            bid,
                            ask,
                            timestamp: price
                                .timestamp
                                .as_deref()
                                .and_then(|t| local_iso8601_to_epoch_millis(t).ok()),
                        });
                    }
                }
                Err(error) => match EngineError::from(error) {
                    // A symbol Local refuses to quote is absent from the result, not an error.
                    EngineError::Broker {
                        kind: BrokerErrorKind::Rejected,
                        ..
                    } => tracing::debug!(%symbol, "no quote available"),
                    other => return Err(other),
                },
            }
        }
        Ok(out)
    }

    async fn server_time(&self) -> Result<UnixMillis> {
        let response = self.client.get_server_time().await?;
        parse_server_time(&serde_json::Value::Object(response.extra)).ok_or_else(|| {
            EngineError::Broker {
                kind: BrokerErrorKind::Protocol,
                retryable: false,
                message: "get_server_time returned no recognizable timestamp".to_owned(),
            }
        })
    }

    async fn ping(&self) -> Result<()> {
        Ok(self.client.ping().await?)
    }

    async fn place_market(&self, order: &MarketOrder) -> Result<PlacedOrder> {
        let instrument = self.instrument_cached(&order.symbol).await?;
        let pips = |distance: f64| -> i64 {
            // Whole pips, at least one. The planner already rounded distances to this
            // granularity, so this is exact for planned orders.
            (distance / instrument.pip_size).round().max(1.0) as i64
        };
        let mut params = PlaceMarketOrderParams::new(
            order.symbol.clone(),
            order.side.to_trade_side(),
            order.volume.to_lots(instrument.volume.lot_size),
        );
        params.stop_loss_pips = order.stop_loss_distance.map(pips);
        params.take_profit_pips = order.take_profit_distance.map(pips);
        params.label = Some(order.label.clone());
        let response = self.client.place_market_order(params).await?;
        Ok(PlacedOrder {
            position_id: None,
            order_id: response.order_id.map(OrderId),
        })
    }

    async fn set_protection(
        &self,
        position: &Position,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<()> {
        self.client
            .amend_position(AmendPositionParams {
                position_id: position.id.get(),
                stop_loss,
                take_profit,
            })
            .await?;
        Ok(())
    }

    async fn close_position(&self, position: &Position, volume: Option<Volume>) -> Result<()> {
        match volume {
            Some(v) if v < position.volume => {
                let instrument = self.instrument_cached(&position.symbol).await?;
                self.client
                    .close_position_partial(
                        position.id.get(),
                        v.to_lots(instrument.volume.lot_size),
                    )
                    .await?;
            }
            _ => {
                self.client.close_position(position.id.get()).await?;
            }
        }
        Ok(())
    }

    async fn cancel_order(&self, order: OrderId) -> Result<()> {
        self.client.cancel_order(order.get()).await?;
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn details(value: serde_json::Value) -> SymbolDetails {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn broker_published_specs_are_used_and_converted_to_units() {
        let d = details(json!({
            "symbolName": "EURUSD", "lotSize": 100000.0, "minVolume": 0.01,
            "volumeStep": 0.01, "digits": 5, "pipSize": 0.0001
        }));
        let i = instrument_from_details("EURUSD", &d, &AssumedSpecs::default()).unwrap();
        assert_eq!(i.specs_source, SpecsSource::Broker);
        assert_eq!(i.volume.min, Volume::from_units(1_000));
        assert_eq!(i.volume.step, Volume::from_units(1_000));
        assert_eq!(i.base_currency.as_deref(), Some("EUR"));
        assert_eq!(i.quote_currency.as_deref(), Some("USD"));
    }

    #[test]
    fn missing_specs_fall_back_to_assumed_and_say_so() {
        let d = details(json!({ "symbolName": "XAUUSD", "digits": 2 }));
        let i = instrument_from_details("XAUUSD", &d, &AssumedSpecs::default()).unwrap();
        assert_eq!(i.specs_source, SpecsSource::Assumed);
        assert!((i.pip_size - 0.01).abs() < 1e-12);
        assert_eq!(i.base_currency, Some("XAU".to_owned()));
    }

    #[test]
    fn a_lot_size_that_erases_the_minimum_volume_fails_closed() {
        // lotSize 1 with minVolume 0.01 would round to zero units: refuse rather than guess.
        let d = details(json!({
            "lotSize": 1.0, "minVolume": 0.01, "volumeStep": 0.01, "digits": 5
        }));
        let err = instrument_from_details("EURUSD", &d, &AssumedSpecs::default()).unwrap_err();
        assert!(matches!(err, EngineError::Invalid(m) if m.contains("rounds to zero")));
    }

    #[test]
    fn only_six_letter_symbols_get_currencies() {
        assert_eq!(
            split_pair("GBPJPY"),
            (Some("GBP".into()), Some("JPY".into()))
        );
        assert_eq!(split_pair("US30"), (None, None));
        assert_eq!(split_pair("GER40"), (None, None));
    }
}
