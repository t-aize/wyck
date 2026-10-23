//! The small computations behind the account panel that do not need the account: the average
//! entry of several positions, and how a moment is written.

use wyck_domain::base::math::i64_to_f64;

/// The average of prices weighted by volume, from `(price, volume)` pairs. `None` when there is
/// no volume.
pub(crate) fn weighted_entry(fills: &[(f64, i64)]) -> Option<f64> {
    let total: f64 = fills.iter().map(|(_, v)| i64_to_f64(*v)).sum();
    if total <= 0.0 {
        return None;
    }
    Some(fills.iter().map(|(p, v)| p * i64_to_f64(*v)).sum::<f64>() / total)
}

/// A moment written as `Sep 25 16:35:12`. `shifted_ms` is already in the zone the user reads.
pub(crate) fn format_time(shifted_ms: i64) -> String {
    let Some(t) = chrono::DateTime::from_timestamp(shifted_ms.div_euclid(1_000), 0) else {
        return String::new();
    };
    t.format("%b %-d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_average_entry_is_weighted_by_volume() {
        assert_eq!(weighted_entry(&[]), None);
        assert_eq!(weighted_entry(&[(1.0, 0)]), None);
        let average = weighted_entry(&[(1.0, 100), (2.0, 300)]).unwrap();
        assert!((average - 1.75).abs() < 1e-12);
    }

    #[test]
    fn a_time_is_written_with_the_month_first() {
        // 2026-09-25 16:35:12 UTC.
        assert_eq!(format_time(1_790_354_112_000), "Sep 25 16:35:12");
    }
}
