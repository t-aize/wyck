//! `MarketClient`: symbol lookup, live price and order book subscriptions for one account.

use serde::Serialize;

use super::quotes::SubscribeSpotsReq;
use super::symbols::{
    Asset, AssetClass, AssetClassListRes, AssetListRes, LightSymbol, Symbol, SymbolByIdReq,
    SymbolByIdRes, SymbolCategory, SymbolCategoryListRes, SymbolsForConversionReq,
    SymbolsForConversionRes, SymbolsListReq, SymbolsListRes,
};
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

/// Market data bound to one account: symbols, live prices and the order book.
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
    pub async fn symbols(&self) -> Result<Vec<LightSymbol>> {
        self.symbols_list(false).await
    }

    /// Every symbol of the account, archived ones included.
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

    /// Follows the prices of some symbols, with the server's timestamp on each event.
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

    /// Follows the order book of some symbols (`crate::openapi::Event::Depth`).
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
}
