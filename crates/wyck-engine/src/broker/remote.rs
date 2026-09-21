//! The [`Broker`] adapter for cTrader's Remote MCP server.
//!
//! Remote speaks integers: prices are pipettes (`price * 10^5`), volumes are cents
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
//! What Remote does **not** publish, checked against a live server (2026-09):
//!
//! - **Volume rules** (lot size, minimum, step). Instruments take those from the engine's
//!   `AssumedSpecs`: the per-symbol rules the user configured
//!   ([`SpecsSource::Configured`]), or the global guess ([`SpecsSource::Assumed`]).
//! - **Price digits and pip size.** `get_symbols` has no `pipDigits`. Every raw price is an
//!   integer in units of 1e-5 whatever the symbol (`15676800` is USDJPY 156.768), so that
//!   scale is a constant, [`PIPETTE_DIGITS`]. The number of decimals a symbol really quotes
//!   is inferred from the trailing zeros of observed quotes. That can only under-estimate
//!   it, which is the safe direction: prices and stop distances come out coarser than
//!   needed, never finer. The pip size follows the forex convention from those digits, so
//!   it is only trustworthy for currency pairs.
//! - **A server clock.** There is no `get_server_time` tool on Remote.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use ctrader_mcp::common::{money_from_raw, price_from_pipettes, price_to_pipettes};
use ctrader_mcp::remote::dto::{
    ClosePositionParams, CreateOrderParams, RemoteOrder, RemotePosition,
};
use ctrader_mcp::workflows::{RemoteSessionContext, bootstrap_remote};
use ctrader_mcp::{ConnectionConfig, RemoteClient, quirks};

use super::{Broker, ConnectRequest, MarketOrder, PlacedOrder, ServiceKind};
use crate::config::AssumedSpecs;
use crate::domain::{
    AccountKind, AccountSnapshot, Candle, Instrument, OrderKind, PendingOrder, Period, Position,
    Quote, Side, SpecsSource, SymbolInfo, UnixMillis, Volume, VolumeSpecs, now_millis,
};
use crate::error::{BrokerErrorKind, EngineError, Result};
use crate::ids::{AccountId, OrderId, PositionId};

/// Decimals of Remote's integer price encoding: every symbol is quoted in units of 1e-5.
pub(crate) const PIPETTE_DIGITS: u32 = 5;

fn pipette_price(raw: i64) -> f64 {
    price_from_pipettes(raw, PIPETTE_DIGITS)
}

/// The number of decimals a symbol quotes, judged from raw prices (integers in units of
/// 1e-5): five minus the trailing decimal zeros every value shares. `None` when there is
/// nothing but zeros to look at.
pub(crate) fn digits_from_pipettes(values: &[i64]) -> Option<u32> {
    let shared_zeros = values
        .iter()
        .filter(|v| **v != 0)
        .map(|v| {
            let mut rest = v.unsigned_abs();
            let mut zeros = 0;
            while rest % 10 == 0 {
                rest /= 10;
                zeros += 1;
            }
            zeros
        })
        .min()?;
    Some(PIPETTE_DIGITS.saturating_sub(shared_zeros))
}

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

/// Used margin and margin level from equity and free margin. Remote's `get_balance` has no
/// margin field, but checked live: with one open BTCUSD position, `equity - freeMargin` was
/// the margin the platform showed. `None` when nothing is in use (no positions), which is the
/// same "no margin level" answer Local gives.
fn derive_margin(equity: Option<f64>, free_margin: Option<f64>) -> (Option<f64>, Option<f64>) {
    let (Some(equity), Some(free)) = (equity, free_margin) else {
        return (None, None);
    };
    let used = equity - free;
    // Sub-cent differences are rounding, not margin.
    if used > 0.005 {
        (Some(used), Some(equity / used * 100.0))
    } else {
        (Some(0.0), None)
    }
}

/// The volume rules for a symbol: what the user configured for it, else the global guess.
fn volume_rules(assumed: &AssumedSpecs, symbol: &str) -> (VolumeSpecs, SpecsSource) {
    match assumed.rules_for(symbol) {
        Some(rules) => (
            VolumeSpecs {
                lot_size: rules.lot_size,
                min: Volume::from_units_f64(rules.min_volume),
                step: Volume::from_units_f64(rules.volume_step),
                max: rules.max_volume.map(Volume::from_units_f64),
            },
            SpecsSource::Configured,
        ),
        None => (
            VolumeSpecs {
                lot_size: assumed.lot_size,
                min: Volume::from_units(assumed.min_volume_units),
                step: Volume::from_units(assumed.volume_step_units),
                max: None,
            },
            SpecsSource::Assumed,
        ),
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
    /// Decimals inferred per symbol id from observed quotes, see the module docs.
    digits: Mutex<HashMap<i64, u32>>,
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
            config = config.with_bearer_token(token.clone());
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
                let digits = symbol.pip_digits.unwrap_or(PIPETTE_DIGITS);
                let (volume, specs_source) = volume_rules(assumed, &symbol.symbol_name);
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
                    volume,
                    specs_source,
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
            digits: Mutex::new(HashMap::new()),
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

    fn known_digits(&self) -> std::sync::MutexGuard<'_, HashMap<i64, u32>> {
        self.digits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records what raw prices say about a symbol's decimals. Digits only ever go up: a
    /// larger sample can reveal more precision, never less.
    fn observe(&self, symbol_id: i64, raw: &[i64]) {
        if let Some(digits) = digits_from_pipettes(raw) {
            let mut known = self.known_digits();
            let entry = known.entry(symbol_id).or_insert(digits);
            *entry = (*entry).max(digits);
        }
    }

    /// `instrument` with the decimals and pip size the quotes have shown so far.
    fn with_precision(&self, instrument: &Instrument) -> Instrument {
        let mut out = instrument.clone();
        if let Some(digits) = instrument
            .symbol_id
            .and_then(|id| self.known_digits().get(&id).copied())
        {
            out.price_digits = digits;
            out.pip_size = pip_size_for_digits(digits);
        }
        out
    }

    /// A money amount from a position. `get_balance` scales money by `10^moneyDigits`, but
    /// positions were only ever seen with `0` in these fields, so the scale of a non-zero
    /// one is a guess: a whole number is read as scaled, a fractional one as already
    /// decimal. Informational only, never used for sizing.
    fn money(&self, raw: f64) -> f64 {
        if raw.fract() == 0.0 {
            #[allow(clippy::cast_possible_truncation)]
            money_from_raw(raw as i64, self.money_digits)
        } else {
            raw
        }
    }

    fn decode_position(&self, raw: &RemotePosition) -> Option<Position> {
        let id = raw.position_id?;
        let instrument = self.instrument_by_id(raw.symbol_id?)?;
        let side = Side::parse(raw.trade_side.as_deref()?)?;
        Some(Position {
            id: PositionId(id),
            symbol: instrument.symbol.clone(),
            side,
            volume: Volume::from_cents(raw.volume?),
            // Display prices, see `ctrader_mcp::remote::dto`.
            entry_price: raw.entry_price.filter(|p| *p > 0.0),
            stop_loss: raw.stop_loss,
            take_profit: raw.take_profit,
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
        Some(PendingOrder {
            id: OrderId(id),
            symbol: instrument.symbol.clone(),
            side: Side::parse(raw.trade_side.as_deref()?)?,
            kind,
            volume: Volume::from_cents(raw.volume?),
            price: raw.limit_price.or(raw.stop_price),
            stop_loss: raw.stop_loss,
            take_profit: raw.take_profit,
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
        let (equity, free_margin) = (balance.display_equity(), balance.display_free_margin());
        let (used_margin, margin_level_pct) = derive_margin(equity, free_margin);
        Ok(AccountSnapshot {
            account_id: self.account_id.clone(),
            kind: self.kind,
            currency: self.session.account_currency.clone(),
            balance: balance.display_balance(),
            equity,
            free_margin,
            used_margin,
            margin_level_pct,
            server_version: self.session.version.clone(),
            captured_at: now_millis(),
        })
    }

    async fn symbols(&self) -> Result<Vec<String>> {
        Ok(self.instruments.iter().map(|i| i.symbol.clone()).collect())
    }

    async fn catalog(&self) -> Result<Vec<SymbolInfo>> {
        Ok(self
            .session
            .symbols
            .iter()
            .zip(&self.instruments)
            .map(|(symbol, instrument)| SymbolInfo {
                symbol: symbol.symbol_name.clone(),
                description: symbol
                    .description
                    .as_deref()
                    .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|d| !d.is_empty()),
                asset_class: None,
                category: None,
                base_currency: instrument.base_currency.clone(),
                quote_currency: instrument.quote_currency.clone(),
                enabled: instrument.enabled,
            })
            .collect())
    }

    async fn instrument(&self, symbol: &str) -> Result<Instrument> {
        let base = self.instrument_by_name(symbol)?;
        if let Some(id) = base.symbol_id
            && !self.known_digits().contains_key(&id)
        {
            // One quote read teaches the decimals. A failed read is an error, not a shrug:
            // the engine caches what this returns, and a wrong precision must not stick for
            // the whole session. The caller can simply try again.
            self.quotes(std::slice::from_ref(&base.symbol)).await?;
        }
        Ok(self.with_precision(base))
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
        let mut quotes = Vec::with_capacity(response.prices.len());
        for p in &response.prices {
            let Some(symbol_id) = p.symbol_id else {
                continue;
            };
            let Some(instrument) = self.instrument_by_id(symbol_id) else {
                continue;
            };
            let raw_extra = |key: &str| p.extra.get(key).and_then(serde_json::Value::as_i64);
            let sample: Vec<i64> = [p.bid, p.ask, raw_extra("high"), raw_extra("low")]
                .into_iter()
                .flatten()
                .chain(raw_extra("sessionClose"))
                .collect();
            self.observe(symbol_id, &sample);
            if let (Some(bid), Some(ask)) = (p.bid, p.ask) {
                quotes.push(Quote {
                    symbol: instrument.symbol.clone(),
                    bid: pipette_price(bid),
                    ask: pipette_price(ask),
                    timestamp: p.timestamp,
                });
            }
        }
        Ok(quotes)
    }

    async fn bars(
        &self,
        symbol: &str,
        period: Period,
        from: UnixMillis,
        to: UnixMillis,
    ) -> Result<Vec<Candle>> {
        let symbol_id = self
            .instrument_by_name(symbol)?
            .symbol_id
            .ok_or_else(|| EngineError::Invalid(format!("`{symbol}` has no symbol id")))?;
        let raw =
            ctrader_mcp::workflows::backfill_trendbars(&self.client, symbol_id, period, from, to)
                .await?;
        let mut bars: Vec<Candle> = raw
            .iter()
            .filter_map(|bar| {
                let candle = Candle {
                    time: bar.timestamp?,
                    open: pipette_price(bar.open?),
                    high: pipette_price(bar.high?),
                    low: pipette_price(bar.low?),
                    close: pipette_price(bar.close?),
                    volume: bar.volume.unwrap_or(0) as f64,
                };
                (candle.time >= from && candle.time < to && candle.is_sane()).then_some(candle)
            })
            .collect();
        bars.sort_by_key(|bar| bar.time);
        bars.dedup_by_key(|bar| bar.time);
        Ok(bars)
    }

    async fn server_time(&self) -> Result<UnixMillis> {
        // Checked against a live server: the Remote tool list has no `get_server_time`.
        Err(EngineError::Broker {
            kind: BrokerErrorKind::Protocol,
            retryable: false,
            message: "the Remote server publishes no clock".to_owned(),
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
        let points = |distance: f64| price_to_pipettes(distance, PIPETTE_DIGITS).max(1);
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
        // Q-R10: an omitted leg was silently removed on older builds, so re-read and send
        // both. The prices go as display decimals: the server rejects nothing else here.
        quirks::amend_position_preserving_legs(
            &self.client,
            position.id.get(),
            stop_loss,
            take_profit,
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

    /// Raw prices captured from the live Remote server (2026-09-19): bid, ask, high, low and
    /// session close of each symbol. The server sends no `pipDigits`, every symbol is in
    /// units of 1e-5, and the decimals it really quotes show in the trailing zeros.
    #[test]
    fn digits_are_inferred_from_live_quotes() {
        let eurusd = [114_879, 114_880, 114_918, 114_549, 114_768];
        let usdjpy = [15_676_700, 15_676_800, 15_805_400, 15_587_400, 15_596_600];
        let xauusd = [
            437_817_000,
            437_857_000,
            439_959_000,
            433_437_000,
            434_171_000,
        ];
        assert_eq!(digits_from_pipettes(&eurusd), Some(5));
        assert_eq!(digits_from_pipettes(&usdjpy), Some(3));
        assert_eq!(digits_from_pipettes(&xauusd), Some(2));
        // BTCUSD really has 3 decimals, but this sample only shows 2: under-estimating is the
        // safe direction, see the module docs.
        let btcusd = [8_147_657_000, 8_149_649_000, 8_191_170_000];
        assert_eq!(digits_from_pipettes(&btcusd), Some(2));
        assert_eq!(digits_from_pipettes(&[0, 0]), None);
        assert_eq!(digits_from_pipettes(&[]), None);
        assert_eq!(digits_from_pipettes(&[100_000]), Some(0));
    }

    #[test]
    fn margin_is_derived_from_equity_and_free_margin() {
        // The live answer with one BTCUSD position open (2026-09-19, moneyDigits 2).
        let (used, level) = derive_margin(Some(9996.66), Some(9964.06));
        assert!((used.unwrap() - 32.60).abs() < 1e-6);
        assert!((level.unwrap() - 30_664.1).abs() < 1.0);
        // No position: nothing used, no level (as Local reports).
        assert_eq!(derive_margin(Some(9999.7), Some(9999.7)), (Some(0.0), None));
        assert_eq!(derive_margin(None, Some(1.0)), (None, None));
    }
    #[test]
    fn raw_prices_use_a_fixed_scale_for_every_symbol() {
        assert!((pipette_price(15_676_800) - 156.768).abs() < 1e-9);
        assert!((pipette_price(437_857_000) - 4378.57).abs() < 1e-9);
        assert!((pipette_price(114_880) - 1.1488).abs() < 1e-9);
    }
}
