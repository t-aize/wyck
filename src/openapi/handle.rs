//! A client bound to one trading account: a router to the four domain sub-clients.
//!
//! Almost every call in this crate needs an authorized account. [`AccountClient`] holds a
//! [`Client`] and an account id and hands out one small, `Clone` sub-client per domain, each still
//! carrying the [`Client`] and the account id (no lifetime, so trivial to keep across an `.await`
//! or move into a task):
//!
//! ```no_run
//! # async fn demo(client: wyck::openapi::Client, account_id: i64) -> wyck::openapi::Result<()> {
//! let account = client.account(account_id);
//! account.authorize("access-token").await?;
//!
//! let market = account.market();
//! let symbols = market.symbols().await?;
//! market.subscribe_spots(&[symbols[0].symbol_id]).await?;
//!
//! let balance = account.account_data().trader().await?.balance_amount();
//! # let _ = balance; Ok(()) }
//! ```

use crate::openapi::account::AccountDataClient;
use crate::openapi::error::Result;
use crate::openapi::margin::MarginClient;
use crate::openapi::market::MarketClient;
use crate::openapi::trading::TradingClient;
use crate::openapi::transport::connection::{Client, RateClass};
use crate::openapi::transport::messages::AccountReq;
use crate::openapi::transport::wire::payload;

impl Client {
    /// The client bound to `account_id`. The account must still be authorized on the connection
    /// (see [`AccountClient::authorize`]).
    #[must_use]
    pub fn account(&self, account_id: i64) -> AccountClient {
        AccountClient {
            client: self.clone(),
            account_id,
        }
    }
}

/// A [`Client`] and one account id. Cheap to clone. See the [module docs](self).
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

    /// Logs the account in on the connection with `access_token`. Data calls for the account need
    /// it.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED`, or a token error. A demo account cannot be authorized on a live
    /// connection, and the other way round.
    pub async fn authorize(&self, access_token: &str) -> Result<()> {
        self.client
            .authorize_account(self.account_id, access_token)
            .await
            .map(|_| ())
    }

    /// Logs the account out of this connection. Its subscriptions end; authorize it again to use it
    /// again.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
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

    /// The account's own data: balance, positions, orders, deals. Named `account_data` rather than
    /// `account` to avoid confusion with [`AccountClient`] itself and with [`Client::account`].
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
