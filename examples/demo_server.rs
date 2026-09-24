//! A made-up cTrader Open API server, for trying the app and working on it without a cTrader
//! account.
//!
//! It speaks the JSON protocol on a local WebSocket and plays one demo account in US dollars on a
//! dozen symbols whose prices follow a smooth made-up path: bars of every period, ticks, live
//! prices and bars, and a working order book (market, limit and stop orders, stop loss and take
//! profit, closing, amending) that fills against those prices.
//!
//! ```sh
//! cargo run --example demo_server            # listens on ws://127.0.0.1:5035
//! WYCK_DEMO_SERVER=ws://127.0.0.1:5035 cargo run
//! ```

use std::collections::{BTreeMap, HashSet};
use std::f64::consts::TAU;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use wyck::openapi::transport::wire::{Envelope, payload};

const ACCOUNT: i64 = 1;
const LEVERAGE: f64 = 100.0;
const MONEY_DIGITS: i64 = 2;
/// Raw prices are real prices times this.
const SCALE: f64 = 100_000.0;

// Execution types, order types and statuses, as the Open API numbers them.
const ACCEPTED: i64 = 2;
const FILLED: i64 = 3;
const REPLACED: i64 = 4;
const CANCELLED: i64 = 5;
const MARKET: i64 = 1;
const LIMIT: i64 = 2;
const STOP: i64 = 3;
const BUY: i64 = 1;

struct Instrument {
    id: i64,
    name: &'static str,
    description: &'static str,
    base: i64,
    quote: i64,
    category: i64,
    price: f64,
    digits: i64,
    pip: i64,
    /// Units in one lot.
    lot_units: f64,
    spread_pips: f64,
    /// How far the made-up path wanders, as a share of the price.
    swing: f64,
}

const USD: i64 = 2;

const INSTRUMENTS: [Instrument; 12] = [
    fx(1, "EURUSD", "Euro vs US Dollar", 1, USD, 1.0852, 5, 4, 0.2),
    fx(
        2,
        "GBPUSD",
        "British Pound vs US Dollar",
        3,
        USD,
        1.2718,
        5,
        4,
        0.4,
    ),
    fx(
        3,
        "USDJPY",
        "US Dollar vs Japanese Yen",
        USD,
        4,
        151.37,
        3,
        2,
        0.3,
    ),
    fx(
        4,
        "AUDUSD",
        "Australian Dollar vs US Dollar",
        5,
        USD,
        0.6612,
        5,
        4,
        0.3,
    ),
    fx(
        5,
        "USDCHF",
        "US Dollar vs Swiss Franc",
        USD,
        6,
        0.9043,
        5,
        4,
        0.5,
    ),
    fx(
        6,
        "EURGBP",
        "Euro vs British Pound",
        1,
        3,
        0.8531,
        5,
        4,
        0.6,
    ),
    Instrument {
        id: 7,
        name: "XAUUSD",
        description: "Gold vs US Dollar",
        base: 7,
        quote: USD,
        category: 2,
        price: 2348.20,
        digits: 2,
        pip: 1,
        lot_units: 100.0,
        spread_pips: 2.5,
        swing: 1.4,
    },
    Instrument {
        id: 8,
        name: "XAGUSD",
        description: "Silver vs US Dollar",
        base: 8,
        quote: USD,
        category: 2,
        price: 28.415,
        digits: 3,
        pip: 2,
        lot_units: 5_000.0,
        spread_pips: 2.0,
        swing: 2.0,
    },
    Instrument {
        id: 9,
        name: "US100",
        description: "US Tech 100 Index",
        base: 9,
        quote: USD,
        category: 3,
        price: 18_240.5,
        digits: 2,
        pip: 0,
        lot_units: 1.0,
        spread_pips: 1.2,
        swing: 1.6,
    },
    Instrument {
        id: 10,
        name: "GER40",
        description: "Germany 40 Index",
        base: 10,
        quote: 1,
        category: 3,
        price: 18_110.0,
        digits: 2,
        pip: 0,
        lot_units: 1.0,
        spread_pips: 1.0,
        swing: 1.3,
    },
    Instrument {
        id: 11,
        name: "BTCUSD",
        description: "Bitcoin vs US Dollar",
        base: 11,
        quote: USD,
        category: 4,
        price: 66_420.0,
        digits: 2,
        pip: 0,
        lot_units: 1.0,
        spread_pips: 25.0,
        swing: 3.5,
    },
    Instrument {
        id: 12,
        name: "ETHUSD",
        description: "Ethereum vs US Dollar",
        base: 12,
        quote: USD,
        category: 4,
        price: 3_180.0,
        digits: 2,
        pip: 0,
        lot_units: 1.0,
        spread_pips: 1.5,
        swing: 4.0,
    },
];

#[allow(clippy::too_many_arguments)]
const fn fx(
    id: i64,
    name: &'static str,
    description: &'static str,
    base: i64,
    quote: i64,
    price: f64,
    digits: i64,
    pip: i64,
    swing: f64,
) -> Instrument {
    Instrument {
        id,
        name,
        description,
        base,
        quote,
        category: 1,
        price,
        digits,
        pip,
        lot_units: 100_000.0,
        spread_pips: 0.8,
        swing,
    }
}

/// When a symbol trades, as seconds from Sunday 00:00 UTC: forex and metals from Sunday 22:00
/// to Friday 22:00, the indices on weekdays with a break each evening, crypto all week.
fn schedule(symbol: &Instrument) -> Vec<Value> {
    const DAY: i64 = 86_400;
    let span = |a: i64, b: i64| json!({ "startSecond": a, "endSecond": b });
    match symbol.category {
        1 | 2 => vec![span(22 * 3_600, 5 * DAY + 22 * 3_600)],
        3 => (1..=5)
            .map(|d| span(d * DAY + 7 * 3_600, d * DAY + 21 * 3_600))
            .collect(),
        _ => Vec::new(),
    }
}

fn instrument(id: i64) -> Option<&'static Instrument> {
    INSTRUMENTS.iter().find(|i| i.id == id)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// A number in -1..1 that depends only on its inputs.
fn noise(a: i64, b: i64) -> f64 {
    let mut x = (a as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x % 2_000_001) as f64 / 1_000_000.0 - 1.0
}

/// The mid price of a symbol at a time: a sum of slow and fast waves (periods chosen so they
/// never line up) with a little noise on each second, so every timeframe looks alive.
fn mid(symbol: &Instrument, time_ms: i64) -> f64 {
    let t = time_ms as f64 / 1_000.0;
    let seed = symbol.id as f64 * 1.618;
    let waves: [(f64, f64); 9] = [
        (41.0 * 86_400.0, 0.030),
        (9.7 * 86_400.0, 0.012),
        (2.3 * 86_400.0, 0.006),
        (0.61 * 86_400.0, 0.0035),
        (3.7 * 3_600.0, 0.0018),
        (1.13 * 3_600.0, 0.0010),
        (19.0 * 60.0, 0.0006),
        (4.3 * 60.0, 0.0003),
        (47.0, 0.00008),
    ];
    let mut change = 0.0;
    for (index, (period, size)) in waves.iter().enumerate() {
        let phase = seed * (index as f64 + 1.0);
        change += size * (TAU * t / period + phase).sin();
    }
    change += 0.00006 * noise(symbol.id, time_ms / 1_000);
    symbol.price * (1.0 + change * symbol.swing)
}

fn pip_size(symbol: &Instrument) -> f64 {
    10f64.powi(-(symbol.pip as i32))
}

/// A real price as the server's integer, rounded to the symbol's decimals.
fn raw(symbol: &Instrument, price: f64) -> i64 {
    let unit = 10f64.powi(5 - symbol.digits as i32);
    ((price * SCALE / unit).round() * unit) as i64
}

fn round_price(symbol: &Instrument, price: f64) -> f64 {
    raw(symbol, price) as f64 / SCALE
}

/// The bid and ask now, raw.
fn quote(symbol: &Instrument, time_ms: i64) -> (i64, i64) {
    let spread =
        pip_size(symbol) * symbol.spread_pips * (1.0 + 0.3 * noise(symbol.id, time_ms / 700).abs());
    let bid = raw(symbol, mid(symbol, time_ms) - spread / 2.0);
    let ask = raw(symbol, mid(symbol, time_ms) + spread / 2.0).max(bid + 1);
    (bid, ask)
}

/// Minutes in each period, by the Open API's period number.
fn period_minutes(period: i64) -> Option<i64> {
    Some(match period {
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 10,
        7 => 15,
        8 => 30,
        9 => 60,
        10 => 240,
        11 => 720,
        12 => 1_440,
        13 => 10_080,
        14 => 43_200,
        _ => return None,
    })
}

/// Where the bar holding `minute` opens. Weeks open on Monday; months are thirty days here.
fn bar_start(minute: i64, span: i64) -> i64 {
    if span == 10_080 {
        // 1970-01-01 was a Thursday: Mondays are four days later.
        let offset = 4 * 1_440;
        return (minute - offset).div_euclid(span) * span + offset;
    }
    minute.div_euclid(span) * span
}

/// One bar in the wire form, from `start` (minutes) lasting `span` minutes, up to `until_ms`.
fn trendbar(symbol: &Instrument, start: i64, span: i64, period: i64, until_ms: i64) -> Value {
    let from_ms = start * 60_000;
    let to_ms = ((start + span) * 60_000 - 1_000).min(until_ms).max(from_ms);
    let samples = 48.min(span.max(1) * 4);
    let mut high = f64::MIN;
    let mut low = f64::MAX;
    for i in 0..=samples {
        let at = from_ms + (to_ms - from_ms) * i / samples.max(1);
        let price = mid(symbol, at);
        high = high.max(price);
        low = low.min(price);
    }
    let open = mid(symbol, from_ms);
    let close = if to_ms >= until_ms {
        quote(symbol, until_ms).0 as f64 / SCALE
    } else {
        mid(symbol, to_ms)
    };
    let wick = pip_size(symbol) * symbol.spread_pips * 0.8;
    let (high, low) = (
        high.max(open).max(close) + wick * noise(start, 7).abs(),
        low.min(open).min(close) - wick * noise(start, 9).abs(),
    );
    let (open, high, low, close) = (
        raw(symbol, open),
        raw(symbol, high),
        raw(symbol, low),
        raw(symbol, close),
    );
    let volume =
        ((span as f64).sqrt() * (120.0 + 80.0 * noise(symbol.id * 31 + start, span))) as i64;
    json!({
        "volume": volume.max(1),
        "period": period,
        "low": low,
        "deltaOpen": open - low,
        "deltaClose": close - low,
        "deltaHigh": high - low,
        "utcTimestampInMinutes": start,
    })
}

// ---- the account ----

#[derive(Clone)]
struct Position {
    id: i64,
    symbol: i64,
    side: i64,
    volume: i64,
    price: f64,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    opened_ms: i64,
    updated_ms: i64,
}

#[derive(Clone)]
struct Order {
    id: i64,
    symbol: i64,
    side: i64,
    volume: i64,
    kind: i64,
    price: f64,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    created_ms: i64,
    updated_ms: i64,
}

struct Book {
    /// Cents.
    balance: i64,
    positions: BTreeMap<i64, Position>,
    orders: BTreeMap<i64, Order>,
    deals: Vec<Value>,
    next_id: i64,
}

impl Book {
    fn next(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }
}

/// How many dollars one unit of the quote currency is worth now.
fn quote_in_usd(symbol: &Instrument, now: i64) -> f64 {
    let price_of = |id: i64| instrument(id).map_or(1.0, |s| mid(s, now));
    match symbol.quote {
        USD => 1.0,
        4 => 1.0 / price_of(3),
        6 => 1.0 / price_of(5),
        3 => price_of(2),
        1 => price_of(1),
        _ => 1.0,
    }
}

/// The profit of a position if closed at `exit`, in cents.
fn profit_cents(position: &Position, exit: f64, now: i64) -> i64 {
    let Some(symbol) = instrument(position.symbol) else {
        return 0;
    };
    let units = position.volume as f64 / 100.0;
    let sign = if position.side == BUY { 1.0 } else { -1.0 };
    ((exit - position.price) * sign * units * quote_in_usd(symbol, now) * 100.0).round() as i64
}

fn margin_cents(symbol: &Instrument, volume: i64, now: i64) -> i64 {
    let units = volume as f64 / 100.0;
    (units * mid(symbol, now) * quote_in_usd(symbol, now) / LEVERAGE * 100.0).round() as i64
}

/// The price a position closes at now: the bid for a buy, the ask for a sell.
fn exit_price(position: &Position, now: i64) -> f64 {
    let Some(symbol) = instrument(position.symbol) else {
        return position.price;
    };
    let (bid, ask) = quote(symbol, now);
    (if position.side == BUY { bid } else { ask }) as f64 / SCALE
}

fn position_json(position: &Position, status: i64, now: i64) -> Value {
    let margin = instrument(position.symbol).map_or(0, |s| margin_cents(s, position.volume, now));
    let mut value = json!({
        "positionId": position.id,
        "tradeData": {
            "symbolId": position.symbol,
            "volume": position.volume,
            "tradeSide": position.side,
            "openTimestamp": position.opened_ms,
        },
        "positionStatus": status,
        "swap": 0,
        "price": position.price,
        "utcLastUpdateTimestamp": position.updated_ms,
        "commission": 0,
        "usedMargin": margin,
        "moneyDigits": MONEY_DIGITS,
    });
    if let Some(sl) = position.stop_loss {
        value["stopLoss"] = json!(sl);
    }
    if let Some(tp) = position.take_profit {
        value["takeProfit"] = json!(tp);
    }
    value
}

fn order_json(order: &Order, status: i64) -> Value {
    let mut value = json!({
        "orderId": order.id,
        "tradeData": {
            "symbolId": order.symbol,
            "volume": order.volume,
            "tradeSide": order.side,
            "openTimestamp": order.created_ms,
        },
        "orderType": order.kind,
        "orderStatus": status,
        "utcLastUpdateTimestamp": order.updated_ms,
    });
    match order.kind {
        LIMIT => value["limitPrice"] = json!(order.price),
        STOP => value["stopPrice"] = json!(order.price),
        _ => {}
    }
    if let Some(sl) = order.stop_loss {
        value["stopLoss"] = json!(sl);
    }
    if let Some(tp) = order.take_profit {
        value["takeProfit"] = json!(tp);
    }
    value
}

fn execution(
    kind: i64,
    position: Option<Value>,
    order: Option<Value>,
    deal: Option<Value>,
) -> Value {
    let mut value = json!({ "ctidTraderAccountId": ACCOUNT, "executionType": kind });
    if let Some(position) = position {
        value["position"] = position;
    }
    if let Some(order) = order {
        value["order"] = order;
    }
    if let Some(deal) = deal {
        value["deal"] = deal;
    }
    value
}

/// A deal of `volume` at `price`; a closing one says what it made.
#[allow(clippy::too_many_arguments)]
fn deal_json(
    id: i64,
    order: i64,
    position: &Position,
    volume: i64,
    side: i64,
    price: f64,
    closing: Option<(i64, i64)>,
    now: i64,
) -> Value {
    let mut value = json!({
        "dealId": id,
        "orderId": order,
        "positionId": position.id,
        "volume": volume,
        "filledVolume": volume,
        "symbolId": position.symbol,
        "createTimestamp": now,
        "executionTimestamp": now,
        "executionPrice": price,
        "tradeSide": side,
        "dealStatus": 2,
        "commission": 0,
        "moneyDigits": MONEY_DIGITS,
    });
    if let Some((gross, balance)) = closing {
        value["closePositionDetail"] = json!({
            "entryPrice": position.price,
            "grossProfit": gross,
            "swap": 0,
            "commission": 0,
            "balance": balance,
            "closedVolume": volume,
            "moneyDigits": MONEY_DIGITS,
        });
    }
    value
}

/// What goes back to the client: an answer to its request (with its id) or an event.
struct Out {
    payload_type: u32,
    body: Value,
    answer: bool,
}

fn answer(payload_type: u32, body: Value) -> Out {
    Out {
        payload_type,
        body,
        answer: true,
    }
}

fn error(code: &str, description: &str) -> Out {
    answer(
        payload::ERROR_RES,
        json!({ "errorCode": code, "description": description }),
    )
}

/// Fills a market order now. The events carry the request's id, as cTrader's do: the first
/// answers the request, the ones after it come as events.
fn fill_market(
    book: &mut Book,
    symbol: &Instrument,
    side: i64,
    volume: i64,
    sl: Option<f64>,
    tp: Option<f64>,
    now: i64,
) -> Vec<Out> {
    let (bid, ask) = quote(symbol, now);
    let price = (if side == BUY { ask } else { bid }) as f64 / SCALE;
    let (order_id, position_id, deal_id) = (book.next(), book.next(), book.next());
    let order = Order {
        id: order_id,
        symbol: symbol.id,
        side,
        volume,
        kind: MARKET,
        price,
        stop_loss: None,
        take_profit: None,
        created_ms: now,
        updated_ms: now,
    };
    let position = Position {
        id: position_id,
        symbol: symbol.id,
        side,
        volume,
        price,
        stop_loss: sl,
        take_profit: tp,
        opened_ms: now,
        updated_ms: now,
    };
    let deal = deal_json(deal_id, order_id, &position, volume, side, price, None, now);
    book.deals.push(deal.clone());
    book.positions.insert(position_id, position.clone());
    let mut filled_order = order_json(&order, 2);
    filled_order["executionPrice"] = json!(price);
    filled_order["executedVolume"] = json!(volume);
    filled_order["positionId"] = json!(position_id);
    vec![
        answer(
            payload::EXECUTION_EVENT,
            execution(
                ACCEPTED,
                Some(position_json(&position, 1, now)),
                Some(order_json(&order, 1)),
                None,
            ),
        ),
        answer(
            payload::EXECUTION_EVENT,
            execution(
                FILLED,
                Some(position_json(&position, 1, now)),
                Some(filled_order),
                Some(deal),
            ),
        ),
    ]
}

/// Closes `volume` of a position at the market; the whole of it when `volume` covers it.
fn close(book: &mut Book, id: i64, volume: i64, now: i64) -> Vec<Out> {
    let Some(mut position) = book.positions.get(&id).cloned() else {
        return vec![error("POSITION_NOT_FOUND", "There is no such position.")];
    };
    let volume = volume.clamp(1, position.volume);
    let price = exit_price(&position, now);
    let closing = Position {
        volume,
        ..position.clone()
    };
    let gross = profit_cents(&closing, price, now);
    book.balance += gross;
    let side = if position.side == BUY { 2 } else { BUY };
    let (order_id, deal_id) = (book.next(), book.next());
    let deal = deal_json(
        deal_id,
        order_id,
        &position,
        volume,
        side,
        price,
        Some((gross, book.balance)),
        now,
    );
    book.deals.push(deal.clone());
    position.volume -= volume;
    position.updated_ms = now;
    let status = if position.volume == 0 {
        book.positions.remove(&id);
        2
    } else {
        book.positions.insert(id, position.clone());
        1
    };
    let mut shown = position_json(&position, status, now);
    if status == 2 {
        shown["tradeData"]["volume"] = json!(0);
    }
    let order = Order {
        id: order_id,
        symbol: position.symbol,
        side,
        volume,
        kind: MARKET,
        price,
        stop_loss: None,
        take_profit: None,
        created_ms: now,
        updated_ms: now,
    };
    vec![
        answer(
            payload::EXECUTION_EVENT,
            execution(ACCEPTED, None, Some(order_json(&order, 1)), None),
        ),
        answer(
            payload::EXECUTION_EVENT,
            execution(FILLED, Some(shown), Some(order_json(&order, 2)), Some(deal)),
        ),
    ]
}

fn opt_price(body: &Value, key: &str) -> Option<f64> {
    body.get(key).and_then(Value::as_f64).filter(|p| *p > 0.0)
}

fn int(body: &Value, key: &str) -> Option<i64> {
    body.get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
}

/// Answers one request.
fn handle(
    request: &Envelope,
    book: &Mutex<Book>,
    subscriptions: &Mutex<Subscriptions>,
) -> Vec<Out> {
    let body = &request.payload;
    let now = now_ms();
    match request.payload_type {
        payload::APPLICATION_AUTH_REQ => vec![answer(payload::APPLICATION_AUTH_RES, json!({}))],
        payload::ACCOUNT_AUTH_REQ => vec![answer(
            payload::ACCOUNT_AUTH_RES,
            json!({ "ctidTraderAccountId": ACCOUNT }),
        )],
        payload::VERSION_REQ => vec![answer(payload::VERSION_RES, json!({ "version": "demo" }))],
        payload::REFRESH_TOKEN_REQ => vec![answer(
            payload::REFRESH_TOKEN_RES,
            json!({ "accessToken": "demo", "refreshToken": "demo", "tokenType": "bearer", "expiresIn": 2_592_000 }),
        )],
        payload::SYMBOLS_LIST_REQ => {
            let symbols: Vec<Value> = INSTRUMENTS
                .iter()
                .map(|s| {
                    json!({
                        "symbolId": s.id,
                        "symbolName": s.name,
                        "enabled": true,
                        "description": s.description,
                        "baseAssetId": s.base,
                        "quoteAssetId": s.quote,
                        "symbolCategoryId": s.category,
                    })
                })
                .collect();
            vec![answer(
                payload::SYMBOLS_LIST_RES,
                json!({ "symbol": symbols }),
            )]
        }
        payload::SYMBOL_BY_ID_REQ => {
            let ids: Vec<i64> = body["symbolId"]
                .as_array()
                .map(|list| list.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            let symbols: Vec<Value> = ids
                .iter()
                .filter_map(|id| instrument(*id))
                .map(|s| {
                    let lot = (s.lot_units * 100.0) as i64;
                    json!({
                        "symbolId": s.id,
                        "digits": s.digits,
                        "pipPosition": s.pip,
                        "lotSize": lot,
                        "minVolume": (lot / 100).max(1),
                        "maxVolume": lot * 1_000,
                        "stepVolume": (lot / 100).max(1),
                        "scheduleTimeZone": "UTC",
                        "schedule": schedule(s),
                        "tradingMode": if s.id == 8 { 3 } else { 0 },
                    })
                })
                .collect();
            vec![answer(
                payload::SYMBOL_BY_ID_RES,
                json!({ "symbol": symbols }),
            )]
        }
        payload::ASSET_LIST_REQ => {
            let names = [
                (1, "EUR"),
                (2, "USD"),
                (3, "GBP"),
                (4, "JPY"),
                (5, "AUD"),
                (6, "CHF"),
                (7, "XAU"),
                (8, "XAG"),
                (9, "US100"),
                (10, "GER40"),
                (11, "BTC"),
                (12, "ETH"),
            ];
            let assets: Vec<Value> = names
                .iter()
                .map(|(id, name)| json!({ "assetId": id, "name": name, "displayName": name, "digits": 2 }))
                .collect();
            vec![answer(payload::ASSET_LIST_RES, json!({ "asset": assets }))]
        }
        payload::ASSET_CLASS_LIST_REQ => vec![answer(
            payload::ASSET_CLASS_LIST_RES,
            json!({ "assetClass": [
                { "id": 1, "name": "Forex" },
                { "id": 2, "name": "Metals" },
                { "id": 3, "name": "Indices" },
                { "id": 4, "name": "Crypto Currency" },
            ]}),
        )],
        payload::SYMBOL_CATEGORY_REQ => vec![answer(
            payload::SYMBOL_CATEGORY_RES,
            json!({ "symbolCategory": [
                { "id": 1, "assetClassId": 1, "name": "Majors" },
                { "id": 2, "assetClassId": 2, "name": "Spot metals" },
                { "id": 3, "assetClassId": 3, "name": "Cash indices" },
                { "id": 4, "assetClassId": 4, "name": "Coins" },
            ]}),
        )],
        payload::SYMBOLS_FOR_CONVERSION_REQ => {
            // One symbol trading the two assets, the one way or the other.
            let (first, last) = (int(body, "firstAssetId"), int(body, "lastAssetId"));
            let chain: Vec<Value> = INSTRUMENTS
                .iter()
                .filter(|s| {
                    (Some(s.base), Some(s.quote)) == (first, last)
                        || (Some(s.quote), Some(s.base)) == (first, last)
                })
                .take(1)
                .map(|s| {
                    json!({
                        "symbolId": s.id,
                        "symbolName": s.name,
                        "baseAssetId": s.base,
                        "quoteAssetId": s.quote,
                    })
                })
                .collect();
            if chain.is_empty() {
                vec![error("SYMBOL_NOT_FOUND", "No conversion chain.")]
            } else {
                vec![answer(
                    payload::SYMBOLS_FOR_CONVERSION_RES,
                    json!({ "symbol": chain }),
                )]
            }
        }
        payload::GET_TRENDBARS_REQ => {
            let (Some(symbol), Some(period)) = (
                int(body, "symbolId").and_then(instrument),
                int(body, "period"),
            ) else {
                return vec![error("INVALID_REQUEST", "Unknown symbol.")];
            };
            let Some(span) = period_minutes(period) else {
                return vec![error("INVALID_REQUEST", "Unknown period.")];
            };
            let to = int(body, "toTimestamp").unwrap_or(now).min(now);
            let from = int(body, "fromTimestamp").unwrap_or(to - span * 60_000 * 1_000);
            let count = int(body, "count").unwrap_or(5_000).clamp(1, 5_000);
            let mut start = bar_start(to / 60_000, span);
            let mut bars = Vec::new();
            while start * 60_000 >= from && (bars.len() as i64) < count {
                if start * 60_000 < to {
                    bars.push(trendbar(symbol, start, span, period, now));
                }
                start = bar_start(start - 1, span);
            }
            bars.reverse();
            vec![answer(
                payload::GET_TRENDBARS_RES,
                json!({ "trendbar": bars, "hasMore": false }),
            )]
        }
        payload::GET_TICK_DATA_REQ => {
            let Some(symbol) = int(body, "symbolId").and_then(instrument) else {
                return vec![error("INVALID_REQUEST", "Unknown symbol.")];
            };
            let ask_side = int(body, "type") == Some(2);
            let to = int(body, "toTimestamp").unwrap_or(now).min(now);
            let from = int(body, "fromTimestamp").unwrap_or(to - 3_600_000);
            // A tick every 1.5 seconds, newest first, each after the first told as a difference.
            let step = 1_500;
            let mut ticks = Vec::new();
            let mut previous: Option<(i64, i64)> = None;
            let mut at = to - to.rem_euclid(step);
            while at >= from && ticks.len() < 5_000 {
                let (bid, ask) = quote(symbol, at);
                let price = if ask_side { ask } else { bid };
                ticks.push(match previous {
                    None => json!({ "timestamp": at, "tick": price }),
                    Some((t, p)) => json!({ "timestamp": at - t, "tick": price - p }),
                });
                previous = Some((at, price));
                at -= step;
            }
            vec![answer(
                payload::GET_TICK_DATA_RES,
                json!({ "tickData": ticks, "hasMore": at >= from }),
            )]
        }
        payload::SUBSCRIBE_SPOTS_REQ => {
            let mut subs = subscriptions.lock().unwrap();
            for id in body["symbolId"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
            {
                subs.spots.insert(id);
            }
            vec![answer(payload::SUBSCRIBE_SPOTS_RES, json!({}))]
        }
        payload::UNSUBSCRIBE_SPOTS_REQ => {
            let mut subs = subscriptions.lock().unwrap();
            for id in body["symbolId"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
            {
                subs.spots.remove(&id);
                subs.bars.retain(|(symbol, _)| *symbol != id);
            }
            vec![answer(payload::UNSUBSCRIBE_SPOTS_RES, json!({}))]
        }
        payload::SUBSCRIBE_LIVE_TRENDBAR_REQ => {
            if let (Some(symbol), Some(period)) = (int(body, "symbolId"), int(body, "period")) {
                subscriptions.lock().unwrap().bars.insert((symbol, period));
            }
            vec![answer(payload::SUBSCRIBE_LIVE_TRENDBAR_RES, json!({}))]
        }
        payload::UNSUBSCRIBE_LIVE_TRENDBAR_REQ => {
            if let (Some(symbol), Some(period)) = (int(body, "symbolId"), int(body, "period")) {
                subscriptions.lock().unwrap().bars.remove(&(symbol, period));
            }
            vec![answer(payload::UNSUBSCRIBE_LIVE_TRENDBAR_RES, json!({}))]
        }
        payload::TRADER_REQ => {
            let book = book.lock().unwrap();
            vec![answer(
                payload::TRADER_RES,
                json!({ "trader": {
                    "ctidTraderAccountId": ACCOUNT,
                    "balance": book.balance,
                    "depositAssetId": USD,
                    "accessRights": 0,
                    "leverageInCents": (LEVERAGE * 100.0) as i64,
                    "maxLeverage": (LEVERAGE * 100.0) as i64,
                    "traderLogin": 5_000_123,
                    "accountType": 0,
                    "brokerName": "Wyck demo",
                    "moneyDigits": MONEY_DIGITS,
                }}),
            )]
        }
        payload::RECONCILE_REQ => {
            let book = book.lock().unwrap();
            let positions: Vec<Value> = book
                .positions
                .values()
                .map(|p| position_json(p, 1, now))
                .collect();
            let orders: Vec<Value> = book.orders.values().map(|o| order_json(o, 1)).collect();
            vec![answer(
                payload::RECONCILE_RES,
                json!({ "position": positions, "order": orders }),
            )]
        }
        payload::DEAL_LIST_REQ => {
            let book = book.lock().unwrap();
            let from = int(body, "fromTimestamp").unwrap_or(0);
            let deals: Vec<Value> = book
                .deals
                .iter()
                .filter(|d| d["executionTimestamp"].as_i64().unwrap_or(0) >= from)
                .cloned()
                .collect();
            vec![answer(
                payload::DEAL_LIST_RES,
                json!({ "deal": deals, "hasMore": false }),
            )]
        }
        payload::ORDER_LIST_REQ => vec![answer(
            payload::ORDER_LIST_RES,
            json!({ "order": [], "hasMore": false }),
        )],
        payload::CASH_FLOW_HISTORY_LIST_REQ => vec![answer(
            payload::CASH_FLOW_HISTORY_LIST_RES,
            json!({ "depositWithdraw": [] }),
        )],
        payload::MARGIN_CALL_LIST_REQ => vec![answer(
            payload::MARGIN_CALL_LIST_RES,
            json!({ "marginCall": [] }),
        )],
        payload::GET_POSITION_UNREALIZED_PNL_REQ => {
            let book = book.lock().unwrap();
            let list: Vec<Value> = book
                .positions
                .values()
                .map(|p| {
                    let profit = profit_cents(p, exit_price(p, now), now);
                    json!({ "positionId": p.id, "grossUnrealizedPnL": profit, "netUnrealizedPnL": profit })
                })
                .collect();
            vec![answer(
                payload::GET_POSITION_UNREALIZED_PNL_RES,
                json!({ "ctidTraderAccountId": ACCOUNT, "positionUnrealizedPnL": list, "moneyDigits": MONEY_DIGITS }),
            )]
        }
        payload::EXPECTED_MARGIN_REQ => {
            let Some(symbol) = int(body, "symbolId").and_then(instrument) else {
                return vec![error("INVALID_REQUEST", "Unknown symbol.")];
            };
            let margins: Vec<Value> = body["volume"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .map(|v| {
                    let m = margin_cents(symbol, v, now);
                    json!({ "volume": v, "buyMargin": m, "sellMargin": m })
                })
                .collect();
            vec![answer(
                payload::EXPECTED_MARGIN_RES,
                json!({ "margin": margins, "moneyDigits": MONEY_DIGITS }),
            )]
        }
        payload::NEW_ORDER_REQ => new_order(body, &mut book.lock().unwrap(), now),
        payload::CANCEL_ORDER_REQ => {
            let mut book = book.lock().unwrap();
            match int(body, "orderId").and_then(|id| book.orders.remove(&id)) {
                Some(order) => vec![answer(
                    payload::EXECUTION_EVENT,
                    execution(CANCELLED, None, Some(order_json(&order, 5)), None),
                )],
                None => vec![error("ORDER_NOT_FOUND", "There is no such order.")],
            }
        }
        payload::AMEND_ORDER_REQ => {
            let mut book = book.lock().unwrap();
            let Some(order) = int(body, "orderId").and_then(|id| book.orders.get_mut(&id)) else {
                return vec![error("ORDER_NOT_FOUND", "There is no such order.")];
            };
            if let Some(price) =
                opt_price(body, "limitPrice").or_else(|| opt_price(body, "stopPrice"))
            {
                order.price = price;
            }
            if let Some(volume) = int(body, "volume") {
                order.volume = volume;
            }
            order.stop_loss = opt_price(body, "stopLoss");
            order.take_profit = opt_price(body, "takeProfit");
            order.updated_ms = now;
            vec![answer(
                payload::EXECUTION_EVENT,
                execution(REPLACED, None, Some(order_json(order, 1)), None),
            )]
        }
        payload::AMEND_POSITION_SLTP_REQ => {
            let mut book = book.lock().unwrap();
            let Some(position) = int(body, "positionId").and_then(|id| book.positions.get_mut(&id))
            else {
                return vec![error("POSITION_NOT_FOUND", "There is no such position.")];
            };
            position.stop_loss = opt_price(body, "stopLoss");
            position.take_profit = opt_price(body, "takeProfit");
            position.updated_ms = now;
            let position = position.clone();
            vec![answer(
                payload::EXECUTION_EVENT,
                execution(REPLACED, Some(position_json(&position, 1, now)), None, None),
            )]
        }
        payload::CLOSE_POSITION_REQ => {
            let mut book = book.lock().unwrap();
            let (Some(id), Some(volume)) = (int(body, "positionId"), int(body, "volume")) else {
                return vec![error(
                    "INVALID_REQUEST",
                    "A position and a volume are needed.",
                )];
            };
            close(&mut book, id, volume, now)
        }
        _ => vec![error(
            "UNSUPPORTED",
            "The demo server does not know this request.",
        )],
    }
}

fn new_order(body: &Value, book: &mut Book, now: i64) -> Vec<Out> {
    let (Some(symbol), Some(kind), Some(side), Some(volume)) = (
        int(body, "symbolId").and_then(instrument),
        int(body, "orderType"),
        int(body, "tradeSide"),
        int(body, "volume"),
    ) else {
        return vec![error(
            "INVALID_REQUEST",
            "Symbol, type, side and volume are needed.",
        )];
    };
    if volume <= 0 {
        return vec![error("TRADING_BAD_VOLUME", "The volume must be positive.")];
    }
    let (bid, ask) = quote(symbol, now);
    let fill = (if side == BUY { ask } else { bid }) as f64 / SCALE;
    let sign = if side == BUY { 1.0 } else { -1.0 };
    // Relative protection is in points (a hundred thousandth of the price).
    let relative = |key: &str, towards: f64| {
        int(body, key).map(|points| round_price(symbol, fill + towards * points as f64 / SCALE))
    };
    let stop_loss = opt_price(body, "stopLoss").or_else(|| relative("relativeStopLoss", -sign));
    let take_profit =
        opt_price(body, "takeProfit").or_else(|| relative("relativeTakeProfit", sign));
    match kind {
        MARKET => fill_market(book, symbol, side, volume, stop_loss, take_profit, now),
        LIMIT | STOP => {
            let Some(price) =
                opt_price(body, "limitPrice").or_else(|| opt_price(body, "stopPrice"))
            else {
                return vec![error("INVALID_REQUEST", "A pending order needs its price.")];
            };
            let id = book.next();
            let order = Order {
                id,
                symbol: symbol.id,
                side,
                volume,
                kind,
                price,
                stop_loss,
                take_profit,
                created_ms: now,
                updated_ms: now,
            };
            book.orders.insert(id, order.clone());
            vec![answer(
                payload::EXECUTION_EVENT,
                execution(ACCEPTED, None, Some(order_json(&order, 1)), None),
            )]
        }
        _ => vec![error(
            "UNSUPPORTED",
            "The demo server takes market, limit and stop orders.",
        )],
    }
}

/// Fills the pending orders the price reached and closes the positions whose stop loss or take
/// profit it crossed. Returns the events to send.
fn match_orders(book: &mut Book, now: i64) -> Vec<Out> {
    let mut out = Vec::new();
    let due: Vec<Order> = book
        .orders
        .values()
        .filter(|o| {
            let Some(symbol) = instrument(o.symbol) else {
                return false;
            };
            let (bid, ask) = quote(symbol, now);
            let price = (if o.side == BUY { ask } else { bid }) as f64 / SCALE;
            match (o.kind, o.side == BUY) {
                (LIMIT, true) | (STOP, false) => price <= o.price,
                (LIMIT, false) | (STOP, true) => price >= o.price,
                _ => false,
            }
        })
        .cloned()
        .collect();
    for order in due {
        book.orders.remove(&order.id);
        let Some(symbol) = instrument(order.symbol) else {
            continue;
        };
        for mut reply in fill_market(
            book,
            symbol,
            order.side,
            order.volume,
            order.stop_loss,
            order.take_profit,
            now,
        ) {
            reply.answer = false;
            out.push(reply);
        }
    }
    let hit: Vec<i64> = book
        .positions
        .values()
        .filter(|p| {
            let price = exit_price(p, now);
            let buy = p.side == BUY;
            p.stop_loss
                .is_some_and(|sl| if buy { price <= sl } else { price >= sl })
                || p.take_profit
                    .is_some_and(|tp| if buy { price >= tp } else { price <= tp })
        })
        .map(|p| p.id)
        .collect();
    for id in hit {
        let volume = book.positions.get(&id).map_or(0, |p| p.volume);
        for mut reply in close(book, id, volume, now) {
            reply.answer = false;
            out.push(reply);
        }
    }
    out
}

#[derive(Default)]
struct Subscriptions {
    spots: HashSet<i64>,
    bars: HashSet<(i64, i64)>,
}

fn spot_event(symbol: &Instrument, subscriptions: &Subscriptions, now: i64) -> Value {
    let (bid, ask) = quote(symbol, now);
    let bars: Vec<Value> = subscriptions
        .bars
        .iter()
        .filter(|(id, _)| *id == symbol.id)
        .filter_map(|(_, period)| {
            let span = period_minutes(*period)?;
            Some(trendbar(
                symbol,
                bar_start(now / 60_000, span),
                span,
                *period,
                now,
            ))
        })
        .collect();
    json!({
        "ctidTraderAccountId": ACCOUNT,
        "symbolId": symbol.id,
        "bid": bid,
        "ask": ask,
        "trendbar": bars,
        "timestamp": now,
    })
}

fn text(payload_type: u32, id: Option<String>, body: Value) -> Message {
    let envelope = Envelope {
        client_msg_id: id,
        payload_type,
        payload: body,
    };
    Message::text(envelope.to_text().unwrap_or_default())
}

async fn serve(stream: TcpStream, book: Arc<Mutex<Book>>) {
    let Ok(socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    println!("a client connected");
    let (mut sink, mut source) = socket.split();
    let (out, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });
    let subscriptions = Arc::new(Mutex::new(Subscriptions::default()));
    // Prices, live bars, fills and heartbeats, for as long as the client stays.
    let ticker = {
        let (out, book, subscriptions) = (out.clone(), book.clone(), subscriptions.clone());
        tokio::spawn(async move {
            let mut beat = 0u32;
            loop {
                tokio::time::sleep(Duration::from_millis(400)).await;
                let now = now_ms();
                let events: Vec<Message> = {
                    let subs = subscriptions.lock().unwrap();
                    let mut events: Vec<Message> = INSTRUMENTS
                        .iter()
                        .filter(|s| subs.spots.contains(&s.id))
                        // Not every symbol moves on every beat, like a real feed.
                        .filter(|s| noise(s.id, now / 400) > -0.3)
                        .map(|s| text(payload::SPOT_EVENT, None, spot_event(s, &subs, now)))
                        .collect();
                    for reply in match_orders(&mut book.lock().unwrap(), now) {
                        events.push(text(reply.payload_type, None, reply.body));
                    }
                    events
                };
                for event in events {
                    if out.send(event).is_err() {
                        return;
                    }
                }
                beat += 1;
                if beat.is_multiple_of(25)
                    && out
                        .send(text(payload::HEARTBEAT_EVENT, None, json!({})))
                        .is_err()
                {
                    return;
                }
            }
        })
    };
    while let Some(frame) = source.next().await {
        let Ok(Message::Text(frame)) = frame else {
            if matches!(frame, Ok(Message::Close(_)) | Err(_)) {
                break;
            }
            continue;
        };
        let Ok(request) = Envelope::from_text(frame.as_str()) else {
            continue;
        };
        if request.payload_type == payload::HEARTBEAT_EVENT {
            continue;
        }
        for reply in handle(&request, &book, &subscriptions) {
            let id = reply
                .answer
                .then(|| request.client_msg_id.clone())
                .flatten();
            if out.send(text(reply.payload_type, id, reply.body)).is_err() {
                break;
            }
        }
    }
    println!("the client left");
    ticker.abort();
    writer.abort();
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(5035);
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("the port is free");
    println!("demo server on ws://127.0.0.1:{port}");
    println!("run the app with WYCK_DEMO_SERVER=ws://127.0.0.1:{port}");
    let now = now_ms();
    let mut book = Book {
        balance: 1_000_000,
        positions: BTreeMap::new(),
        orders: BTreeMap::new(),
        deals: Vec::new(),
        next_id: 1_000,
    };
    // Something to look at from the start: two open positions and a working order.
    let eurusd = &INSTRUMENTS[0];
    let gold = &INSTRUMENTS[6];
    let entry = mid(eurusd, now - 3_600_000);
    fill_market(
        &mut book,
        eurusd,
        BUY,
        20_000_000,
        Some(round_price(eurusd, entry - 0.0035)),
        Some(round_price(eurusd, entry + 0.0060)),
        now,
    );
    if let Some(position) = book.positions.values_mut().next() {
        position.price = round_price(eurusd, entry);
        position.opened_ms = now - 3_600_000;
    }
    fill_market(&mut book, gold, 2, 5_000, None, None, now);
    let id = book.next();
    book.orders.insert(
        id,
        Order {
            id,
            symbol: eurusd.id,
            side: BUY,
            volume: 10_000_000,
            kind: LIMIT,
            price: round_price(eurusd, mid(eurusd, now) - 0.0025),
            stop_loss: Some(round_price(eurusd, mid(eurusd, now) - 0.0050)),
            take_profit: None,
            created_ms: now,
            updated_ms: now,
        },
    );
    let book = Arc::new(Mutex::new(book));
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        tokio::spawn(serve(stream, book.clone()));
    }
}
