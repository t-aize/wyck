//! The volume profile of the bars on screen: how much volume traded at each price.
//!
//! The range from the lowest low to the highest high of the visible bars is cut into rows. Each
//! bar spreads its volume evenly over the rows its own range touches, in proportion to how much
//! of each row it covers, and counts as up volume when it closed at or above its open. The row
//! with the most volume is the point of control; the value area grows from it one row at a time
//! toward the side with more volume until it holds the share asked for (70 percent by default).
//!
//! The volume here is the tick volume the Open API gives (price changes, not traded amounts),
//! which is what every spot forex and CFD chart works with.

/// One row of the profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    pub lo: f64,
    pub hi: f64,
    pub up: f64,
    pub down: f64,
}

impl Row {
    pub fn total(&self) -> f64 {
        self.up + self.down
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub rows: Vec<Row>,
    /// The row with the most volume.
    pub poc: usize,
    /// The rows of the value area, `first..=last`.
    pub value_area: (usize, usize),
    pub max_total: f64,
}

/// A bar as the profile reads it.
#[derive(Debug, Clone, Copy)]
pub struct Slice {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Builds the profile of `bars` in `rows` rows, with a value area holding `value_area` (0 to 1) of
/// the volume. `None` when there is nothing to profile.
pub fn build(bars: &[Slice], rows: usize, value_area: f64) -> Option<Profile> {
    let rows = rows.max(1);
    let lo = bars
        .iter()
        .map(|b| b.low)
        .filter(|v| v.is_finite())
        .reduce(f64::min)?;
    let hi = bars
        .iter()
        .map(|b| b.high)
        .filter(|v| v.is_finite())
        .reduce(f64::max)?;
    let span = (hi - lo).max(f64::EPSILON);
    let height = span / rows as f64;
    let mut out: Vec<Row> = (0..rows)
        .map(|i| Row {
            lo: lo + height * i as f64,
            hi: lo + height * (i + 1) as f64,
            up: 0.0,
            down: 0.0,
        })
        .collect();
    for bar in bars {
        if !(bar.volume > 0.0 && bar.low.is_finite() && bar.high.is_finite()) {
            continue;
        }
        let up = bar.close >= bar.open;
        let first = (((bar.low - lo) / height).floor().max(0.0) as usize).min(rows - 1);
        let last = (((bar.high - lo) / height).floor().max(0.0) as usize).min(rows - 1);
        let range = bar.high - bar.low;
        for row in out.iter_mut().take(last + 1).skip(first) {
            // A bar with no range puts all of its volume in its one row.
            let share = if range <= 0.0 {
                1.0 / (last - first + 1) as f64
            } else {
                ((bar.high.min(row.hi) - bar.low.max(row.lo)) / range).max(0.0)
            };
            if up {
                row.up += bar.volume * share;
            } else {
                row.down += bar.volume * share;
            }
        }
    }
    let total: f64 = out.iter().map(Row::total).sum();
    if total <= 0.0 {
        return None;
    }
    let poc = out
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total().total_cmp(&b.1.total()))
        .map_or(0, |(i, _)| i);
    let target = total * value_area.clamp(0.0, 1.0);
    let (mut first, mut last) = (poc, poc);
    let mut held = out[poc].total();
    while held < target && (first > 0 || last + 1 < rows) {
        let below = (first > 0).then(|| out[first - 1].total());
        let above = (last + 1 < rows).then(|| out[last + 1].total());
        match (below, above) {
            (Some(b), Some(a)) if a >= b => {
                last += 1;
                held += a;
            }
            (Some(b), _) => {
                first -= 1;
                held += b;
            }
            (None, Some(a)) => {
                last += 1;
                held += a;
            }
            (None, None) => break,
        }
    }
    let max_total = out.iter().map(Row::total).fold(0.0, f64::max);
    Some(Profile {
        rows: out,
        poc,
        value_area: (first, last),
        max_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(low: f64, high: f64, up: bool, volume: f64) -> Slice {
        Slice {
            open: if up { low } else { high },
            close: if up { high } else { low },
            high,
            low,
            volume,
        }
    }

    #[test]
    fn volume_is_spread_over_the_rows_a_bar_covers() {
        let bars = [bar(0.0, 10.0, true, 100.0), bar(0.0, 5.0, false, 50.0)];
        let profile = build(&bars, 2, 0.7).unwrap();
        assert_eq!(profile.rows.len(), 2);
        // The first bar splits 50/50, the second is all in the lower row.
        assert!((profile.rows[0].up - 50.0).abs() < 1e-9);
        assert!((profile.rows[0].down - 50.0).abs() < 1e-9);
        assert!((profile.rows[1].up - 50.0).abs() < 1e-9);
        assert_eq!(profile.poc, 0);
        let total: f64 = profile.rows.iter().map(Row::total).sum();
        assert!((total - 150.0).abs() < 1e-9, "no volume is lost");
    }

    #[test]
    fn the_value_area_grows_from_the_point_of_control() {
        // Volumes by row: 1, 2, 10, 3, 1.
        let volumes = [1.0, 2.0, 10.0, 3.0, 1.0];
        let bars: Vec<Slice> = volumes
            .iter()
            .enumerate()
            .map(|(i, v)| bar(i as f64 + 0.1, i as f64 + 0.9, true, *v))
            .collect();
        let mut with_edges = bars.clone();
        with_edges.push(bar(0.0, 0.0, true, 0.0));
        with_edges.push(bar(5.0, 5.0, true, 0.0));
        let profile = build(&with_edges, 5, 0.7).unwrap();
        assert_eq!(profile.poc, 2);
        // 10 of 17 is under 70%; adding the 3 above makes 13 (76%).
        assert_eq!(profile.value_area, (2, 3));
        assert_eq!(profile.max_total, 10.0);
    }

    #[test]
    fn nothing_to_profile_gives_nothing() {
        assert!(build(&[], 10, 0.7).is_none());
        assert!(build(&[bar(1.0, 2.0, true, 0.0)], 10, 0.7).is_none());
    }

    #[test]
    fn a_flat_bar_lands_in_one_row() {
        let profile = build(&[bar(5.0, 5.0, true, 7.0)], 3, 0.7).unwrap();
        let total: f64 = profile.rows.iter().map(Row::total).sum();
        assert!((total - 7.0).abs() < 1e-9);
    }
}
