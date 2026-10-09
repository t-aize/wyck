//! The order book events of the server (`ProtoOADepthEvent`).

use serde::Deserialize;

use crate::domain::flex;

/// One order book entry (`ProtoOADepthQuote`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthQuote {
    /// The entry id.
    #[serde(default, deserialize_with = "flex::opt")]
    pub id: Option<i64>,
    /// The size, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub size: Option<i64>,
    /// The price, for a bid entry.
    #[serde(default, deserialize_with = "flex::opt")]
    pub bid: Option<i64>,
    /// The price, for an ask entry.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ask: Option<i64>,
}

/// `ProtoOADepthEvent`: changes of the order book.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthEvent {
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// Entries that were added or changed.
    #[serde(default)]
    pub new_quotes: Vec<DepthQuote>,
    /// Ids of the entries that were removed.
    #[serde(default, deserialize_with = "flex::list")]
    pub deleted_quotes: Vec<i64>,
}
