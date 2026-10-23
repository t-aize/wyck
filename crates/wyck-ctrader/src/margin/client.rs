//! [`MarginClient`]: the calls themselves.

use super::ExpectedMarginRes;
use super::requests::ExpectedMarginReq as MarginReq;
use wyck_domain::trading::ExpectedMargin;

use crate::connection::{Client, RateClass};
use crate::error::Result;
use crate::transport::wire::payload;

/// Margin bound to one account: the margin a volume would need. See
/// [`crate::AccountClient::margin`].
///
/// ```no_run
/// # async fn demo(account: wyck_ctrader::AccountClient) -> wyck_ctrader::Result<()> {
/// for margin in account.margin().expected_margin(1, &[100_000]).await? {
///     println!("buy {} sell {}", margin.buy_margin, margin.sell_margin);
/// }
/// # Ok(()) }
/// ```
#[derive(Debug, Clone)]
pub struct MarginClient {
    client: Client,
    account_id: i64,
}

impl MarginClient {
    pub(crate) fn new(client: Client, account_id: i64) -> Self {
        Self { client, account_id }
    }

    /// The margin a buy and a sell of each of `volumes` would use on `symbol_id`. Does not cover
    /// the `ACCORDING_TO_GSL` margin calculation type: with a guaranteed stop loss the margin is
    /// simply `(entry price - GSL price) * volume`, in the deposit currency.
    ///
    /// # Errors
    ///
    /// `SYMBOL_NOT_FOUND`, and the usual account errors.
    pub async fn expected_margin(
        &self,
        symbol_id: i64,
        volumes: &[i64],
    ) -> Result<Vec<ExpectedMargin>> {
        let response: ExpectedMarginRes = self
            .client
            .call(
                payload::EXPECTED_MARGIN_REQ,
                payload::EXPECTED_MARGIN_RES,
                &MarginReq {
                    ctid_trader_account_id: self.account_id,
                    symbol_id,
                    volume: volumes.to_vec(),
                },
                RateClass::Standard,
                "the expected margin",
            )
            .await?;
        Ok(response.margin)
    }
}
