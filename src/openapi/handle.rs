//! A client bound to one trading account: a router to the four domain sub-clients.

use crate::openapi::account::AccountDataClient;
use crate::openapi::error::Result;
use crate::openapi::margin::MarginClient;
use crate::openapi::market::MarketClient;
use crate::openapi::trading::TradingClient;
use crate::openapi::transport::connection::{Client, RateClass};
use crate::openapi::transport::messages::AccountReq;
use crate::openapi::transport::wire::payload;

impl Client {
    /// The client bound to `account_id`.
    #[must_use]
    pub fn account(&self, account_id: i64) -> AccountClient {
        AccountClient {
            client: self.clone(),
            account_id,
        }
    }
}

/// A `Client` and one account id.
#[derive(Debug, Clone)]
pub struct AccountClient {
    client: Client,
    account_id: i64,
}

impl AccountClient {
    /// The account id.
    #[must_use]
    pub fn id(&self) -> i64 {
        self.account_id
    }

    /// The connection underneath, for the calls that are not about an account.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Logs the account in on the connection with `access_token`.
    pub async fn authorize(&self, access_token: &str) -> Result<()> {
        self.client
            .authorize_account(self.account_id, access_token)
            .await
            .map(|_| ())
    }

    /// Logs the account out of this connection.
    pub async fn logout(&self) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::ACCOUNT_LOGOUT_REQ,
                payload::ACCOUNT_LOGOUT_RES,
                &AccountReq {
                    ctid_trader_account_id: self.account_id,
                },
                RateClass::Standard,
                "the account log out",
            )
            .await?;
        Ok(())
    }

    /// Symbol lookup, live prices, the order book, and history.
    #[must_use]
    pub fn market(&self) -> MarketClient {
        MarketClient::new(self.client.clone(), self.account_id)
    }

    /// The account's own data: balance, positions, orders, deals.
    #[must_use]
    pub fn account_data(&self) -> AccountDataClient {
        AccountDataClient::new(self.client.clone(), self.account_id)
    }

    /// Places, amends and cancels orders, and closes positions.
    #[must_use]
    pub fn trading(&self) -> TradingClient {
        TradingClient::new(self.client.clone(), self.account_id)
    }

    /// Expected margin, margin call thresholds, dynamic leverage.
    #[must_use]
    pub fn margin(&self) -> MarginClient {
        MarginClient::new(self.client.clone(), self.account_id)
    }
}
