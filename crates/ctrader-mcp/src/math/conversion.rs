//! Shortest spot-rate currency chain resolution, ported from
//! `scripts/conversion_rate.py`.
//!
//! When a symbol's quote currency differs from the account currency, every money figure
//! (commission, swap, realized P&L, pip value, margin) requires conversion through a
//! chain of spot rates (`SKILL.md` "Currency conversion: quote currency vs account
//! currency"). This module builds that chain by breadth-first search over the quote map
//! fetched from `get_spot_prices`, so the shortest available chain is always used, and
//! the composite rate is the product of each hop's edge rate.

use std::collections::{HashMap, VecDeque};

/// A normalized quote map: canonical 6-letter symbol -> `(base_currency,
/// quote_currency, rate)`. The output of [`parse_quotes`] and the input to
/// [`compute_chain`].
pub type QuoteEdgeMap = HashMap<String, (String, String, f64)>;

/// Which side of a bid/ask spread to use when a quote is supplied as a spread rather
/// than a single scalar rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteSide {
    Bid,
    Ask,
    Mid,
}

/// A single input quote: either a scalar rate, or a bid/ask spread (resolved to a single
/// rate per [`QuoteSide`] before entering the conversion graph).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuoteValue {
    Rate(f64),
    BidAsk { bid: f64, ask: f64 },
}

impl QuoteValue {
    fn resolve(self, side: QuoteSide) -> f64 {
        match self {
            QuoteValue::Rate(rate) => rate,
            QuoteValue::BidAsk { bid, ask } => match side {
                QuoteSide::Bid => bid,
                QuoteSide::Ask => ask,
                QuoteSide::Mid => (bid + ask) / 2.0,
            },
        }
    }
}

/// The result of [`compute_chain`].
#[derive(Debug, Clone, PartialEq)]
pub struct ChainResult {
    /// The composite conversion rate (`0.0` if no chain was found).
    pub rate: f64,
    /// The symbols used, in hop order (e.g. `["EURUSD", "USDJPY"]` for a `EUR -> JPY`
    /// chain via USD). Empty if `from_asset == to_asset` or no chain was found.
    pub chain: Vec<String>,
    pub hops: usize,
    /// Non-fatal notices: symbols skipped during loose normalization, or a "no chain
    /// found" notice (which is not itself an error: `rate` will be `0.0` and the caller
    /// decides how to handle it, per `SKILL.md`'s W5 "Conversion chain undefined" edge
    /// case).
    pub warnings: Vec<String>,
}

/// Normalizes a broker symbol display variant (`"USD/ZAR"`, `"usd_zar"`, `"EUR.USD"`) to
/// canonical 6-uppercase-letter form by stripping non-alphabetic separators and
/// uppercasing. Returns `None` (with a reason) if the result isn't exactly 6 letters or
/// has an identical base/quote.
fn normalize_symbol(raw: &str) -> Result<String, String> {
    let stripped: String = raw.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let canonical = stripped.to_ascii_uppercase();
    if canonical.len() != 6 {
        return Err(format!(
            "after stripping separators, expected 6 alpha chars, got {} ({canonical:?})",
            canonical.len()
        ));
    }
    let (base, quote) = canonical.split_at(3);
    if base == quote {
        return Err(format!(
            "normalized {raw:?} -> {canonical:?} has identical base and quote currencies"
        ));
    }
    Ok(canonical)
}

/// Parses a raw `symbol -> quote` map into the normalized `symbol -> (base, quote,
/// rate)` edges [`compute_chain`] operates on.
///
/// When `strict` is `false` (the default the skill recommends: see
/// `references/*.md` "Phase 4 forward-reference" notes), symbols that don't normalize to
/// 6 uppercase letters are skipped with a warning rather than rejected outright, so a
/// batch `get_spot_prices` response containing a stray malformed entry doesn't fail the
/// whole conversion. When `strict` is `true`, every key must already be exactly 6
/// uppercase letters.
pub fn parse_quotes(
    quotes: &HashMap<String, QuoteValue>,
    side: QuoteSide,
    strict: bool,
) -> (QuoteEdgeMap, Vec<String>) {
    let mut normalized = HashMap::with_capacity(quotes.len());
    let mut warnings = Vec::new();

    for (raw_symbol, value) in quotes {
        let canonical = if strict {
            let is_strict_form =
                raw_symbol.len() == 6 && raw_symbol.chars().all(|c| c.is_ascii_uppercase());
            if !is_strict_form {
                warnings.push(format!(
                    "skipped {raw_symbol:?}: not 6 uppercase letters (strict mode)"
                ));
                continue;
            }
            raw_symbol.clone()
        } else {
            match normalize_symbol(raw_symbol) {
                Ok(canonical) => {
                    if &canonical != raw_symbol {
                        warnings.push(format!("normalized {raw_symbol:?} -> {canonical:?}"));
                    }
                    canonical
                }
                Err(reason) => {
                    warnings.push(format!("skipped {raw_symbol:?}: {reason}"));
                    continue;
                }
            }
        };

        let (base, quote) = canonical.split_at(3);
        let rate = value.resolve(side);
        normalized.insert(
            canonical.clone(),
            (base.to_string(), quote.to_string(), rate),
        );
    }

    (normalized, warnings)
}

/// Finds the shortest spot-rate chain from `from_asset` to `to_asset` by breadth-first
/// search over `quotes` (as produced by [`parse_quotes`]).
///
/// Each quote `BASEQUOTE -> rate` contributes two graph edges: `base -> quote` at
/// `rate`, and `quote -> base` at `1 / rate`. Returns `rate: 0.0` with a `warnings` entry
/// (not an error) if `from_asset` and `to_asset` are not connected within `max_hops`.
pub fn compute_chain(
    from_asset: &str,
    to_asset: &str,
    quotes: &QuoteEdgeMap,
    max_hops: usize,
) -> ChainResult {
    if from_asset == to_asset {
        return ChainResult {
            rate: 1.0,
            chain: Vec::new(),
            hops: 0,
            warnings: Vec::new(),
        };
    }

    let mut adjacency: HashMap<&str, Vec<(&str, f64, &str)>> = HashMap::new();
    for (symbol, (base, quote, rate)) in quotes {
        adjacency
            .entry(base.as_str())
            .or_default()
            .push((quote.as_str(), *rate, symbol.as_str()));
        adjacency.entry(quote.as_str()).or_default().push((
            base.as_str(),
            1.0 / rate,
            symbol.as_str(),
        ));
    }

    let no_chain = || ChainResult {
        rate: 0.0,
        chain: Vec::new(),
        hops: 0,
        warnings: vec![format!(
            "no chain found from {from_asset} to {to_asset} within {max_hops} hops"
        )],
    };

    if !adjacency.contains_key(from_asset) {
        return no_chain();
    }

    let mut queue: VecDeque<(&str, Vec<&str>, f64)> = VecDeque::new();
    queue.push_back((from_asset, Vec::new(), 1.0));
    let mut visited: std::collections::HashSet<&str> =
        std::collections::HashSet::from([from_asset]);

    while let Some((current, chain_so_far, rate_so_far)) = queue.pop_front() {
        if chain_so_far.len() >= max_hops {
            continue;
        }
        let Some(edges) = adjacency.get(current) else {
            continue;
        };
        for &(neighbor, edge_rate, edge_symbol) in edges {
            if visited.contains(neighbor) {
                continue;
            }
            let mut new_chain = chain_so_far.clone();
            new_chain.push(edge_symbol);
            let new_rate = rate_so_far * edge_rate;

            if neighbor == to_asset {
                return ChainResult {
                    rate: new_rate,
                    chain: new_chain.into_iter().map(str::to_owned).collect(),
                    hops: chain_so_far.len() + 1,
                    warnings: Vec::new(),
                };
            }
            visited.insert(neighbor);
            queue.push_back((neighbor, new_chain, new_rate));
        }
    }

    no_chain()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quotes_from(pairs: &[(&str, f64)]) -> HashMap<String, QuoteValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), QuoteValue::Rate(*v)))
            .collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    // Ported from conversion_rate.py's `_self_test` fixtures.

    #[test]
    fn jpy_to_usd_via_usdjpy() {
        let raw = quotes_from(&[("USDJPY", 150.3)]);
        let (quotes, _) = parse_quotes(&raw, QuoteSide::Mid, false);
        let result = compute_chain("JPY", "USD", &quotes, 3);
        assert_eq!(result.chain, vec!["USDJPY"]);
        assert_eq!(result.hops, 1);
        assert!(close(result.rate, 0.0066533599), "got {}", result.rate);
    }

    #[test]
    fn eur_to_usd_direct() {
        let raw = quotes_from(&[("EURUSD", 1.0850)]);
        let (quotes, _) = parse_quotes(&raw, QuoteSide::Mid, false);
        let result = compute_chain("EUR", "USD", &quotes, 3);
        assert_eq!(result.chain, vec!["EURUSD"]);
        assert!(close(result.rate, 1.085));
    }

    #[test]
    fn eur_to_jpy_two_hop() {
        let raw = quotes_from(&[("EURUSD", 1.0850), ("USDJPY", 150.3)]);
        let (quotes, _) = parse_quotes(&raw, QuoteSide::Mid, false);
        let result = compute_chain("EUR", "JPY", &quotes, 3);
        assert_eq!(result.chain, vec!["EURUSD", "USDJPY"]);
        assert!(close(result.rate, 163.0755), "got {}", result.rate);
    }

    #[test]
    fn xau_to_eur_via_usd() {
        let raw = quotes_from(&[("XAUUSD", 1900.5), ("EURUSD", 1.0850)]);
        let (quotes, _) = parse_quotes(&raw, QuoteSide::Mid, false);
        let result = compute_chain("XAU", "EUR", &quotes, 3);
        assert_eq!(result.chain, vec!["XAUUSD", "EURUSD"]);
    }

    #[test]
    fn same_asset_is_identity() {
        let raw = quotes_from(&[("EURUSD", 1.085)]);
        let (quotes, _) = parse_quotes(&raw, QuoteSide::Mid, false);
        let result = compute_chain("USD", "USD", &quotes, 3);
        assert_eq!(result.chain, Vec::<String>::new());
        assert_eq!(result.rate, 1.0);
    }

    #[test]
    fn no_chain_found_reports_zero_rate_and_warning() {
        let raw = quotes_from(&[("EURUSD", 1.0850)]);
        let (quotes, _) = parse_quotes(&raw, QuoteSide::Mid, false);
        let result = compute_chain("NZD", "BRL", &quotes, 3);
        assert_eq!(result.rate, 0.0);
        assert_eq!(result.chain, Vec::<String>::new());
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn bid_ask_side_selection() {
        let mut raw = HashMap::new();
        raw.insert(
            "EURUSD".to_string(),
            QuoteValue::BidAsk {
                bid: 1.0848,
                ask: 1.0852,
            },
        );
        let (quotes, _) = parse_quotes(&raw, QuoteSide::Ask, false);
        let result = compute_chain("EUR", "USD", &quotes, 3);
        assert!(close(result.rate, 1.0852));
    }

    #[test]
    fn loose_display_variant_normalizes() {
        let raw = quotes_from(&[("USD/ZAR", 18.50)]);
        let (quotes, warnings) = parse_quotes(&raw, QuoteSide::Mid, false);
        assert!(!warnings.is_empty(), "expected a normalization warning");
        let result = compute_chain("USD", "ZAR", &quotes, 3);
        assert_eq!(result.chain, vec!["USDZAR"]);
        assert!(close(result.rate, 18.50));
    }

    #[test]
    fn loose_malformed_symbol_skipped_with_warning_chain_still_computed() {
        let raw = quotes_from(&[("EURUSD", 1.0850), ("BADSYM!", 2.0)]);
        let (quotes, warnings) = parse_quotes(&raw, QuoteSide::Mid, false);
        assert!(warnings.iter().any(|w| w.contains("BADSYM")));
        let result = compute_chain("EUR", "USD", &quotes, 3);
        assert_eq!(result.chain, vec!["EURUSD"]);
    }

    #[test]
    fn strict_mode_rejects_display_variant() {
        let raw = quotes_from(&[("USD/ZAR", 18.50)]);
        let (quotes, warnings) = parse_quotes(&raw, QuoteSide::Mid, true);
        assert!(quotes.is_empty());
        assert!(!warnings.is_empty());
    }
}
