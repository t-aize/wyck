//! The [`Broker`] adapter for the MCP server inside cTrader Desktop (Local).
//!
//! Local speaks floating point: display prices, ticker names instead of ids, and stop loss
//! and take profit on market orders as *pip distances* (`Q-L2`).
//!
//! # Volumes
//!
//! Checked against a live server (2026-09). `get_symbol_details` reports `minVolume`,
//! `maxVolume` and `volumeStep` **in base-asset units**, whatever the lot size: EURUSD is
//! `lotSize 100000, minVolume 1000, volumeStep 1000`, XAUUSD `100, 1, 1`, BTCUSD
//! `1, 0.01, 0.01`. Every tool that takes a volume also requires a `volumeType` next to it
//! (`lots` or `units`), and this adapter always sends `units`, so the engine's [`Volume`]
//! goes over the wire as `volume.as_units()` with no lot arithmetic in between. On the way
//! back a position's `volumeInUnits` is the canonical field per the tool description; the
//! decoder falls back to `volumeInLots * lotSize`, then to a bare `volume` read as units.
//! A live position (2026-09-19) has `id`, `symbolName`, `tradeSide` ("Buy"), `entryPrice`,
//! `stopLossPrice`, `takeProfitPrice`, `netProfit`, `label`, `volumeInUnits`. Pending orders
//! have not been read from a live Local server yet (the account
//! used for validation had none), so those field names still follow the tool descriptions.
//!
//! Local can only fetch symbol details one call at a time, so they are loaded on first use
//! and cached for the session.
//!
//! # Account kind
//!
//! `get_balance` does not say whether the account is demo or live (`accountType` is the
//! margin mode, `Hedged` or `Netted`). `get_accounts_list` has an `isLive` flag, but the active
//! account is not always listed under the number `get_balance` gives it (`Q-L15`: on a real
//! server `traderId` was neither the listed `id` nor the `login`). So the kind is read in two
//! steps: by identity when the numbers match, and otherwise from a listed account that has the
//! same broker, currency, account type and balance to the cent, provided every such account
//! agrees. See `kind_from_accounts`. Anything less certain is [`AccountKind::Unknown`], and a
//! front end must treat that as possibly live.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use ctrader_mcp::local::dto::{
    AccountSummary, AmendPositionParams, BalanceResponse, GetAccountsListResponse,
    PlaceMarketOrderParams, SymbolDetails, VolumeType,
};
use ctrader_mcp::time::local_iso8601_to_epoch_millis;
use ctrader_mcp::{ConnectionConfig, LocalClient};
use serde_json::Value;

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
/// Volume rules come from the broker when the details carry all of `lotSize`, `minVolume`
/// and `volumeStep`; otherwise from the user's per-symbol rules, otherwise from the global
/// assumption. Fails closed when the resulting minimum or step is not positive.
fn instrument_from_details(
    symbol: &str,
    details: &SymbolDetails,
    assumed: &AssumedSpecs,
) -> Result<Instrument> {
    let positive = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0);
    let (volume, source) = match (
        positive(details.lot_size),
        positive(details.min_volume),
        positive(details.volume_step),
    ) {
        (Some(lot_size), Some(min), Some(step)) => (
            VolumeSpecs {
                lot_size,
                min: Volume::from_units_f64(min),
                step: Volume::from_units_f64(step),
                max: positive(details.extra.get("maxVolume").and_then(Value::as_f64))
                    .map(Volume::from_units_f64),
            },
            SpecsSource::Broker,
        ),
        _ => match assumed.rules_for(symbol) {
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
        },
    };
    if !volume.min.is_positive() || !volume.step.is_positive() {
        return Err(EngineError::Invalid(format!(
            "cannot model the volume of {symbol} on Local: the minimum volume or the step is below 0.01 unit"
        )));
    }
    let digits = details.digits.unwrap_or(5);
    let pip_size =
        positive(details.pip_size).unwrap_or_else(|| super::remote::pip_size_for_digits(digits));
    let (base, quote) = split_pair(symbol);
    Ok(Instrument {
        symbol: symbol.to_owned(),
        symbol_id: None,
        price_digits: digits,
        pip_size,
        base_currency: base,
        quote_currency: quote,
        enabled: true,
        volume,
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

/// The account kind of the active account, read from `get_accounts_list`. See the
/// [module docs](self).
///
/// The list is looked at in two steps, and the first that gives an answer wins:
///
/// 1. **By identity.** The balance's `traderId` is one of the listed accounts' `traderId`, `id` or
///    `login`. This is the certain case.
/// 2. **By evidence.** The active account is not in the list under any of those numbers (`Q-L15`),
///    but a listed account has the same broker, the same currency, the same account type and the
///    same balance to the cent. The two answers are read a moment apart, so a balance that equal
///    on all four is the same account. The kind is taken from those listed accounts only if there
///    is at least one, every one of them says `isLive`, and they all agree.
///
/// Anything short of that is [`AccountKind::Unknown`], which a front end treats as possibly live:
/// a wrong "demo" is the one mistake this function must never make.
fn kind_from_accounts(list: &GetAccountsListResponse, balance: &BalanceResponse) -> AccountKind {
    kind_by_identity(list, balance.trader_id).unwrap_or_else(|| kind_by_evidence(list, balance))
}

/// The `isLive` flag of a listed account, as a kind.
fn listed_kind(account: &AccountSummary) -> Option<AccountKind> {
    match account.extra.get("isLive").and_then(Value::as_bool) {
        Some(true) => Some(AccountKind::Live),
        Some(false) => Some(AccountKind::Demo),
        None => None,
    }
}

/// The kind of the listed account that `trader_id` names, if there is one.
fn kind_by_identity(list: &GetAccountsListResponse, trader_id: Option<i64>) -> Option<AccountKind> {
    let trader_id = trader_id?;
    let matches = |account: &AccountSummary| {
        let field = |key: &str| account.extra.get(key).and_then(Value::as_i64);
        account.trader_id == Some(trader_id)
            || field("id") == Some(trader_id)
            || field("login") == Some(trader_id)
    };
    list.accounts
        .iter()
        .find(|a| matches(a))
        .and_then(listed_kind)
}

/// Whether two optional texts are both present and equal, ignoring case.
fn same_text(a: Option<&str>, b: Option<&str>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a.trim().eq_ignore_ascii_case(b.trim()))
}

/// The kind of the listed accounts that are the active account by the evidence described in
/// [`kind_from_accounts`], if they are unambiguous.
fn kind_by_evidence(list: &GetAccountsListResponse, balance: &BalanceResponse) -> AccountKind {
    let Some(active_balance) = balance.balance else {
        return AccountKind::Unknown;
    };
    let broker = text_field(&balance.extra, &["brokerName"]);
    let same_account = |account: &&AccountSummary| {
        let listed_balance = account.extra.get("balance").and_then(Value::as_f64);
        let listed_broker = account
            .broker
            .as_deref()
            .or_else(|| text_field(&account.extra, &["brokerTitle"]));
        listed_balance.is_some_and(|b| (b - active_balance).abs() < 0.005)
            && same_text(listed_broker, broker)
            && same_text(
                text_field(&account.extra, &["currency"]),
                balance.currency.as_deref(),
            )
            && same_text(
                text_field(&account.extra, &["accountType"]),
                balance.account_type.as_deref(),
            )
    };
    let same: Vec<&AccountSummary> = list.accounts.iter().filter(same_account).collect();
    let kinds: Vec<Option<AccountKind>> = same.iter().map(|a| listed_kind(a)).collect();
    match kinds.first() {
        Some(Some(first)) if kinds.iter().all(|k| k == &Some(*first)) => {
            tracing::info!(
                kind = ?first,
                "the active account is not listed under its own id, but a listed account has the same broker, currency, type and balance: taking its kind"
            );
            *first
        }
        _ => AccountKind::Unknown,
    }
}

/// The first of `keys` present in `extra` as a string.
fn text_field<'a>(extra: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| extra.get(*k).and_then(Value::as_str))
}

/// A price under `key`, ignoring the zero some answers use for "none".
fn price_field(extra: &serde_json::Map<String, Value>, key: &str) -> Option<f64> {
    extra.get(key).and_then(Value::as_f64).filter(|p| *p > 0.0)
}

/// A position's or order's volume, see the [module docs](self).
fn decode_volume(
    extra: &serde_json::Map<String, Value>,
    bare: Option<f64>,
    lot_size: f64,
) -> Option<Volume> {
    let number = |key: &str| extra.get(key).and_then(Value::as_f64);
    let units = number("volumeInUnits")
        .or_else(|| number("volumeInLots").map(|lots| lots * lot_size))
        .or(bare)?;
    Some(Volume::from_units_f64(units)).filter(|v| v.is_positive())
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
        let kind = match client.get_accounts_list().await {
            Ok(list) => kind_from_accounts(&list, &balance),
            Err(error) => {
                tracing::debug!(%error, "could not read the account list; the account kind stays unknown");
                AccountKind::Unknown
            }
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
            let side = raw
                .trade_side
                .as_deref()
                .or_else(|| text_field(&raw.extra, &["side", "tradeType", "direction"]))
                .and_then(Side::parse);
            let (Some(id), Some(symbol), Some(side)) = (raw.id, raw.symbol_name.as_deref(), side)
            else {
                tracing::warn!(position_id = ?raw.id, "skipping a position that could not be decoded");
                continue;
            };
            let instrument = self.instrument_cached(symbol).await?;
            let Some(volume) = decode_volume(&raw.extra, raw.volume, instrument.volume.lot_size)
            else {
                tracing::warn!(position_id = ?raw.id, "skipping a position with no readable volume");
                continue;
            };
            out.push(Position {
                id: PositionId(id),
                symbol: instrument.symbol.clone(),
                side,
                volume,
                entry_price: raw.entry_price,
                stop_loss: raw
                    .stop_loss
                    .or_else(|| price_field(&raw.extra, "stopLossPrice")),
                take_profit: raw
                    .take_profit
                    .or_else(|| price_field(&raw.extra, "takeProfitPrice")),
                swap: raw.swap,
                commission: raw.commission,
                unrealized_pnl: raw
                    .extra
                    .get("unrealizedPnl")
                    .or_else(|| raw.extra.get("netProfit"))
                    .or_else(|| raw.extra.get("profit"))
                    .and_then(Value::as_f64),
                label: text_field(&raw.extra, &["label", "comment"]).map(str::to_owned),
            });
        }
        Ok(out)
    }

    async fn pending_orders(&self) -> Result<Vec<PendingOrder>> {
        let response = self.client.get_pending_orders().await?;
        let mut out = Vec::with_capacity(response.orders.len());
        for raw in &response.orders {
            let side = raw
                .trade_side
                .as_deref()
                .or_else(|| text_field(&raw.extra, &["side", "tradeType", "direction"]))
                .and_then(Side::parse);
            let (Some(id), Some(symbol), Some(side)) = (raw.id, raw.symbol_name.as_deref(), side)
            else {
                continue;
            };
            let instrument = self.instrument_cached(symbol).await?;
            let Some(volume) = decode_volume(&raw.extra, raw.volume, instrument.volume.lot_size)
            else {
                continue;
            };
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
                volume,
                price: raw.target_price.or(raw.entry_price),
                stop_loss: raw
                    .stop_loss
                    .or_else(|| price_field(&raw.extra, "stopLossPrice")),
                take_profit: raw
                    .take_profit
                    .or_else(|| price_field(&raw.extra, "takeProfitPrice")),
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
                    // A symbol Local refuses to quote (not in the Market Watch, for one) is
                    // absent from the result, not an error.
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
        parse_server_time(&Value::Object(response.extra)).ok_or_else(|| EngineError::Broker {
            kind: BrokerErrorKind::Protocol,
            retryable: false,
            message: "get_server_time returned no recognizable timestamp".to_owned(),
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
            order.volume.as_units(),
        );
        params.volume_type = VolumeType::Units;
        params.stop_loss_pips = order.stop_loss_distance.map(pips);
        params.take_profit_pips = order.take_profit_distance.map(pips);
        params.label = Some(order.label.clone());
        let response = self.client.place_market_order(params).await?;
        Ok(PlacedOrder {
            position_id: response
                .extra
                .get("positionId")
                .and_then(Value::as_i64)
                .map(PositionId),
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
                self.client
                    .close_position_partial(position.id.get(), v.as_units())
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

    fn details(value: Value) -> SymbolDetails {
        serde_json::from_value(value).unwrap()
    }

    /// `get_symbol_details` as the live Local server answered it (2026-09).
    fn live_details(name: &str) -> SymbolDetails {
        details(match name {
            "EURUSD" => json!({
                "ask": 1.1488, "assetClass": "Forex", "bid": 1.14879, "digits": 5,
                "lotSize": 100_000, "maxVolume": 10_000_000, "minVolume": 1000,
                "name": "EURUSD", "pipSize": 0.0001, "volumeStep": 1000
            }),
            "XAUUSD" => json!({
                "assetClass": "Metals", "digits": 2, "lotSize": 100, "maxVolume": 10_000,
                "minVolume": 1, "name": "XAUUSD", "pipSize": 0.01, "volumeStep": 1
            }),
            _ => json!({
                "assetClass": "Cryptocurrency", "digits": 3, "lotSize": 1, "maxVolume": 50,
                "minVolume": 0.01, "name": "BTCUSD", "pipSize": 0.1, "volumeStep": 0.01
            }),
        })
    }

    #[test]
    fn live_volume_rules_are_read_as_units() {
        let assumed = AssumedSpecs::default();
        let eur = instrument_from_details("EURUSD", &live_details("EURUSD"), &assumed).unwrap();
        assert_eq!(eur.specs_source, SpecsSource::Broker);
        assert_eq!(eur.volume.min, Volume::from_units(1_000));
        assert_eq!(eur.volume.step, Volume::from_units(1_000));
        assert_eq!(eur.volume.max, Some(Volume::from_units(10_000_000)));
        assert!((eur.volume.lot_size - 100_000.0).abs() < f64::EPSILON);
        assert_eq!(eur.base_currency.as_deref(), Some("EUR"));

        let gold = instrument_from_details("XAUUSD", &live_details("XAUUSD"), &assumed).unwrap();
        assert_eq!(gold.volume.min, Volume::from_units(1));
        assert!((gold.pip_size - 0.01).abs() < 1e-12);

        // A fraction of a coin: representable now that volumes keep hundredths of a unit.
        let btc = instrument_from_details("BTCUSD", &live_details("BTCUSD"), &assumed).unwrap();
        assert_eq!(btc.volume.min, Volume::from_cents(1));
        assert_eq!(btc.volume.step, Volume::from_cents(1));
        assert!(
            (btc.pip_size - 0.1).abs() < 1e-12,
            "the broker's pip size wins"
        );
        assert_eq!(btc.price_digits, 3);
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
    fn configured_rules_replace_the_global_guess_but_not_the_broker() {
        let mut assumed = AssumedSpecs::default();
        assumed.symbols.insert(
            "btcusd".to_owned(),
            crate::config::SymbolVolumeRules {
                lot_size: 1.0,
                min_volume: 0.01,
                volume_step: 0.01,
                max_volume: Some(50.0),
            },
        );
        let bare = details(json!({ "symbolName": "BTCUSD", "digits": 3 }));
        let i = instrument_from_details("BTCUSD", &bare, &assumed).unwrap();
        assert_eq!(i.specs_source, SpecsSource::Configured);
        assert_eq!(i.volume.min, Volume::from_cents(1));

        let i = instrument_from_details("BTCUSD", &live_details("BTCUSD"), &assumed).unwrap();
        assert_eq!(i.specs_source, SpecsSource::Broker);
    }

    #[test]
    fn a_volume_step_below_one_hundredth_fails_closed() {
        let d = details(json!({
            "lotSize": 1.0, "minVolume": 0.001, "volumeStep": 0.001, "digits": 5
        }));
        let err = instrument_from_details("EURUSD", &d, &AssumedSpecs::default()).unwrap_err();
        assert!(matches!(err, EngineError::Invalid(m) if m.contains("below 0.01")));
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

    fn accounts(value: Value) -> GetAccountsListResponse {
        serde_json::from_value(value).unwrap()
    }

    /// The listed account as the live Local server answered (2026-09, balance altered).
    fn listed_demo() -> Value {
        json!({
            "accountName": "", "accountType": "Hedged", "balance": 10004.03,
            "brokerTitle": "Spotware", "currency": "USD", "id": 48_333_320,
            "isLive": false, "isOnline": false, "login": 5_884_727
        })
    }

    /// The active account as `get_balance` answered on the same server, at the same moment.
    fn active(balance: f64) -> BalanceResponse {
        serde_json::from_value(json!({
            "accountName": null, "accountType": "Hedged", "balance": balance,
            "brokerName": "Spotware", "connectionState": "Authenticated",
            "depositAsset": "USD", "equity": balance, "freeMargin": balance,
            "leverage": 100, "margin": 0, "marginLevel": null, "traderId": 3_382_746
        }))
        .unwrap()
    }

    #[test]
    fn an_account_named_by_the_balance_takes_its_kind_from_the_list() {
        let list = accounts(json!({ "accounts": [listed_demo()] }));
        assert_eq!(
            kind_by_identity(&list, Some(48_333_320)),
            Some(AccountKind::Demo)
        );
        assert_eq!(
            kind_by_identity(&list, Some(5_884_727)),
            Some(AccountKind::Demo)
        );
        assert_eq!(kind_by_identity(&list, Some(3_382_746)), None);
        assert_eq!(kind_by_identity(&list, None), None);

        let live = accounts(json!({ "accounts": [{ "id": 1, "isLive": true }] }));
        assert_eq!(kind_by_identity(&live, Some(1)), Some(AccountKind::Live));
    }

    #[test]
    fn the_real_servers_answers_are_recognized_as_one_demo_account() {
        // traderId 3382746 is neither the listed id nor the login, yet broker, currency, type and
        // balance to the cent all agree.
        let list = accounts(json!({ "accounts": [listed_demo()] }));
        assert_eq!(
            kind_from_accounts(&list, &active(10004.03)),
            AccountKind::Demo
        );
    }

    #[test]
    fn a_balance_that_differs_is_not_taken_for_the_same_account() {
        let list = accounts(json!({ "accounts": [listed_demo()] }));
        assert_eq!(
            kind_from_accounts(&list, &active(10004.04)),
            AccountKind::Unknown,
            "a cent off is another account, or a moved one: never assume demo"
        );
    }

    #[test]
    fn another_broker_currency_or_type_is_not_the_same_account() {
        for (field, value) in [
            ("brokerTitle", json!("Someone Else")),
            ("currency", json!("EUR")),
            ("accountType", json!("Netted")),
        ] {
            let mut listed = listed_demo();
            listed[field] = value;
            let list = accounts(json!({ "accounts": [listed] }));
            assert_eq!(
                kind_from_accounts(&list, &active(10004.03)),
                AccountKind::Unknown,
                "{field}"
            );
        }
    }

    #[test]
    fn a_listed_account_that_does_not_say_whether_it_is_live_proves_nothing() {
        let mut listed = listed_demo();
        listed.as_object_mut().unwrap().remove("isLive");
        let list = accounts(json!({ "accounts": [listed] }));
        assert_eq!(
            kind_from_accounts(&list, &active(10004.03)),
            AccountKind::Unknown
        );
    }

    #[test]
    fn a_demo_and_a_live_account_with_the_same_figures_are_ambiguous() {
        let mut live = listed_demo();
        live["isLive"] = json!(true);
        live["id"] = json!(1);
        let list = accounts(json!({ "accounts": [listed_demo(), live] }));
        assert_eq!(
            kind_from_accounts(&list, &active(10004.03)),
            AccountKind::Unknown
        );
    }

    #[test]
    fn a_live_account_that_matches_is_reported_live() {
        let mut live = listed_demo();
        live["isLive"] = json!(true);
        let list = accounts(json!({ "accounts": [live] }));
        assert_eq!(
            kind_from_accounts(&list, &active(10004.03)),
            AccountKind::Live
        );
    }

    #[test]
    fn identity_wins_over_the_evidence() {
        // The balance names a live account by id, while a demo account happens to share the figures.
        let mut named_live = listed_demo();
        named_live["isLive"] = json!(true);
        named_live["id"] = json!(3_382_746);
        named_live["balance"] = json!(1.0);
        let list = accounts(json!({ "accounts": [listed_demo(), named_live] }));
        assert_eq!(
            kind_from_accounts(&list, &active(10004.03)),
            AccountKind::Live
        );
    }

    #[test]
    fn an_empty_list_or_a_missing_balance_leaves_the_kind_unknown() {
        assert_eq!(
            kind_from_accounts(&accounts(json!({ "accounts": [] })), &active(1.0)),
            AccountKind::Unknown
        );
        let list = accounts(json!({ "accounts": [listed_demo()] }));
        let no_balance: BalanceResponse = serde_json::from_value(json!({
            "accountType": "Hedged", "brokerName": "Spotware", "depositAsset": "USD"
        }))
        .unwrap();
        assert_eq!(kind_from_accounts(&list, &no_balance), AccountKind::Unknown);
    }

    #[test]
    fn live_local_position_prices_are_read_and_zero_means_none() {
        let map = |v: Value| v.as_object().unwrap().clone();
        let live = map(json!({"stopLossPrice": 80896.64, "takeProfitPrice": 0.0}));
        assert_eq!(price_field(&live, "stopLossPrice"), Some(80896.64));
        assert_eq!(price_field(&live, "takeProfitPrice"), None);
        assert_eq!(price_field(&live, "missing"), None);
    }
    #[test]
    fn position_volume_prefers_units_then_lots_then_the_bare_field() {
        let map = |v: Value| v.as_object().unwrap().clone();
        let lot = 100_000.0;
        assert_eq!(
            decode_volume(
                &map(json!({"volumeInUnits": 2000.0, "volumeInLots": 0.02})),
                None,
                lot
            ),
            Some(Volume::from_units(2_000))
        );
        assert_eq!(
            decode_volume(&map(json!({"volumeInLots": 0.03})), None, lot),
            Some(Volume::from_units(3_000))
        );
        assert_eq!(
            decode_volume(&map(json!({})), Some(1_000.0), lot),
            Some(Volume::from_units(1_000))
        );
        assert_eq!(decode_volume(&map(json!({})), None, lot), None);
        assert_eq!(decode_volume(&map(json!({})), Some(0.0), lot), None);
    }
}
