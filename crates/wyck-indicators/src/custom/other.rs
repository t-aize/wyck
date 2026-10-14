//! The prices of a second symbol, read by a script with `bars_of`.
//!
//! A script names the symbol with an input of type symbol (`input_symbol`). The app loads the bars
//! of that symbol on the timeframe of the chart and hands them to the run, which lines them up on
//! the bars of the chart: `Aligned` holds one number per bar of the chart.

use std::collections::BTreeMap;

use super::series::Series;
use crate::compute::StudyInput;
use wyck_market::Bar;

/// The bars of another symbol, as they came from the server: one value per bar, oldest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OtherBars {
    pub time: Vec<i64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<f64>,
}

impl OtherBars {
    /// The bars a server gave, with prices as the chart holds them (raw units).
    #[must_use]
    pub fn from_bars(bars: &[Bar]) -> Self {
        Self {
            time: bars.iter().map(|b| b.time_ms).collect(),
            open: bars.iter().map(|b| b.open as f64).collect(),
            high: bars.iter().map(|b| b.high as f64).collect(),
            low: bars.iter().map(|b| b.low as f64).collect(),
            close: bars.iter().map(|b| b.close as f64).collect(),
            volume: bars.iter().map(|b| b.volume as f64).collect(),
        }
    }

    /// The bars a chart feeds its indicators.
    #[must_use]
    pub fn from_input(input: &StudyInput) -> Self {
        Self {
            time: input.time.clone(),
            open: input.open.clone(),
            high: input.high.clone(),
            low: input.low.clone(),
            close: input.close.clone(),
            volume: input.volume.clone(),
        }
    }
}

/// The key a symbol name is kept under: the same name in any case, with no spaces around.
#[must_use]
pub fn key_of(name: &str) -> String {
    name.trim().to_uppercase()
}

/// The bars of other symbols, each under the [`key_of`] its name.
pub type OtherSymbols = BTreeMap<String, OtherBars>;

/// The prices of another symbol on the bars of the chart.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Aligned {
    pub open: Series,
    pub high: Series,
    pub low: Series,
    pub close: Series,
    pub volume: Series,
}

impl Aligned {
    /// No bars at all, for the run that declares the indicator.
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    /// Lines `other` up on the bar times `times` (both oldest first). A bar of the chart takes the
    /// bar of the other symbol that opened at the same time. When the other symbol has none
    /// there (a market that was closed), the bar is flat at the last close it had and has no
    /// volume; before its first bar the values are `NaN`.
    pub(crate) fn on(times: &[i64], other: &OtherBars) -> Self {
        let n = times.len();
        let (mut open, mut high, mut low, mut close, mut volume) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        let have = other
            .time
            .len()
            .min(other.open.len())
            .min(other.high.len())
            .min(other.low.len())
            .min(other.close.len())
            .min(other.volume.len());
        // The first bar of `other` that opens after the chart bar being looked at.
        let mut next = 0;
        for &t in times {
            while next < have && other.time[next] <= t {
                next += 1;
            }
            match next.checked_sub(1) {
                None => {
                    for column in [&mut open, &mut high, &mut low, &mut close, &mut volume] {
                        column.push(f64::NAN);
                    }
                }
                Some(j) if other.time[j] == t => {
                    open.push(other.open[j]);
                    high.push(other.high[j]);
                    low.push(other.low[j]);
                    close.push(other.close[j]);
                    volume.push(other.volume[j]);
                }
                Some(j) => {
                    let flat = other.close[j];
                    for column in [&mut open, &mut high, &mut low, &mut close] {
                        column.push(flat);
                    }
                    volume.push(0.0);
                }
            }
        }
        Self {
            open: Series::new(open),
            high: Series::new(high),
            low: Series::new(low),
            close: Series::new(close),
            volume: Series::new(volume),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn other() -> OtherBars {
        OtherBars {
            time: vec![60, 120, 240],
            open: vec![1.0, 2.0, 4.0],
            high: vec![1.5, 2.5, 4.5],
            low: vec![0.5, 1.5, 3.5],
            close: vec![1.2, 2.2, 4.2],
            volume: vec![10.0, 20.0, 40.0],
        }
    }

    #[test]
    fn a_bar_takes_the_bar_that_opened_with_it() {
        let aligned = Aligned::on(&[60, 120], &other());
        assert_eq!(aligned.close.values(), &[1.2, 2.2]);
        assert_eq!(aligned.high.values(), &[1.5, 2.5]);
        assert_eq!(aligned.volume.values(), &[10.0, 20.0]);
    }

    #[test]
    fn a_missing_bar_is_flat_at_the_last_close_and_a_bar_before_the_first_is_empty() {
        let aligned = Aligned::on(&[0, 60, 180, 240], &other());
        assert!(aligned.close.values()[0].is_nan());
        assert!(aligned.volume.values()[0].is_nan());
        assert_eq!(aligned.close.values()[2], 2.2);
        assert_eq!(aligned.open.values()[2], 2.2);
        assert_eq!(aligned.high.values()[2], 2.2);
        assert_eq!(aligned.volume.values()[2], 0.0);
        assert_eq!(aligned.close.values()[3], 4.2);
    }

    #[test]
    fn columns_of_different_lengths_do_not_panic() {
        let mut bars = other();
        bars.volume.pop();
        let aligned = Aligned::on(&[60, 120, 240], &bars);
        assert_eq!(aligned.close.values()[2], 2.2);
    }

    #[test]
    fn bars_from_a_server_keep_their_raw_prices() {
        let bar = Bar {
            time_ms: 60,
            open: 100_000,
            high: 110_000,
            low: 90_000,
            close: 105_000,
            volume: 7,
        };
        let bars = OtherBars::from_bars(&[bar]);
        assert_eq!(bars.time, [60]);
        assert_eq!(bars.high, [110_000.0]);
        assert_eq!(bars.volume, [7.0]);
    }

    #[test]
    fn a_name_is_kept_in_one_case_without_spaces() {
        assert_eq!(key_of(" eurUSD "), "EURUSD");
    }
}
