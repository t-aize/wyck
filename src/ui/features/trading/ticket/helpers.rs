//! The small arithmetic and formats the ticket shares.

use crate::domain::trading::math::{Contract, Offset, SizeMode};
use crate::ui::kit::number;

/// The margin of a side: the buy one or the sell one.
pub(super) fn side_of(margins: (f64, f64), buy: bool) -> f64 {
    if buy { margins.0 } else { margins.1 }
}

/// A distance in pips as the points the server counts: the smallest step of the price.
pub(super) fn slippage_points(pips: f64, contract: &Contract) -> i32 {
    let digits = i32::try_from(contract.digits).unwrap_or(5);
    (pips * contract.pip() * 10f64.powi(digits))
        .round()
        .max(0.0) as i32
}

/// Where the limit of a stop limit sits: past the stop by the range, in the direction the order
/// trades.
pub(super) fn stop_limit_price(buy: bool, stop: f64, range: f64) -> f64 {
    if buy { stop + range } else { stop - range }
}

/// What the size field holds in a mode.
pub(super) fn size_kind(mode: SizeMode, contract: &Contract) -> number::Kind {
    match mode {
        SizeMode::Lots => number::Kind::Volume {
            step: contract.lots_of_volume(contract.step_volume.max(1)),
        },
        SizeMode::Units => number::Kind::Volume {
            step: contract.step_volume.max(1) as f64 / 100.0,
        },
        SizeMode::RiskBalance | SizeMode::RiskEquity | SizeMode::FreeMargin => {
            number::Kind::Percent
        }
        SizeMode::RiskMoney => number::Kind::Money,
    }
}

/// What a protection field holds in its unit.
pub(super) fn offset_kind(unit: Offset, contract: &Contract) -> number::Kind {
    match unit {
        Offset::Price => number::Kind::Price {
            step: contract.pip(),
        },
        Offset::Pips => number::Kind::Pips,
        Offset::Money => number::Kind::Money,
        Offset::Percent => number::Kind::Percent,
        Offset::Ratio => number::Kind::Ratio,
    }
}

/// A value of a protection written for its unit.
pub(super) fn format_offset(unit: Offset, value: f64) -> String {
    let decimals = match unit {
        Offset::Pips => 1,
        _ => 2,
    };
    number::format(value, decimals)
}

/// An amount rounded to two significant digits, for a preset: 97.3 is 97, 1 234 is 1 200.
pub(super) fn nice(amount: f64) -> f64 {
    if !amount.is_finite() || amount <= 0.0 {
        return 10.0;
    }
    let magnitude = 10f64.powi(amount.log10().floor() as i32 - 1);
    ((amount / magnitude).round() * magnitude).max(1.0)
}
