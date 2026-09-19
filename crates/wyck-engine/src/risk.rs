//! Turning "buy EURUSD, stop 30 pips, risk 1%" into an exact, validated order.
//!
//! [`build_plan`] is a **pure function**: quote, instrument and account figures in, plan
//! out, no I/O and no clock. That is what makes sizing testable exhaustively, and it is why
//! planning can never send anything.
//!
//! # The arithmetic
//!
//! For a stop `d` away from entry (in price terms), a symbol with `lot_size` units per lot,
//! and a quote-to-account currency rate `c`:
//!
//! ```text
//! risk per unit of volume = d * c              (account currency)
//! units                   = floor(target_risk / (d * c)), then rounded DOWN to the step
//! actual risk             = units * d * c      (never above the target)
//! ```
//!
//! Volume is always rounded *down*: rounding up would risk more than was asked for. The
//! stop distance is rounded *up* to what the broker can express (`stop_granularity`), and
//! the volume is sized on that rounded distance, so what is sized is exactly what is sent.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::domain::{Instrument, Quote, Side, SpecsSource, UnixMillis, Volume, now_millis};
use crate::error::{EngineError, Result};
use crate::ids::AccountId;

/// Identifies a plan created by the engine. Only plans the engine itself created (and has
/// not expired) can be submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanId(u64);

impl PlanId {
    pub(crate) fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// The raw counter value.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for PlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plan-{}", self.0)
    }
}

/// How much to risk on a trade.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RiskSpec {
    /// A percentage of the account balance, in `(0, 100)`.
    PercentOfBalance(f64),
    /// A fixed amount in account currency.
    Amount(f64),
}

/// How to size the order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SizeSpec {
    /// Size from the stop distance so the loss at the stop equals the given risk.
    Risk(RiskSpec),
    /// Use exactly this volume.
    Fixed(Volume),
}

/// Where to put the stop loss.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StopSpec {
    /// This many pips from the entry.
    Pips(f64),
    /// This price distance from the entry.
    Distance(f64),
    /// An absolute price. Must be on the losing side of the entry.
    Price(f64),
}

/// Where to put the take profit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TakeProfitSpec {
    /// This multiple of the stop distance.
    RiskReward(f64),
    /// This many pips from the entry.
    Pips(f64),
    /// This price distance from the entry.
    Distance(f64),
    /// An absolute price. Must be on the winning side of the entry.
    Price(f64),
}

/// What the user wants to do, before any numbers are resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryIntent {
    /// The ticker.
    pub symbol: String,
    /// Buy or sell.
    pub side: Side,
    /// How to size it.
    pub size: SizeSpec,
    /// The stop loss. Required when sizing by risk.
    pub stop_loss: Option<StopSpec>,
    /// The take profit, if any.
    pub take_profit: Option<TakeProfitSpec>,
}

/// A fully resolved, validated market order that has not been sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderPlan {
    /// The plan's identity. Submit it back to the engine to act on it.
    pub id: PlanId,
    /// The account it is for.
    pub account: AccountId,
    /// The ticker.
    pub symbol: String,
    /// Buy or sell.
    pub side: Side,
    /// The volume that will be sent.
    pub volume: Volume,
    /// The quote price the plan was computed against (ask for a buy, bid for a sell).
    pub entry_reference: f64,
    /// Stop distance from entry, after rounding to what the broker can express.
    pub stop_loss_distance: Option<f64>,
    /// Take-profit distance from entry.
    pub take_profit_distance: Option<f64>,
    /// The stop loss as a price, relative to `entry_reference`.
    pub stop_loss_price: Option<f64>,
    /// The take profit as a price, relative to `entry_reference`.
    pub take_profit_price: Option<f64>,
    /// The loss at the stop, in account currency, when it can be computed.
    pub risk_amount: Option<f64>,
    /// `risk_amount` as a percentage of balance.
    pub risk_percent: Option<f64>,
    /// The account currency, when known.
    pub account_currency: Option<String>,
    /// Ask minus bid when the plan was made.
    pub spread: f64,
    /// Whether the instrument's volume rules are the broker's or assumed.
    pub specs_source: SpecsSource,
    /// Notes for the user: assumptions, rounding, wide spread. Never blocking.
    pub warnings: Vec<String>,
    /// When the plan was made, in Unix milliseconds.
    pub created_at: UnixMillis,
}

/// Everything [`build_plan`] needs besides the intent.
#[derive(Debug, Clone, Copy)]
pub struct PlanContext<'a> {
    /// The account the plan is for.
    pub account: &'a AccountId,
    /// The account currency, if known.
    pub account_currency: Option<&'a str>,
    /// The instrument being traded.
    pub instrument: &'a Instrument,
    /// A current quote for it.
    pub quote: &'a Quote,
    /// The account balance, needed for percentage risk.
    pub balance: Option<f64>,
    /// How many account-currency units one quote-currency unit is worth. `1.0` when the
    /// quote currency is the account currency; `None` when it could not be determined.
    pub conversion_rate: Option<f64>,
    /// The smallest stop distance the broker can express, from
    /// [`Broker::stop_granularity`](crate::broker::Broker::stop_granularity).
    pub stop_granularity: f64,
}

/// Rounds a distance up to a multiple of `granularity` (at least one step), tolerating
/// float noise so an exact multiple is not bumped to the next one.
fn ceil_to(distance: f64, granularity: f64) -> f64 {
    ((distance / granularity) - 1e-6).ceil().max(1.0) * granularity
}

fn round_to(distance: f64, granularity: f64) -> f64 {
    (distance / granularity).round().max(1.0) * granularity
}

fn positive(name: &str, value: f64) -> Result<f64> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(EngineError::Invalid(format!(
            "{name} must be a positive number, got {value}"
        )))
    }
}

/// Resolves and validates an intent. See the [module docs](self) for the arithmetic.
///
/// # Errors
///
/// [`EngineError::Invalid`] naming what is wrong: a disabled instrument, an unusable quote,
/// a stop on the wrong side of the entry, missing data needed for the requested sizing, or a
/// risk too small for the instrument's minimum volume.
pub fn build_plan(intent: &EntryIntent, ctx: &PlanContext<'_>) -> Result<OrderPlan> {
    let instrument = ctx.instrument;
    let quote = ctx.quote;
    if !instrument.enabled {
        return Err(EngineError::Invalid(format!(
            "{} is not tradable right now",
            instrument.symbol
        )));
    }
    if !quote.is_valid() {
        return Err(EngineError::Invalid(format!(
            "the quote for {} is not usable (bid {}, ask {})",
            instrument.symbol, quote.bid, quote.ask
        )));
    }
    if !(ctx.stop_granularity.is_finite() && ctx.stop_granularity > 0.0) {
        return Err(EngineError::Internal(
            "stop granularity must be positive".to_owned(),
        ));
    }

    let side = intent.side;
    let entry = quote.price_for(side);
    let direction = match side {
        Side::Buy => 1.0,
        Side::Sell => -1.0,
    };
    let g = ctx.stop_granularity;

    // Stop loss distance, on the losing side.
    let stop_distance = match intent.stop_loss {
        None => None,
        Some(spec) => {
            let raw = match spec {
                StopSpec::Pips(p) => instrument.pips_to_distance(positive("stop loss pips", p)?),
                StopSpec::Distance(d) => positive("stop loss distance", d)?,
                StopSpec::Price(p) => {
                    let signed = (entry - positive("stop loss price", p)?) * direction;
                    if signed <= 0.0 {
                        return Err(EngineError::Invalid(format!(
                            "a stop loss for a {side} at {entry} must be {} the entry, got {p}",
                            if side == Side::Buy { "below" } else { "above" }
                        )));
                    }
                    signed
                }
            };
            Some(ceil_to(raw, g))
        }
    };

    // Take profit distance, on the winning side.
    let take_profit_distance = match intent.take_profit {
        None => None,
        Some(spec) => {
            let raw = match spec {
                TakeProfitSpec::RiskReward(r) => {
                    let sl = stop_distance.ok_or_else(|| {
                        EngineError::Invalid(
                            "a risk/reward take profit needs a stop loss".to_owned(),
                        )
                    })?;
                    sl * positive("risk/reward ratio", r)?
                }
                TakeProfitSpec::Pips(p) => {
                    instrument.pips_to_distance(positive("take profit pips", p)?)
                }
                TakeProfitSpec::Distance(d) => positive("take profit distance", d)?,
                TakeProfitSpec::Price(p) => {
                    let signed = (positive("take profit price", p)? - entry) * direction;
                    if signed <= 0.0 {
                        return Err(EngineError::Invalid(format!(
                            "a take profit for a {side} at {entry} must be {} the entry, got {p}",
                            if side == Side::Buy { "above" } else { "below" }
                        )));
                    }
                    signed
                }
            };
            Some(round_to(raw, g))
        }
    };

    let mut warnings = Vec::new();

    // Volume.
    let volume = match intent.size {
        SizeSpec::Fixed(v) => {
            instrument.check_volume(v).map_err(EngineError::Invalid)?;
            v
        }
        SizeSpec::Risk(risk) => {
            let sl = stop_distance.ok_or_else(|| {
                EngineError::Invalid("sizing by risk needs a stop loss".to_owned())
            })?;
            let rate = ctx.conversion_rate.filter(|r| r.is_finite() && *r > 0.0).ok_or_else(|| {
                EngineError::Invalid(format!(
                    "cannot convert {} risk to the account currency: no exchange rate available",
                    instrument.quote_currency.as_deref().unwrap_or("quote currency")
                ))
            })?;
            let target = match risk {
                RiskSpec::Amount(a) => positive("risk amount", a)?,
                RiskSpec::PercentOfBalance(p) => {
                    if !(p.is_finite() && p > 0.0 && p < 100.0) {
                        return Err(EngineError::Invalid(format!(
                            "risk percent must be between 0 and 100, got {p}"
                        )));
                    }
                    let balance = ctx
                        .balance
                        .filter(|b| b.is_finite() && *b > 0.0)
                        .ok_or_else(|| {
                            EngineError::Invalid(
                                "the account balance is unknown or not positive".to_owned(),
                            )
                        })?;
                    balance * p / 100.0
                }
            };
            let per_unit = sl * rate;
            // Truncation is the point: floor to hundredths of a unit, then round down to the step.
            #[allow(clippy::cast_possible_truncation)]
            let raw_cents = (target / per_unit * 100.0).floor().min(9.0e18) as i64;
            let mut volume = instrument.round_volume_down(Volume::from_cents(raw_cents));
            if let Some(max) = instrument.volume.max
                && volume > max
            {
                volume = max.round_down_to(instrument.volume.step);
                warnings.push(format!(
                    "the volume was capped at the instrument maximum of {max}, so the risk is below the target"
                ));
            }
            if volume < instrument.volume.min {
                let min_risk = instrument.volume.min.as_units() * per_unit;
                return Err(EngineError::Invalid(format!(
                    "a risk of {target:.2} is too small for {} at this stop: the minimum volume ({}) already risks {min_risk:.2}",
                    instrument.symbol, instrument.volume.min
                )));
            }
            volume
        }
    };

    // Actual risk, when it can be computed.
    let risk_amount = match (stop_distance, ctx.conversion_rate) {
        (Some(sl), Some(rate)) if rate.is_finite() && rate > 0.0 => {
            Some(volume.as_units() * sl * rate)
        }
        _ => None,
    };
    let risk_percent = match (risk_amount, ctx.balance) {
        (Some(r), Some(b)) if b > 0.0 => Some(r / b * 100.0),
        _ => None,
    };

    if instrument.specs_source == SpecsSource::Assumed {
        warnings.push(format!(
            "the lot size, minimum volume and step for {} are assumed, not published by the broker; check them before trusting the size",
            instrument.symbol
        ));
    }
    let spread = quote.spread();
    if let Some(sl) = stop_distance
        && spread > 0.25 * sl
    {
        warnings.push(format!(
            "the spread ({:.1} pips) is more than a quarter of the stop distance ({:.1} pips)",
            instrument.distance_to_pips(spread),
            instrument.distance_to_pips(sl)
        ));
    }
    if stop_distance.is_none() {
        warnings.push("this order has no stop loss".to_owned());
    }

    let price = |distance: f64, toward_loss: bool| {
        let sign = if toward_loss { -direction } else { direction };
        instrument.round_price(entry + sign * distance)
    };
    Ok(OrderPlan {
        id: PlanId::next(),
        account: ctx.account.clone(),
        symbol: instrument.symbol.clone(),
        side,
        volume,
        entry_reference: entry,
        stop_loss_distance: stop_distance,
        take_profit_distance,
        stop_loss_price: stop_distance.map(|d| price(d, true)),
        take_profit_price: take_profit_distance.map(|d| price(d, false)),
        risk_amount,
        risk_percent,
        account_currency: ctx.account_currency.map(str::to_owned),
        spread,
        specs_source: instrument.specs_source,
        warnings,
        created_at: now_millis(),
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::domain::{SpecsSource, VolumeSpecs};

    fn eurusd() -> Instrument {
        Instrument {
            symbol: "EURUSD".into(),
            symbol_id: Some(1),
            price_digits: 5,
            pip_size: 0.0001,
            base_currency: Some("EUR".into()),
            quote_currency: Some("USD".into()),
            enabled: true,
            volume: VolumeSpecs {
                lot_size: 100_000.0,
                min: Volume::from_units(1_000),
                step: Volume::from_units(1_000),
                max: Some(Volume::from_units(100_000_000)),
            },
            specs_source: SpecsSource::Broker,
        }
    }

    fn quote(bid: f64, ask: f64) -> Quote {
        Quote {
            symbol: "EURUSD".into(),
            bid,
            ask,
            timestamp: None,
        }
    }

    fn plan(
        intent: &EntryIntent,
        instrument: &Instrument,
        q: &Quote,
        balance: f64,
        rate: Option<f64>,
    ) -> Result<OrderPlan> {
        let account = AccountId::new("a");
        build_plan(
            intent,
            &PlanContext {
                account: &account,
                account_currency: Some("USD"),
                instrument,
                quote: q,
                balance: Some(balance),
                conversion_rate: rate,
                stop_granularity: 0.00001,
            },
        )
    }

    fn buy_risk(pct: f64, sl_pips: f64) -> EntryIntent {
        EntryIntent {
            symbol: "EURUSD".into(),
            side: Side::Buy,
            size: SizeSpec::Risk(RiskSpec::PercentOfBalance(pct)),
            stop_loss: Some(StopSpec::Pips(sl_pips)),
            take_profit: Some(TakeProfitSpec::RiskReward(2.0)),
        }
    }

    #[test]
    fn one_percent_with_a_thirty_pip_stop() {
        let p = plan(
            &buy_risk(1.0, 30.0),
            &eurusd(),
            &quote(1.08499, 1.08501),
            10_000.0,
            Some(1.0),
        )
        .unwrap();
        // 100 USD / (0.0030 * 1) = 33,333 units, rounded down to the 1,000 step.
        assert_eq!(p.volume, Volume::from_units(33_000));
        assert!(
            (p.risk_amount.unwrap() - 99.0).abs() < 1e-6,
            "{:?}",
            p.risk_amount
        );
        assert!(p.risk_amount.unwrap() <= 100.0);
        assert_eq!(p.entry_reference, 1.08501);
        assert!((p.stop_loss_price.unwrap() - 1.08201).abs() < 1e-9);
        assert!(
            (p.take_profit_price.unwrap() - 1.09101).abs() < 1e-9,
            "2:1 reward"
        );
        assert!((p.risk_percent.unwrap() - 0.99).abs() < 1e-6);
    }

    #[test]
    fn a_sell_uses_the_bid_and_mirrors_the_levels() {
        let mut intent = buy_risk(1.0, 30.0);
        intent.side = Side::Sell;
        let p = plan(
            &intent,
            &eurusd(),
            &quote(1.08499, 1.08501),
            10_000.0,
            Some(1.0),
        )
        .unwrap();
        assert_eq!(p.entry_reference, 1.08499);
        assert!((p.stop_loss_price.unwrap() - 1.08799).abs() < 1e-9);
        assert!((p.take_profit_price.unwrap() - 1.07899).abs() < 1e-9);
    }

    #[test]
    fn agrees_with_the_ctrader_mcp_sizing_math_on_whole_pip_stops() {
        use ctrader_mcp::math::sizing::{SizingParams, from_risk_percent};
        let reference = from_risk_percent(
            10_000.0,
            1.0,
            &SizingParams {
                sl_pips: 30,
                pip_value_per_lot: 10.0,
                conversion_rate: 1.0,
                lot_size: 100_000.0,
                min_volume_step: 1_000.0,
                min_volume: 1_000.0,
                max_volume: None,
            },
        )
        .unwrap();
        let p = plan(
            &buy_risk(1.0, 30.0),
            &eurusd(),
            &quote(1.08499, 1.08501),
            10_000.0,
            Some(1.0),
        )
        .unwrap();
        assert_eq!(p.volume.units(), reference.units);
    }

    #[test]
    fn a_stop_price_on_the_wrong_side_is_rejected() {
        let mut intent = buy_risk(1.0, 30.0);
        intent.stop_loss = Some(StopSpec::Price(1.09000));
        let err = plan(
            &intent,
            &eurusd(),
            &quote(1.08499, 1.08501),
            10_000.0,
            Some(1.0),
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::Invalid(m) if m.contains("below")));
    }

    #[test]
    fn a_risk_too_small_for_the_minimum_volume_says_what_the_minimum_risks() {
        let err = plan(
            &buy_risk(0.001, 30.0),
            &eurusd(),
            &quote(1.08499, 1.08501),
            10_000.0,
            Some(1.0),
        )
        .unwrap_err();
        assert!(
            matches!(err, EngineError::Invalid(ref m) if m.contains("too small") && m.contains("minimum volume")),
            "{err:?}"
        );
    }

    #[test]
    fn risk_sizing_without_a_conversion_rate_or_stop_or_balance_fails_clearly() {
        let q = quote(1.08499, 1.08501);
        let no_rate = plan(&buy_risk(1.0, 30.0), &eurusd(), &q, 10_000.0, None).unwrap_err();
        assert!(matches!(no_rate, EngineError::Invalid(m) if m.contains("exchange rate")));
        let mut no_stop = buy_risk(1.0, 30.0);
        no_stop.stop_loss = None;
        no_stop.take_profit = None;
        assert!(plan(&no_stop, &eurusd(), &q, 10_000.0, Some(1.0)).is_err());
        assert!(plan(&buy_risk(1.0, 30.0), &eurusd(), &q, 0.0, Some(1.0)).is_err());
    }

    #[test]
    fn fixed_volume_is_validated_against_the_instrument() {
        let q = quote(1.08499, 1.08501);
        let mut intent = buy_risk(1.0, 30.0);
        intent.size = SizeSpec::Fixed(Volume::from_units(10_500));
        assert!(
            plan(&intent, &eurusd(), &q, 10_000.0, Some(1.0)).is_err(),
            "not a multiple of the step"
        );
        intent.size = SizeSpec::Fixed(Volume::from_units(10_000));
        let p = plan(&intent, &eurusd(), &q, 10_000.0, Some(1.0)).unwrap();
        assert_eq!(p.volume, Volume::from_units(10_000));
        assert!((p.risk_amount.unwrap() - 30.0).abs() < 1e-6);
    }

    #[test]
    fn unusable_quotes_and_disabled_instruments_are_refused() {
        let mut i = eurusd();
        i.enabled = false;
        assert!(
            plan(
                &buy_risk(1.0, 30.0),
                &i,
                &quote(1.0, 1.1),
                10_000.0,
                Some(1.0)
            )
            .is_err()
        );
        assert!(
            plan(
                &buy_risk(1.0, 30.0),
                &eurusd(),
                &quote(1.1, 1.0),
                10_000.0,
                Some(1.0)
            )
            .is_err()
        );
    }

    #[test]
    fn warnings_cover_assumptions_wide_spreads_and_missing_stops() {
        let mut i = eurusd();
        i.specs_source = SpecsSource::Assumed;
        let p = plan(
            &buy_risk(1.0, 30.0),
            &i,
            &quote(1.0850, 1.0862),
            10_000.0,
            Some(1.0),
        )
        .unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("assumed")));
        assert!(p.warnings.iter().any(|w| w.contains("spread")));
        let mut intent = buy_risk(1.0, 30.0);
        intent.size = SizeSpec::Fixed(Volume::from_units(1_000));
        intent.stop_loss = None;
        intent.take_profit = None;
        let p = plan(&intent, &eurusd(), &quote(1.08499, 1.08501), 10_000.0, None).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("no stop loss")));
    }

    #[test]
    fn a_stop_is_rounded_up_to_what_the_broker_can_express() {
        let account = AccountId::new("a");
        let i = eurusd();
        let q = quote(1.08499, 1.08501);
        let p = build_plan(
            &buy_risk(1.0, 30.0),
            &PlanContext {
                account: &account,
                account_currency: Some("USD"),
                instrument: &i,
                quote: &q,
                balance: Some(10_000.0),
                conversion_rate: Some(1.0),
                stop_granularity: 0.0001, // whole pips only, like Local
            },
        )
        .unwrap();
        assert!((p.stop_loss_distance.unwrap() - 0.0030).abs() < 1e-12);
        let fractional = EntryIntent {
            stop_loss: Some(StopSpec::Pips(30.2)),
            ..buy_risk(1.0, 30.0)
        };
        let p = build_plan(
            &fractional,
            &PlanContext {
                account: &account,
                account_currency: Some("USD"),
                instrument: &i,
                quote: &q,
                balance: Some(10_000.0),
                conversion_rate: Some(1.0),
                stop_granularity: 0.0001,
            },
        )
        .unwrap();
        assert!(
            (p.stop_loss_distance.unwrap() - 0.0031).abs() < 1e-12,
            "30.2 pips rounds up to 31"
        );
    }

    proptest! {
        /// The core safety property: whatever the inputs, the real risk never exceeds the
        /// requested risk, and the volume is always a valid multiple of the step.
        #[test]
        fn risk_never_exceeds_the_target_and_volume_is_valid(
            balance in 500.0f64..1_000_000.0,
            pct in 0.05f64..5.0,
            sl_pips in 1.0f64..500.0,
            rate in 0.2f64..5.0,
        ) {
            let intent = buy_risk(pct, sl_pips);
            let q = quote(1.08499, 1.08501);
            let i = eurusd();
            if let Ok(p) = plan(&intent, &i, &q, balance, Some(rate)) {
                let target = balance * pct / 100.0;
                prop_assert!(p.risk_amount.unwrap() <= target + 1e-6, "risk {} > target {}", p.risk_amount.unwrap(), target);
                prop_assert!(i.check_volume(p.volume).is_ok());
                // And not wastefully far below: within one step's worth of risk.
                let step_risk = i.volume.step.as_units() * p.stop_loss_distance.unwrap() * rate;
                prop_assert!(p.risk_amount.unwrap() > target - step_risk - 1e-6 || p.volume == i.volume.max.unwrap());
            }
        }
    }
}
