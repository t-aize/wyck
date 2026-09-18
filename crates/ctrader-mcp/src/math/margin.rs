//! Dynamic-leverage tiered margin computation, ported from `scripts/tiered_margin.py`.
//!
//! Brokers apply leverage tiers that fall as notional exposure grows; required margin is
//! computed **per tier** and summed (`SKILL.md` "Dynamic-leverage margin calculation").

use serde::{Deserialize, Serialize};

/// One leverage tier: covers exposure up to `upper_usd` (exclusive of the previous
/// tier's bound), at `leverage:1`. The final tier in a curve must have `upper_usd:
/// None`, meaning "everything above the last finite bound".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MarginTier {
    /// Upper exposure bound in USD for this tier, or `None` for the final catch-all
    /// tier.
    pub upper_usd: Option<f64>,
    /// Leverage ratio for this tier (e.g. `500` means `1:500`).
    pub leverage: u32,
}

/// Per-tier margin contribution, included in [`TieredMarginResult`] for transparency
/// (e.g. to show the user exactly how a multi-tier margin figure was assembled).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TierBreakdown {
    pub upper_usd: Option<f64>,
    pub leverage: u32,
    pub absorbed_usd: f64,
    pub tier_margin_usd: f64,
}

/// The result of [`compute_tiered_margin`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TieredMarginResult {
    pub margin_usd: f64,
    pub margin_account_ccy: f64,
    pub notional_usd: f64,
    pub per_tier_breakdown: Vec<TierBreakdown>,
}

/// Errors from [`compute_tiered_margin`]'s tier-curve validation.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MarginTierError {
    #[error("tier curve must contain at least one tier")]
    Empty,
    #[error("tier {index} has upper_usd <= 0")]
    NonPositiveUpper { index: usize },
    #[error("tier {index} leverage must be > 0")]
    NonPositiveLeverage { index: usize },
    #[error("tier {index} upper_usd must be strictly greater than the previous tier's upper_usd")]
    NotStrictlyAscending { index: usize },
    #[error("only the final tier may have upper_usd = None (tier {index} is not final)")]
    NonFinalOpenTier { index: usize },
    #[error(
        "the final tier must have upper_usd = None (covers all exposure above the last finite bound)"
    )]
    FinalTierMustBeOpen,
}

fn validate_tiers(tiers: &[MarginTier]) -> Result<(), MarginTierError> {
    if tiers.is_empty() {
        return Err(MarginTierError::Empty);
    }
    let mut last_upper = Some(0.0);
    let last_index = tiers.len() - 1;
    for (index, tier) in tiers.iter().enumerate() {
        if tier.leverage == 0 {
            return Err(MarginTierError::NonPositiveLeverage { index });
        }
        match tier.upper_usd {
            None if index != last_index => {
                return Err(MarginTierError::NonFinalOpenTier { index });
            }
            None => {}
            Some(upper) => {
                if upper <= 0.0 {
                    return Err(MarginTierError::NonPositiveUpper { index });
                }
                if let Some(prev) = last_upper
                    && upper <= prev
                {
                    return Err(MarginTierError::NotStrictlyAscending { index });
                }
                last_upper = Some(upper);
            }
        }
    }
    if tiers[last_index].upper_usd.is_some() {
        return Err(MarginTierError::FinalTierMustBeOpen);
    }
    Ok(())
}

/// Computes required margin in USD (and the account currency) for a position of
/// `volume_base_units` base-asset units, where `quote_rate_usd` is the USD value of one
/// base-asset unit (for a symbol quoted directly in USD this is simply the spot price),
/// across the dynamic-leverage `tiers` curve.
///
/// `account_currency_rate_vs_usd` is the multiplier from USD to the account currency
/// (`1 USD = rate account-ccy units`); pass `1.0` for a USD account, or e.g. the
/// `USDJPY` spot rate for a JPY account. Resolve this rate with
/// [`crate::math::conversion::compute_chain`] against a live quote map when the account
/// currency isn't USD.
///
/// # Errors
///
/// Returns [`MarginTierError`] if the tier curve is malformed (empty, non-ascending
/// bounds, a non-final open tier, or a final tier that isn't open-ended).
pub fn compute_tiered_margin(
    volume_base_units: i64,
    quote_rate_usd: f64,
    tiers: &[MarginTier],
    account_currency_rate_vs_usd: f64,
) -> Result<TieredMarginResult, MarginTierError> {
    validate_tiers(tiers)?;

    let notional_usd = volume_base_units as f64 * quote_rate_usd;
    let mut remaining = notional_usd;
    let mut cursor = 0.0f64;
    let mut margin_total = 0.0f64;
    let mut breakdown = Vec::with_capacity(tiers.len());

    for tier in tiers {
        let absorbed = match tier.upper_usd {
            None => remaining,
            Some(upper) => {
                let capacity = upper - cursor;
                capacity.min(remaining)
            }
        }
        .max(0.0);

        let tier_margin = absorbed / tier.leverage as f64;
        margin_total += tier_margin;
        breakdown.push(TierBreakdown {
            upper_usd: tier.upper_usd,
            leverage: tier.leverage,
            absorbed_usd: absorbed,
            tier_margin_usd: tier_margin,
        });

        remaining -= absorbed;
        if let Some(upper) = tier.upper_usd {
            cursor = upper;
        }
        if remaining <= 0.0 {
            break;
        }
    }

    Ok(TieredMarginResult {
        margin_usd: margin_total,
        margin_account_ccy: margin_total * account_currency_rate_vs_usd,
        notional_usd,
        per_tier_breakdown: breakdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    // Ported from tiered_margin.py's `_self_test` fixtures.

    #[test]
    fn eurusd_1m_across_three_tiers_skill_md_example() {
        let tiers = [
            MarginTier {
                upper_usd: Some(1_000_000.0),
                leverage: 500,
            },
            MarginTier {
                upper_usd: Some(5_000_000.0),
                leverage: 200,
            },
            MarginTier {
                upper_usd: None,
                leverage: 100,
            },
        ];
        let result = compute_tiered_margin(1_000_000, 1.21345, &tiers, 1.0).unwrap();
        assert!(
            close(result.margin_usd, 3067.25),
            "got {}",
            result.margin_usd
        );
        assert!(close(result.notional_usd, 1_213_450.0));
    }

    #[test]
    fn xauusd_single_tier() {
        let tiers = [MarginTier {
            upper_usd: None,
            leverage: 100,
        }];
        let result = compute_tiered_margin(100, 1900.50, &tiers, 1.0).unwrap();
        assert!(close(result.margin_usd, 1900.5));
        assert!(close(result.notional_usd, 190_050.0));
    }

    #[test]
    fn exposure_exactly_equals_first_tier_upper() {
        let tiers = [
            MarginTier {
                upper_usd: Some(1_000_000.0),
                leverage: 500,
            },
            MarginTier {
                upper_usd: None,
                leverage: 200,
            },
        ];
        let result = compute_tiered_margin(1_000_000, 1.0, &tiers, 1.0).unwrap();
        assert!(close(result.margin_usd, 2000.0));
    }

    #[test]
    fn tier_crossing_5m_position() {
        let tiers = [
            MarginTier {
                upper_usd: Some(1_000_000.0),
                leverage: 500,
            },
            MarginTier {
                upper_usd: Some(5_000_000.0),
                leverage: 200,
            },
            MarginTier {
                upper_usd: None,
                leverage: 100,
            },
        ];
        let result = compute_tiered_margin(5_000_000, 1.0, &tiers, 1.0).unwrap();
        assert!(
            close(result.margin_usd, 22_000.0),
            "got {}",
            result.margin_usd
        );
    }

    #[test]
    fn empty_tiers_rejected() {
        assert_eq!(
            compute_tiered_margin(1000, 1.0, &[], 1.0),
            Err(MarginTierError::Empty)
        );
    }

    #[test]
    fn final_tier_must_be_open() {
        let tiers = [MarginTier {
            upper_usd: Some(1000.0),
            leverage: 100,
        }];
        assert_eq!(
            compute_tiered_margin(1000, 1.0, &tiers, 1.0),
            Err(MarginTierError::FinalTierMustBeOpen)
        );
    }

    #[test]
    fn jpy_account_conversion() {
        let tiers = [
            MarginTier {
                upper_usd: Some(1_000_000.0),
                leverage: 500,
            },
            MarginTier {
                upper_usd: Some(5_000_000.0),
                leverage: 200,
            },
            MarginTier {
                upper_usd: None,
                leverage: 100,
            },
        ];
        let result = compute_tiered_margin(1_000_000, 1.21345, &tiers, 149.25).unwrap();
        assert!(close(result.margin_usd, 3067.25));
        assert!(
            close(result.margin_account_ccy, 457_787.062_5),
            "got {}",
            result.margin_account_ccy
        );
    }
}
