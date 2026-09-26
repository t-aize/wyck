//! What the tables of the account panel show: every row of every tab worked out from the account
//! and the alerts, as cells with a text, a tone and a value to sort by, then filtered, sorted and
//! totalled as the user asked.
//!
//! Rows are built with a cell for every column the table has, so showing or moving a column never
//! asks the account again.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde_json::Value;
use wyck_openapi::account::{OrderType, Position, money};

use super::columns::{AlertCol, DealCol, ExposureCol, OrderCol, PositionCol, Sort, TablePrefs};
use super::prefs::{HistoryRange, PanelPrefs, ProfitUnit, Tab};
use super::stats::{self, HistoryStats};
use crate::app::alerts::Alerts;
use crate::app::chart::now_ms;
use crate::app::chart::zone::Zone;
use crate::app::trading::account::{Account, Busy};
use crate::app::trading::book::is_buy;
use crate::app::trading::math::{self, Contract, format_money};
use crate::app::trading::ticket::prefs::Slot;

/// The color a cell is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Muted,
    Up,
    Down,
    Accent,
}

impl Tone {
    /// The tone of an amount: gains up, losses down.
    pub fn of(value: f64) -> Self {
        if value > 0.0 {
            Self::Up
        } else if value < 0.0 {
            Self::Down
        } else {
            Self::Plain
        }
    }
}

/// What a cell is sorted by.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    None,
    Num(f64),
    Text(String),
}

/// How the sum of a column is written in the totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Lots,
    Money,
    Percent,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
    pub key: Key,
    /// What the column adds up, when it does.
    pub sum: Option<f64>,
    pub unit: Unit,
}

impl Cell {
    fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            key: Key::Text(text.to_lowercase()),
            text,
            tone: Tone::Plain,
            sum: None,
            unit: Unit::None,
        }
    }

    fn empty() -> Self {
        Self {
            text: String::new(),
            tone: Tone::Muted,
            key: Key::None,
            sum: None,
            unit: Unit::None,
        }
    }

    fn dash() -> Self {
        Self {
            text: "-".to_owned(),
            ..Self::empty()
        }
    }

    fn num(text: impl Into<String>, value: f64) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Plain,
            key: Key::Num(value),
            sum: None,
            unit: Unit::None,
        }
    }

    /// A number that may be missing.
    fn opt(value: Option<f64>, text: impl FnOnce(f64) -> String) -> Self {
        value.map_or_else(Self::dash, |v| Self::num(text(v), v))
    }

    fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    fn adds(mut self, value: f64, unit: Unit) -> Self {
        self.sum = Some(value);
        self.unit = unit;
        self
    }
}

/// What can be done to a position from its row.
#[derive(Debug, Clone)]
pub struct PositionRow {
    pub id: i64,
    /// Half of the volume, stepped, when that is a volume the broker takes.
    pub half: Option<i64>,
    pub entry: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub trailing: bool,
    pub busy: bool,
    /// `Buy 0.1 EURUSD`, for what a confirmation says.
    pub describe: String,
}

#[derive(Debug, Clone)]
pub enum RowKind {
    Position(PositionRow),
    Order {
        id: i64,
        busy: bool,
        describe: String,
    },
    Deal,
    Alert {
        id: u64,
        active: bool,
    },
    Exposure,
}

#[derive(Debug, Clone)]
pub struct Row {
    /// A key that stays the same for the same row, for element ids.
    pub key: String,
    pub symbol: Option<i64>,
    /// What the row gains or loses, for its tint.
    pub profit: Option<f64>,
    pub kind: RowKind,
    /// One cell for each column the table can have, in the order of the column type.
    pub cells: Vec<Cell>,
}

/// A column that shows.
#[derive(Debug, Clone)]
pub struct ColView {
    /// Its place among all the columns of the table.
    pub index: usize,
    /// Its place among the columns the user arranged.
    pub slot: usize,
    pub label: &'static str,
    pub width: f32,
    pub right: bool,
    /// `Some(descending)` when the table is sorted by it.
    pub sorted: Option<bool>,
}

/// A table ready to draw.
#[derive(Debug, Clone)]
pub struct Table {
    pub tab: Tab,
    pub columns: Vec<ColView>,
    pub rows: Vec<Row>,
    /// One text for each column that shows; empty where a column adds up to nothing.
    pub totals: Vec<String>,
    /// How many rows there are before the filters.
    pub unfiltered: usize,
    pub stats: Option<HistoryStats>,
}

/// What the tables are built from.
pub struct Ctx<'a> {
    pub account: &'a Account,
    pub alerts: &'a Alerts,
    pub prefs: &'a PanelPrefs,
    /// The symbol of the active chart.
    pub symbol: Option<i64>,
    pub query: &'a str,
}

impl Ctx<'_> {
    /// Whether a row passes the filters and the search.
    fn keeps(&self, symbol: i64, buy: Option<bool>, texts: &[&str]) -> bool {
        if self.prefs.only_symbol && self.symbol.is_some_and(|s| s != symbol) {
            return false;
        }
        if let Some(buy) = buy
            && !self.prefs.side.keeps(buy)
        {
            return false;
        }
        stats::matches(self.query, texts)
    }

    fn currency(&self) -> &str {
        &self.account.book.currency
    }

    fn time(&self, ms: i64) -> String {
        stats::format_time(Zone::Local.shift(ms), self.prefs.time_style)
    }
}

fn side_cell(buy: bool, colored: bool) -> Cell {
    let cell = Cell::text(if buy { "Buy" } else { "Sell" });
    if colored {
        cell.tone(if buy { Tone::Up } else { Tone::Down })
    } else {
        cell
    }
}

fn price_cell(contract: &Contract, price: Option<f64>) -> Cell {
    Cell::opt(price, |p| contract.format_price(p))
}

fn pips_cell(contract: &Contract, distance: Option<f64>) -> Cell {
    Cell::opt(distance, |d| format!("{:+.1}", contract.pips(d)))
}

/// The value a sort key compares: numbers before text, missing last.
fn compare(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Num(x), Key::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Key::Text(x), Key::Text(y)) => x.cmp(y),
        (Key::None, Key::None) => Ordering::Equal,
        (Key::None, _) => Ordering::Greater,
        (_, Key::None) => Ordering::Less,
        (Key::Num(_), Key::Text(_)) => Ordering::Less,
        (Key::Text(_), Key::Num(_)) => Ordering::Greater,
    }
}

/// Puts the rows in the order of a sort. A missing value stays last either way.
fn sort_rows(rows: &mut [Row], index: usize, descending: bool) {
    rows.sort_by(|a, b| {
        let (x, y) = (&a.cells[index].key, &b.cells[index].key);
        match (x, y) {
            (Key::None, _) | (_, Key::None) => compare(x, y),
            _ if descending => compare(y, x),
            _ => compare(x, y),
        }
    });
}

/// The columns that show, with which one the table is sorted by.
fn views<C: super::columns::Column>(prefs: &TablePrefs<C>) -> Vec<ColView> {
    prefs
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.shown)
        .map(|(slot, c)| ColView {
            index: C::ALL.iter().position(|x| *x == c.item).unwrap_or(0),
            slot,
            label: c.item.label(),
            width: prefs.width_at(slot),
            right: c.item.right(),
            sorted: prefs
                .sort
                .filter(|s| s.column == c.item)
                .map(|s| s.descending),
        })
        .collect()
}

/// What a column adds up to, written for its unit.
fn total_text(sum: f64, unit: Unit, currency: &str, balance: f64) -> String {
    match unit {
        Unit::None => String::new(),
        Unit::Lots => math::format_lots(sum),
        Unit::Money => format_money(sum, currency),
        Unit::Percent if balance > 0.0 => format!("{sum:+.2}%"),
        Unit::Percent => String::new(),
    }
}

/// Sorts, totals and wraps the rows of a table.
fn finish<C: super::columns::Column>(
    ctx: &Ctx,
    tab: Tab,
    prefs: &TablePrefs<C>,
    mut rows: Vec<Row>,
    unfiltered: usize,
    stats: Option<HistoryStats>,
) -> Table {
    let columns = views(prefs);
    if let Some(Sort { column, descending }) = prefs.sort
        && let Some(index) = C::ALL.iter().position(|c| *c == column)
    {
        sort_rows(&mut rows, index, descending);
    }
    let balance = ctx.account.summary().balance;
    let mut totals: Vec<String> = columns
        .iter()
        .map(|col| {
            let cells = rows.iter().map(|r| &r.cells[col.index]);
            let unit = rows.first().map_or(Unit::None, |r| r.cells[col.index].unit);
            let sums: Vec<f64> = cells.filter_map(|c| c.sum).collect();
            if sums.is_empty() {
                String::new()
            } else {
                total_text(sums.iter().sum(), unit, ctx.currency(), balance)
            }
        })
        .collect();
    // The first column says what the line is, when it adds up to nothing itself.
    if let Some(first) = totals.first_mut()
        && first.is_empty()
        && !rows.is_empty()
    {
        *first = "Total".to_owned();
    }
    Table {
        tab,
        columns,
        rows,
        totals,
        unfiltered,
        stats,
    }
}

/// The table of a tab.
pub fn build(ctx: &Ctx, tab: Tab) -> Table {
    match tab {
        Tab::Positions => positions(ctx),
        Tab::Orders => orders(ctx),
        Tab::History => history(ctx),
        Tab::Exposure => exposure(ctx),
        Tab::Alerts => alerts(ctx),
    }
}

/// The profit of a position written in the unit the user chose.
fn profit_cell(
    unit: ProfitUnit,
    profit: Option<f64>,
    pips: Option<f64>,
    balance: f64,
    currency: &str,
) -> Cell {
    let Some(profit) = profit else {
        return Cell::text("...").tone(Tone::Muted);
    };
    let text = match unit {
        ProfitUnit::Money => format_money(profit, currency),
        ProfitUnit::Pips => pips.map_or_else(|| "-".to_owned(), |p| format!("{p:+.1} pips")),
        ProfitUnit::Percent if balance > 0.0 => format!("{:+.2}%", profit / balance * 100.0),
        ProfitUnit::Percent => "-".to_owned(),
        ProfitUnit::MoneyPips => match pips {
            Some(p) => format!("{} ({p:+.1} pips)", format_money(profit, currency)),
            None => format_money(profit, currency),
        },
    };
    Cell::num(text, profit)
        .tone(Tone::of(profit))
        .adds(profit, Unit::Money)
}

fn positions(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let currency = ctx.currency();
    let digits = account.book.money_digits();
    let balance = account.summary().balance;
    let now = now_ms();
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    for position in account.book.positions.values() {
        unfiltered += 1;
        let id = position.position_id;
        let symbol = position.trade_data.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = is_buy(position.trade_data.trade_side);
        let name = account.book.name(symbol);
        let comment = position
            .trade_data
            .comment
            .clone()
            .or_else(|| position.trade_data.label.clone())
            .unwrap_or_default();
        let id_text = id.to_string();
        if !ctx.keeps(
            symbol,
            Some(buy),
            &[&name, if buy { "buy" } else { "sell" }, &id_text, &comment],
        ) {
            continue;
        }
        let (bid, ask) = account.quote(symbol);
        let market = if buy { bid } else { ask };
        // The direction a gain goes: up for a buy, down for a sell.
        let sign = if buy { 1.0 } else { -1.0 };
        let entry = position.price;
        let move_ = entry.zip(market).map(|(e, m)| (m - e) * sign);
        let pips = move_.map(|d| contract.pips(d));
        let profit = account.net_profit(id);
        let units = position.trade_data.units();
        let risk = entry
            .zip(position.stop_loss)
            .zip(account.rate(symbol))
            .map(|((e, s), rate)| (e - s).abs() * units * rate);
        let lots = contract.lots_of_volume(position.trade_data.volume);
        let step = contract.step_volume.max(1);
        let half = position.trade_data.volume / 2 / step * step;
        let half = (half >= contract.min_volume && half > 0 && half < position.trade_data.volume)
            .then_some(half);
        let describe = format!(
            "{} {} {name}",
            if buy { "Buy" } else { "Sell" },
            math::format_lots(lots)
        );
        let cells = PositionCol::ALL
            .iter()
            .map(|col| match col {
                PositionCol::Symbol => Cell::text(name.clone()),
                PositionCol::Side => side_cell(buy, prefs.color_side),
                PositionCol::Lots => {
                    Cell::num(math::format_lots(lots), lots).adds(lots, Unit::Lots)
                }
                PositionCol::Entry => price_cell(&contract, entry),
                PositionCol::Price => price_cell(&contract, market),
                PositionCol::StopLoss => {
                    price_cell(&contract, position.stop_loss).tone(Tone::Muted)
                }
                PositionCol::TakeProfit => {
                    price_cell(&contract, position.take_profit).tone(Tone::Muted)
                }
                PositionCol::SlPips => pips_cell(
                    &contract,
                    position.stop_loss.zip(market).map(|(s, m)| (m - s) * sign),
                ),
                PositionCol::TpPips => pips_cell(
                    &contract,
                    position
                        .take_profit
                        .zip(market)
                        .map(|(t, m)| (t - m) * sign),
                ),
                PositionCol::Pips => match pips {
                    Some(p) => Cell::num(format!("{p:+.1}"), p).tone(Tone::of(p)),
                    None => Cell::dash(),
                },
                PositionCol::Swap => Cell::opt(position.swap.map(|s| money(s, digits)), |s| {
                    format_money(s, "")
                })
                .tone(Tone::Muted),
                PositionCol::Commission => {
                    let value = position.commission.map(|c| money(c, digits));
                    let cell = Cell::opt(value, |c| format_money(c, "")).tone(Tone::Muted);
                    match value {
                        Some(v) => cell.adds(v, Unit::Money),
                        None => cell,
                    }
                }
                PositionCol::Margin => {
                    let value = position.used_margin.map(|m| money(m, digits));
                    let cell = Cell::opt(value, |m| format_money(m, ""));
                    match value {
                        Some(v) => cell.adds(v, Unit::Money),
                        None => cell,
                    }
                }
                PositionCol::Risk => {
                    let cell = Cell::opt(risk, |r| format_money(r, ""));
                    match risk {
                        Some(r) => cell.adds(r, Unit::Money),
                        None => cell,
                    }
                }
                PositionCol::Profit => {
                    profit_cell(prefs.profit_unit, profit, pips, balance, currency)
                }
                PositionCol::ProfitPercent => match profit {
                    Some(p) if balance > 0.0 => {
                        let share = p / balance * 100.0;
                        Cell::num(format!("{share:+.2}%"), share)
                            .tone(Tone::of(p))
                            .adds(share, Unit::Percent)
                    }
                    _ => Cell::dash(),
                },
                PositionCol::RMultiple => match profit.zip(risk).filter(|(_, r)| *r > 0.0) {
                    Some((p, r)) => Cell::num(format!("{:+.2}R", p / r), p / r).tone(Tone::of(p)),
                    None => Cell::dash(),
                },
                PositionCol::Opened => position
                    .trade_data
                    .open_timestamp
                    .map_or_else(Cell::dash, |t| Cell::num(ctx.time(t), t as f64)),
                PositionCol::Age => position
                    .trade_data
                    .open_timestamp
                    .map_or_else(Cell::dash, |t| {
                        Cell::num(stats::duration_text(now - t), (now - t) as f64)
                    }),
                PositionCol::Id => Cell::num(id_text.clone(), id as f64).tone(Tone::Muted),
                PositionCol::Comment => Cell::text(comment.clone()).tone(Tone::Muted),
            })
            .collect();
        rows.push(Row {
            key: format!("position-{id}"),
            symbol: Some(symbol),
            profit,
            kind: RowKind::Position(PositionRow {
                id,
                half,
                entry,
                stop_loss: position.stop_loss,
                take_profit: position.take_profit,
                trailing: position.trailing_stop_loss.unwrap_or(false),
                busy: account.is_busy(Busy::Closing(id)) || account.is_busy(Busy::Amending(id)),
                describe,
            }),
            cells,
        });
    }
    finish(
        ctx,
        Tab::Positions,
        &prefs.positions,
        rows,
        unfiltered,
        None,
    )
}

fn orders(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    for order in account.book.orders.values() {
        unfiltered += 1;
        let id = order.order_id;
        let symbol = order.trade_data.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = is_buy(order.trade_data.trade_side);
        let name = account.book.name(symbol);
        let kind = order.kind().map_or("order", OrderType::label);
        let kind = capitalized(kind);
        let comment = order
            .trade_data
            .comment
            .clone()
            .or_else(|| order.trade_data.label.clone())
            .unwrap_or_default();
        let id_text = id.to_string();
        if !ctx.keeps(
            symbol,
            Some(buy),
            &[
                &name,
                if buy { "buy" } else { "sell" },
                &kind,
                &id_text,
                &comment,
            ],
        ) {
            continue;
        }
        let price = order.limit_price.or(order.stop_price);
        let (bid, ask) = account.quote(symbol);
        let market = if buy { ask } else { bid };
        let lots = contract.lots_of_volume(order.trade_data.volume);
        let describe = format!(
            "{} {} {name} {}",
            if buy { "Buy" } else { "Sell" },
            math::format_lots(lots),
            kind.to_lowercase()
        );
        let cells = OrderCol::ALL
            .iter()
            .map(|col| match col {
                OrderCol::Symbol => Cell::text(name.clone()),
                OrderCol::Side => side_cell(buy, prefs.color_side),
                OrderCol::Kind => Cell::text(kind.clone()).tone(Tone::Muted),
                OrderCol::Lots => Cell::num(math::format_lots(lots), lots).adds(lots, Unit::Lots),
                OrderCol::Price => price_cell(&contract, price),
                OrderCol::Distance => match price.zip(market) {
                    Some((p, m)) => {
                        let pips = contract.pips((p - m).abs());
                        Cell::num(format!("{pips:.1} pips"), pips).tone(Tone::Muted)
                    }
                    None => Cell::dash(),
                },
                OrderCol::StopLoss => price_cell(&contract, order.stop_loss).tone(Tone::Muted),
                OrderCol::TakeProfit => price_cell(&contract, order.take_profit).tone(Tone::Muted),
                OrderCol::Expires => order.expiration_timestamp.map_or_else(
                    || Cell::text("Until cancelled").tone(Tone::Muted),
                    |t| Cell::num(ctx.time(t), t as f64),
                ),
                OrderCol::Created => order
                    .trade_data
                    .open_timestamp
                    .map_or_else(Cell::dash, |t| Cell::num(ctx.time(t), t as f64)),
                OrderCol::Id => Cell::num(id_text.clone(), id as f64).tone(Tone::Muted),
                OrderCol::Comment => Cell::text(comment.clone()).tone(Tone::Muted),
            })
            .collect();
        rows.push(Row {
            key: format!("order-{id}"),
            symbol: Some(symbol),
            profit: None,
            kind: RowKind::Order {
                id,
                busy: account.is_busy(Busy::Cancelling(id)) || account.is_busy(Busy::Amending(id)),
                describe,
            },
            cells,
        });
    }
    finish(ctx, Tab::Orders, &prefs.orders, rows, unfiltered, None)
}

/// A number the server put in the details of a closing deal, scaled by the money digits.
fn detail_money(detail: &Value, key: &str, digits: Option<u32>) -> Option<f64> {
    detail
        .get(key)
        .and_then(Value::as_i64)
        .map(|v| money(v, digits))
}

fn history(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let now = now_ms();
    let since = match prefs.history_range {
        // The start of today, in the zone the user reads.
        HistoryRange::Today => {
            let day = Zone::Local.day(now);
            day * 86_400_000 - Zone::Local.offset_ms(now)
        }
        HistoryRange::Day => now - 86_400_000,
        HistoryRange::ThreeDays => now - 3 * 86_400_000,
        HistoryRange::Week => now - 7 * 86_400_000,
    };
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    let mut results = Vec::new();
    for deal in &account.book.deals {
        if deal.execution_timestamp < since {
            continue;
        }
        let closing = deal.close_position_detail.is_some();
        if !closing && !prefs.history_opening {
            continue;
        }
        unfiltered += 1;
        let symbol = deal.symbol_id;
        let contract = account.book.contract(symbol);
        let buy = is_buy(deal.trade_side);
        let name = account.book.name(symbol);
        let kind = if closing { "Close" } else { "Open" };
        let digits = deal.money_digits.and_then(|d| u32::try_from(d).ok());
        let detail = deal.close_position_detail.as_ref();
        let gross = detail.and_then(|d| detail_money(d, "grossProfit", digits));
        let swap = detail.and_then(|d| detail_money(d, "swap", digits));
        let commission = detail
            .and_then(|d| detail_money(d, "commission", digits))
            .or_else(|| deal.commission.map(|c| money(c, digits)));
        let net = gross.map(|g| g + swap.unwrap_or(0.0) + commission.unwrap_or(0.0));
        let balance = detail.and_then(|d| detail_money(d, "balance", digits));
        let entry = detail
            .and_then(|d| d.get("entryPrice"))
            .and_then(Value::as_f64);
        let id_text = deal.deal_id.to_string();
        if !ctx.keeps(
            symbol,
            Some(buy),
            &[
                &name,
                if buy { "buy" } else { "sell" },
                kind,
                &id_text,
                &deal.position_id.to_string(),
            ],
        ) {
            continue;
        }
        if let Some(net) = net {
            results.push(net);
        }
        // A closing deal is on the other side of the position it closes.
        let pips = entry.zip(deal.execution_price).map(|(e, p)| {
            let sign = if buy { -1.0 } else { 1.0 };
            contract.pips((p - e) * sign)
        });
        let lots = contract.lots_of_volume(deal.filled_volume);
        let cells = DealCol::ALL
            .iter()
            .map(|col| match col {
                DealCol::Time => Cell::num(
                    ctx.time(deal.execution_timestamp),
                    deal.execution_timestamp as f64,
                )
                .tone(Tone::Muted),
                DealCol::Symbol => Cell::text(name.clone()),
                DealCol::Side => side_cell(buy, prefs.color_side),
                DealCol::Kind => Cell::text(kind).tone(Tone::Muted),
                DealCol::Lots => Cell::num(math::format_lots(lots), lots).adds(lots, Unit::Lots),
                DealCol::Entry => price_cell(&contract, entry),
                DealCol::Price => price_cell(&contract, deal.execution_price),
                DealCol::Pips => match pips {
                    Some(p) => Cell::num(format!("{p:+.1}"), p).tone(Tone::of(p)),
                    None => Cell::dash(),
                },
                DealCol::Commission => {
                    let cell = Cell::opt(commission, |c| format_money(c, "")).tone(Tone::Muted);
                    match commission {
                        Some(c) => cell.adds(c, Unit::Money),
                        None => cell,
                    }
                }
                DealCol::Swap => {
                    let cell = Cell::opt(swap, |s| format_money(s, "")).tone(Tone::Muted);
                    match swap {
                        Some(s) => cell.adds(s, Unit::Money),
                        None => cell,
                    }
                }
                DealCol::Gross => {
                    let cell = Cell::opt(gross, |g| format_money(g, "")).tone(Tone::Muted);
                    match gross {
                        Some(g) => cell.adds(g, Unit::Money),
                        None => cell,
                    }
                }
                DealCol::Profit => match net {
                    Some(n) => Cell::num(format_money(n, ctx.currency()), n)
                        .tone(Tone::of(n))
                        .adds(n, Unit::Money),
                    None => Cell::dash(),
                },
                DealCol::Balance => Cell::opt(balance, |b| format_money(b, "")).tone(Tone::Muted),
                DealCol::Id => Cell::num(id_text.clone(), deal.deal_id as f64).tone(Tone::Muted),
                DealCol::Order => {
                    Cell::num(deal.order_id.to_string(), deal.order_id as f64).tone(Tone::Muted)
                }
                DealCol::Position => {
                    Cell::num(deal.position_id.to_string(), deal.position_id as f64)
                        .tone(Tone::Muted)
                }
            })
            .collect();
        rows.push(Row {
            key: format!("deal-{}", deal.deal_id),
            symbol: Some(symbol),
            profit: net,
            kind: RowKind::Deal,
            cells,
        });
    }
    // Newest first, unless the user sorts.
    let stats = (prefs.history_stats && !results.is_empty()).then(|| HistoryStats::of(&results));
    finish(ctx, Tab::History, &prefs.history, rows, unfiltered, stats)
}

/// What the account holds on one symbol.
#[derive(Default)]
struct Held {
    long: i64,
    short: i64,
    positions: usize,
    orders: usize,
    fills: Vec<(f64, i64)>,
    profit: Option<f64>,
    margin: f64,
}

fn exposure(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let digits = account.book.money_digits();
    let mut held: BTreeMap<i64, Held> = BTreeMap::new();
    for position in account.book.positions.values() {
        let entry = held.entry(position.trade_data.symbol_id).or_default();
        add_position(entry, position, account, digits);
    }
    for order in account.book.orders.values() {
        held.entry(order.trade_data.symbol_id).or_default().orders += 1;
    }
    let unfiltered = held.len();
    let mut rows = Vec::new();
    for (symbol, held) in held {
        let name = account.book.name(symbol);
        if !ctx.keeps(symbol, None, &[&name]) {
            continue;
        }
        let contract = account.book.contract(symbol);
        let long = contract.lots_of_volume(held.long);
        let short = contract.lots_of_volume(held.short);
        let net = long - short;
        let average = stats::weighted_entry(&held.fills);
        let market = {
            let (bid, ask) = account.quote(symbol);
            match (bid, ask) {
                (Some(b), Some(a)) => Some((b + a) / 2.0),
                (b, a) => b.or(a),
            }
        };
        let cells = ExposureCol::ALL
            .iter()
            .map(|col| match col {
                ExposureCol::Symbol => Cell::text(name.clone()),
                ExposureCol::Net => Cell::num(format!("{net:+.2}"), net).tone(if net == 0.0 {
                    Tone::Plain
                } else if net > 0.0 {
                    Tone::Up
                } else {
                    Tone::Down
                }),
                ExposureCol::Long => Cell::num(math::format_lots(long), long)
                    .tone(Tone::Muted)
                    .adds(long, Unit::Lots),
                ExposureCol::Short => Cell::num(math::format_lots(short), short)
                    .tone(Tone::Muted)
                    .adds(short, Unit::Lots),
                ExposureCol::Positions => {
                    Cell::num(held.positions.to_string(), held.positions as f64)
                }
                ExposureCol::Orders => {
                    Cell::num(held.orders.to_string(), held.orders as f64).tone(Tone::Muted)
                }
                ExposureCol::Entry => price_cell(&contract, average),
                ExposureCol::Price => price_cell(&contract, market),
                ExposureCol::Margin => Cell::num(format_money(held.margin, ""), held.margin)
                    .adds(held.margin, Unit::Money),
                ExposureCol::Profit => match held.profit {
                    Some(p) => Cell::num(format_money(p, ctx.currency()), p)
                        .tone(Tone::of(p))
                        .adds(p, Unit::Money),
                    None => Cell::dash(),
                },
            })
            .collect();
        rows.push(Row {
            key: format!("exposure-{symbol}"),
            symbol: Some(symbol),
            profit: held.profit,
            kind: RowKind::Exposure,
            cells,
        });
    }
    finish(ctx, Tab::Exposure, &prefs.exposure, rows, unfiltered, None)
}

fn add_position(held: &mut Held, position: &Position, account: &Account, digits: Option<u32>) {
    let volume = position.trade_data.volume;
    if is_buy(position.trade_data.trade_side) {
        held.long += volume;
    } else {
        held.short += volume;
    }
    held.positions += 1;
    if let Some(price) = position.price {
        held.fills.push((price, volume));
    }
    if let Some(profit) = account.net_profit(position.position_id) {
        *held.profit.get_or_insert(0.0) += profit;
    }
    held.margin += position.used_margin.map_or(0.0, |m| money(m, digits));
}

fn alerts(ctx: &Ctx) -> Table {
    let account = ctx.account;
    let prefs = ctx.prefs;
    let book = ctx.alerts.book();
    let mut rows = Vec::new();
    let mut unfiltered = 0;
    for alert in &book.alerts {
        unfiltered += 1;
        let symbol = alert.symbol_id;
        let contract = account.book.contract(symbol);
        let digits = ctx
            .alerts
            .digits
            .get(&symbol)
            .copied()
            .unwrap_or(contract.digits);
        let state = match (alert.active, alert.fired_at) {
            (true, _) if alert.repeat => "Watching (repeats)",
            (true, _) => "Watching",
            (false, Some(_)) => "Fired",
            (false, None) => "Paused",
        };
        if !ctx.keeps(
            symbol,
            None,
            &[
                &alert.symbol,
                alert.condition.label(),
                state,
                &alert.message,
            ],
        ) {
            continue;
        }
        let (bid, ask) = account.quote(symbol);
        let market = match (bid, ask) {
            (Some(b), Some(a)) => Some((b + a) / 2.0),
            (b, a) => b.or(a),
        };
        let cells = AlertCol::ALL
            .iter()
            .map(|col| match col {
                AlertCol::Symbol => Cell::text(alert.symbol.clone()),
                AlertCol::Condition => Cell::text(alert.condition.label()).tone(Tone::Muted),
                AlertCol::Price => {
                    Cell::num(format!("{:.*}", digits as usize, alert.price), alert.price)
                }
                AlertCol::Distance => match market {
                    Some(m) => {
                        let pips = contract.pips(alert.price - m);
                        Cell::num(format!("{pips:+.1} pips"), pips).tone(Tone::Muted)
                    }
                    None => Cell::dash(),
                },
                AlertCol::State => Cell::text(state).tone(if alert.active {
                    Tone::Accent
                } else {
                    Tone::Muted
                }),
                AlertCol::Message => Cell::text(alert.message.clone()).tone(Tone::Muted),
                AlertCol::Repeats => {
                    Cell::text(if alert.repeat { "Yes" } else { "No" }).tone(Tone::Muted)
                }
                AlertCol::Created => {
                    if alert.created_at > 0 {
                        Cell::num(ctx.time(alert.created_at), alert.created_at as f64)
                    } else {
                        Cell::dash()
                    }
                }
                AlertCol::Fired => alert
                    .fired_at
                    .map_or_else(Cell::dash, |t| Cell::num(ctx.time(t), t as f64)),
            })
            .collect();
        rows.push(Row {
            key: format!("alert-{}", alert.id),
            symbol: Some(symbol),
            profit: None,
            kind: RowKind::Alert {
                id: alert.id,
                active: alert.active,
            },
            cells,
        });
    }
    finish(ctx, Tab::Alerts, &prefs.alerts, rows, unfiltered, None)
}

/// A word with its first letter capital.
fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The table as CSV: the columns that show, then a line for each row.
pub fn to_csv(table: &Table) -> String {
    let mut lines = vec![stats::csv_line(table.columns.iter().map(|c| c.label))];
    for row in &table.rows {
        lines.push(stats::csv_line(
            table
                .columns
                .iter()
                .map(|c| row.cells[c.index].text.as_str()),
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: Option<f64>) -> Row {
        Row {
            key: String::new(),
            symbol: None,
            profit: None,
            kind: RowKind::Deal,
            cells: vec![value.map_or_else(Cell::empty, |v| Cell::num(v.to_string(), v))],
        }
    }

    fn values(rows: &[Row]) -> Vec<Option<f64>> {
        rows.iter()
            .map(|r| match r.cells[0].key {
                Key::Num(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn rows_sort_by_number_and_a_missing_value_stays_last() {
        let mut rows = vec![row(Some(2.0)), row(None), row(Some(-1.0)), row(Some(5.0))];
        sort_rows(&mut rows, 0, false);
        assert_eq!(values(&rows), [Some(-1.0), Some(2.0), Some(5.0), None]);
        sort_rows(&mut rows, 0, true);
        assert_eq!(values(&rows), [Some(5.0), Some(2.0), Some(-1.0), None]);
    }

    #[test]
    fn text_sorts_without_regard_to_case() {
        let mut rows: Vec<Row> = ["b", "A", "c"]
            .iter()
            .map(|t| Row {
                cells: vec![Cell::text(*t)],
                ..row(None)
            })
            .collect();
        sort_rows(&mut rows, 0, false);
        let texts: Vec<&str> = rows.iter().map(|r| r.cells[0].text.as_str()).collect();
        assert_eq!(texts, ["A", "b", "c"]);
    }

    #[test]
    fn a_profit_is_written_in_the_unit_chosen() {
        let money = |unit| profit_cell(unit, Some(12.5), Some(6.25), 1_000.0, "USD").text;
        assert_eq!(money(ProfitUnit::Money), "12.50 USD");
        assert_eq!(money(ProfitUnit::Pips), "+6.2 pips");
        assert_eq!(money(ProfitUnit::Percent), "+1.25%");
        assert_eq!(money(ProfitUnit::MoneyPips), "12.50 USD (+6.2 pips)");
        let waiting = profit_cell(ProfitUnit::Money, None, None, 1_000.0, "USD");
        assert_eq!(waiting.text, "...");
        assert_eq!(waiting.sum, None);
    }

    #[test]
    fn totals_are_written_for_their_unit() {
        assert_eq!(total_text(1.5, Unit::Lots, "USD", 100.0), "1.5");
        assert_eq!(total_text(-3.0, Unit::Money, "USD", 100.0), "-3.00 USD");
        assert_eq!(total_text(2.0, Unit::Percent, "USD", 100.0), "+2.00%");
        assert_eq!(total_text(2.0, Unit::Percent, "USD", 0.0), "");
        assert_eq!(total_text(2.0, Unit::None, "USD", 100.0), "");
    }

    #[test]
    fn a_table_is_written_as_csv_with_the_columns_that_show() {
        let table = Table {
            tab: Tab::History,
            columns: vec![
                ColView {
                    index: 0,
                    slot: 0,
                    label: "Symbol",
                    width: 100.0,
                    right: false,
                    sorted: None,
                },
                ColView {
                    index: 1,
                    slot: 1,
                    label: "Note, long",
                    width: 100.0,
                    right: false,
                    sorted: None,
                },
            ],
            rows: vec![Row {
                cells: vec![Cell::text("EURUSD"), Cell::text("a \"b\"")],
                ..row(None)
            }],
            totals: Vec::new(),
            unfiltered: 1,
            stats: None,
        };
        assert_eq!(
            to_csv(&table),
            "Symbol,\"Note, long\"\nEURUSD,\"a \"\"b\"\"\""
        );
    }

    #[test]
    fn tones_follow_the_sign() {
        assert_eq!(Tone::of(1.0), Tone::Up);
        assert_eq!(Tone::of(-1.0), Tone::Down);
        assert_eq!(Tone::of(0.0), Tone::Plain);
    }
}
