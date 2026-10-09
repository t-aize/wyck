//! [`MarketClient`]: symbol lookup, live price and order book subscriptions, and history, all
//! bound to one account.

use serde::Serialize;

use super::bars::{Bar, GetTrendbarsReq, GetTrendbarsRes, LiveTrendbarReq, Period, decode_bars};
use super::history;
use super::quotes::SubscribeSpotsReq;
use super::symbols::{
    Asset, AssetClass, AssetClassListRes, AssetListRes, LightSymbol, Symbol, SymbolByIdReq,
    SymbolByIdRes, SymbolCategory, SymbolCategoryListRes, SymbolsForConversionReq,
    SymbolsForConversionRes, SymbolsListReq, SymbolsListRes,
};
use super::ticks::{GetTickDataReq, GetTickDataRes, QuoteType, Tick, decode_ticks};
use crate::openapi::error::Result;
use crate::openapi::transport::connection::{Client, RateClass};
use crate::openapi::transport::messages::AccountReq;
use crate::openapi::transport::wire::payload;

/// A request naming an account and some symbols: `ProtoOAUnsubscribeSpotsReq` and the request of
/// the depth subscription calls.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SymbolsReq {
    ctid_trader_account_id: i64,
    symbol_id: Vec<i64>,
}

/// Market data bound to one account: symbols, live prices, the order book, and history. See
/// [`crate::openapi::AccountClient::market`].
#[derive(Debug, Clone)]
pub struct MarketClient {
    client: Client,
    account_id: i64,
}

impl MarketClient {
    pub(crate) fn new(client: Client, account_id: i64) -> Self {
        Self { client, account_id }
    }

    /// The account id this client is bound to.
    #[must_use]
    pub fn account_id(&self) -> i64 {
        self.account_id
    }

    /// The connection underneath, for calls that are not about market data.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    // ---- symbols ----

    /// The symbols of the account, archived ones left out.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn symbols(&self) -> Result<Vec<LightSymbol>> {
        self.symbols_list(false).await
    }

    /// Every symbol of the account, archived ones included.
    ///
    /// # Errors
    ///
    /// See [`MarketClient::symbols`].
    pub async fn symbols_including_archived(&self) -> Result<Vec<LightSymbol>> {
        self.symbols_list(true).await
    }

    async fn symbols_list(&self, include_archived: bool) -> Result<Vec<LightSymbol>> {
        let response: SymbolsListRes = self
            .client
            .call(
                payload::SYMBOLS_LIST_REQ,
                payload::SYMBOLS_LIST_RES,
                &SymbolsListReq {
                    ctid_trader_account_id: self.account_id,
                    include_archived_symbols: include_archived.then_some(true),
                },
                RateClass::Standard,
                "the symbol list",
            )
            .await?;
        Ok(response.symbol)
    }

    /// The details of some symbols: decimals, pip position, volume rules.
    ///
    /// # Errors
    ///
    /// `SYMBOL_NOT_FOUND` for an unknown id.
    pub async fn symbol_details(&self, symbol_ids: &[i64]) -> Result<Vec<Symbol>> {
        let response: SymbolByIdRes = self
            .client
            .call(
                payload::SYMBOL_BY_ID_REQ,
                payload::SYMBOL_BY_ID_RES,
                &SymbolByIdReq {
                    ctid_trader_account_id: self.account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "the symbol details",
            )
            .await?;
        Ok(response.symbol)
    }

    /// The chain of symbols that converts `first_asset_id` into `last_asset_id` when no symbol
    /// quotes them directly (for example EUR/USD, USD/JPY to convert EUR into JPY).
    ///
    /// # Errors
    ///
    /// A server error when no conversion chain exists between the two assets.
    pub async fn symbols_for_conversion(
        &self,
        first_asset_id: i64,
        last_asset_id: i64,
    ) -> Result<Vec<LightSymbol>> {
        let response: SymbolsForConversionRes = self
            .client
            .call(
                payload::SYMBOLS_FOR_CONVERSION_REQ,
                payload::SYMBOLS_FOR_CONVERSION_RES,
                &SymbolsForConversionReq {
                    ctid_trader_account_id: self.account_id,
                    first_asset_id,
                    last_asset_id,
                },
                RateClass::Standard,
                "the conversion chain",
            )
            .await?;
        Ok(response.symbol)
    }

    // ---- reference catalogs ----

    /// The assets (currencies and other units) of the broker.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn assets(&self) -> Result<Vec<Asset>> {
        let response: AssetListRes = self
            .client
            .call(
                payload::ASSET_LIST_REQ,
                payload::ASSET_LIST_RES,
                &AccountReq {
                    ctid_trader_account_id: self.account_id,
                },
                RateClass::Standard,
                "the asset list",
            )
            .await?;
        Ok(response.asset)
    }

    /// The asset classes (forex, indices, metals, ...).
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn asset_classes(&self) -> Result<Vec<AssetClass>> {
        let response: AssetClassListRes = self
            .client
            .call(
                payload::ASSET_CLASS_LIST_REQ,
                payload::ASSET_CLASS_LIST_RES,
                &AccountReq {
                    ctid_trader_account_id: self.account_id,
                },
                RateClass::Standard,
                "the asset class list",
            )
            .await?;
        Ok(response.asset_class)
    }

    /// The symbol categories (major pairs, cryptos, ...), each pointing to an asset class.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn symbol_categories(&self) -> Result<Vec<SymbolCategory>> {
        let response: SymbolCategoryListRes = self
            .client
            .call(
                payload::SYMBOL_CATEGORY_REQ,
                payload::SYMBOL_CATEGORY_RES,
                &AccountReq {
                    ctid_trader_account_id: self.account_id,
                },
                RateClass::Standard,
                "the symbol category list",
            )
            .await?;
        Ok(response.symbol_category)
    }

    // ---- live data ----

    /// Follows the prices of some symbols, with the server's timestamp on each event. The first
    /// [`crate::openapi::Event::Spot`] of each carries the latest price even when the market is closed; then
    /// one arrives at every change of bid or ask.
    ///
    /// # Errors
    ///
    /// `ALREADY_SUBSCRIBED` or `SYMBOL_NOT_FOUND` for a bad id.
    pub async fn subscribe_spots(&self, symbol_ids: &[i64]) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::SUBSCRIBE_SPOTS_REQ,
                payload::SUBSCRIBE_SPOTS_RES,
                &SubscribeSpotsReq {
                    ctid_trader_account_id: self.account_id,
                    symbol_id: symbol_ids.to_vec(),
                    subscribe_to_spot_timestamp: Some(true),
                },
                RateClass::Standard,
                "the price subscription",
            )
            .await?;
        Ok(())
    }

    /// Stops following the prices of some symbols.
    ///
    /// # Errors
    ///
    /// `NOT_SUBSCRIBED_TO_SPOTS` when there was no subscription.
    pub async fn unsubscribe_spots(&self, symbol_ids: &[i64]) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::UNSUBSCRIBE_SPOTS_REQ,
                payload::UNSUBSCRIBE_SPOTS_RES,
                &SymbolsReq {
                    ctid_trader_account_id: self.account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "ending the price subscription",
            )
            .await?;
        Ok(())
    }

    /// Follows the live bar of a symbol at a period: the bar in progress arrives inside the
    /// [`crate::openapi::Event::Spot`] events. Needs a price subscription on the same symbol first.
    ///
    /// # Errors
    ///
    /// `NOT_SUBSCRIBED_TO_SPOTS` without the price subscription.
    pub async fn subscribe_live_bars(&self, symbol_id: i64, period: Period) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::SUBSCRIBE_LIVE_TRENDBAR_REQ,
                payload::SUBSCRIBE_LIVE_TRENDBAR_RES,
                &LiveTrendbarReq {
                    ctid_trader_account_id: self.account_id,
                    period: period.number(),
                    symbol_id,
                },
                RateClass::Standard,
                "the live bar subscription",
            )
            .await?;
        Ok(())
    }

    /// Stops following the live bar of a symbol at a period.
    ///
    /// # Errors
    ///
    /// A server error when there was no such subscription.
    pub async fn unsubscribe_live_bars(&self, symbol_id: i64, period: Period) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::UNSUBSCRIBE_LIVE_TRENDBAR_REQ,
                payload::UNSUBSCRIBE_LIVE_TRENDBAR_RES,
                &LiveTrendbarReq {
                    ctid_trader_account_id: self.account_id,
                    period: period.number(),
                    symbol_id,
                },
                RateClass::Standard,
                "ending the live bar subscription",
            )
            .await?;
        Ok(())
    }

    /// Follows the order book of some symbols ([`crate::openapi::Event::Depth`]). Not every broker offers it.
    ///
    /// # Errors
    ///
    /// A server error when the broker has no depth for the symbol.
    pub async fn subscribe_depth(&self, symbol_ids: &[i64]) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::SUBSCRIBE_DEPTH_QUOTES_REQ,
                payload::SUBSCRIBE_DEPTH_QUOTES_RES,
                &SymbolsReq {
                    ctid_trader_account_id: self.account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "the depth subscription",
            )
            .await?;
        Ok(())
    }

    /// Stops following the order book of some symbols.
    ///
    /// # Errors
    ///
    /// A server error when there was no such subscription.
    pub async fn unsubscribe_depth(&self, symbol_ids: &[i64]) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::UNSUBSCRIBE_DEPTH_QUOTES_REQ,
                payload::UNSUBSCRIBE_DEPTH_QUOTES_RES,
                &SymbolsReq {
                    ctid_trader_account_id: self.account_id,
                    symbol_id: symbol_ids.to_vec(),
                },
                RateClass::Standard,
                "ending the depth subscription",
            )
            .await?;
        Ok(())
    }

    // ---- history, one page ----

    /// One request for bars in `[from_ms, to_ms]`. Returns the bars, oldest first, and whether more
    /// exist in the range than were returned. The range is limited per period by the server (see
    /// [`MarketClient::bars`] for a whole range in pages).
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range the server refuses, and the usual account errors.
    pub async fn bars_page(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<(Vec<Bar>, bool)> {
        let response: GetTrendbarsRes = self
            .client
            .call(
                payload::GET_TRENDBARS_REQ,
                payload::GET_TRENDBARS_RES,
                &GetTrendbarsReq {
                    ctid_trader_account_id: self.account_id,
                    from_timestamp: Some(from_ms),
                    to_timestamp: Some(to_ms),
                    period: period.number(),
                    symbol_id,
                    count: None,
                },
                RateClass::Historical,
                "the bar history",
            )
            .await?;
        Ok((
            decode_bars(&response.trendbar),
            response.has_more.unwrap_or(false),
        ))
    }

    /// One request for the ticks of one side in `[from_ms, to_ms]`, which may span at most one
    /// week. Returns the ticks with absolute times, oldest first, and whether more exist in the
    /// range than were returned (the ones returned are the **newest**; see [`MarketClient::ticks`]
    /// for a whole range).
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range over a week, and the usual account errors.
    pub async fn tick_page(
        &self,
        symbol_id: i64,
        side: QuoteType,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<(Vec<Tick>, bool)> {
        let response: GetTickDataRes = self
            .client
            .call(
                payload::GET_TICK_DATA_REQ,
                payload::GET_TICK_DATA_RES,
                &GetTickDataReq {
                    ctid_trader_account_id: self.account_id,
                    symbol_id,
                    r#type: side.number(),
                    from_timestamp: Some(from_ms),
                    to_timestamp: Some(to_ms),
                },
                RateClass::Historical,
                "the tick history",
            )
            .await?;
        Ok((decode_ticks(&response.tick_data), response.has_more))
    }

    // ---- history, a whole range ----

    /// Every bar of `period` in `[from_ms, to_ms]`, paged. See [`history::fetch_bars`].
    ///
    /// # Errors
    ///
    /// See [`history::fetch_bars`].
    pub async fn bars(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<Bar>> {
        history::fetch_bars(self, symbol_id, period, from_ms, to_ms).await
    }

    /// Every tick of one side in `[from_ms, to_ms]`, paged. See [`history::fetch_ticks`].
    ///
    /// # Errors
    ///
    /// See [`history::fetch_ticks`].
    pub async fn ticks(
        &self,
        symbol_id: i64,
        side: QuoteType,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<Tick>> {
        history::fetch_ticks(self, symbol_id, side, from_ms, to_ms).await
    }
}
