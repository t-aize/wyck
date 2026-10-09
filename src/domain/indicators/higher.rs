//! Reading the bars of the chart at a higher timeframe.
//!
//! An indicator that works on a higher timeframe (the 4 hour average on a 15 minute chart) needs
//! the chart's bars grouped into bigger ones, and a way back from a chart bar to the bigger bar
//! that holds it. Nothing is fetched: the bigger bars are built from the bars already loaded, so
//! they reach back as far as the chart does.

use crate::domain::chart::timeframe::Timeframe;

use super::StudyInput;

/// The chart's bars grouped into the bars of a higher timeframe.
#[derive(Debug, Clone, PartialEq)]
pub struct Higher {
    /// The grouped bars.
    pub input: StudyInput,
    /// For each bar of the chart, the position of the grouped bar that holds it.
    pub of: Vec<usize>,
    /// For each bar of the chart, the grouped bar that holds it as it was at that moment: its
    /// open, and its high, low, close and volume up to and including that chart bar.
    pub forming: StudyInput,
}

/// How long a bar of `timeframe` lasts, in milliseconds, for the timeframes made of bars.
fn span(timeframe: Timeframe) -> Option<i64> {
    match timeframe {
        Timeframe::Bars(period) => Some(period.millis()),
        Timeframe::Multiple(period, n) => Some(period.millis() * i64::from(n)),
        Timeframe::Seconds(s) => Some(i64::from(s) * 1000),
        Timeframe::Ticks => None,
    }
}

/// Which group a time belongs to: the same number for every time of one grouped bar.
fn key(timeframe: Timeframe, time: i64) -> i64 {
    match timeframe {
        Timeframe::Bars(period) => Timeframe::Multiple(period, 1).group_key(time),
        Timeframe::Seconds(s) => time.div_euclid(i64::from(s) * 1000),
        other => other.group_key(time),
    }
}

/// When the grouped bar holding a bar opening at `time` opens.
fn open(timeframe: Timeframe, time: i64) -> i64 {
    match timeframe {
        Timeframe::Bars(period) => Timeframe::Multiple(period, 1)
            .group_open(time)
            .unwrap_or(time),
        Timeframe::Seconds(s) => time - time.rem_euclid(i64::from(s) * 1000),
        other => other.group_open(time).unwrap_or(time),
    }
}

/// The smallest gap between two bars of the chart, which is its timeframe when no bar is missing.
fn step(input: &StudyInput) -> Option<i64> {
    input
        .time
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|gap| *gap > 0)
        .min()
}

/// Groups the chart's bars into the bars of `target`.
///
/// `None` when `target` is not higher than the chart's own timeframe (it would group nothing), or
/// is made of ticks.
pub fn higher(input: &StudyInput, target: Timeframe) -> Option<Higher> {
    let length = span(target)?;
    if step(input).is_some_and(|step| length <= step) {
        return None;
    }
    let mut grouped = StudyInput {
        time: Vec::new(),
        open: Vec::new(),
        high: Vec::new(),
        low: Vec::new(),
        close: Vec::new(),
        volume: Vec::new(),
        day: Vec::new(),
    };
    let mut of = Vec::with_capacity(input.len());
    let mut forming = StudyInput {
        time: input.time.clone(),
        open: Vec::with_capacity(input.len()),
        high: Vec::with_capacity(input.len()),
        low: Vec::with_capacity(input.len()),
        close: Vec::with_capacity(input.len()),
        volume: Vec::with_capacity(input.len()),
        day: input.day.clone(),
    };
    let mut last_key = None;
    for i in 0..input.len() {
        let group = key(target, input.time[i]);
        if last_key == Some(group) {
            let at = grouped.close.len() - 1;
            grouped.high[at] = grouped.high[at].max(input.high[i]);
            grouped.low[at] = grouped.low[at].min(input.low[i]);
            grouped.close[at] = input.close[i];
            grouped.volume[at] += input.volume[i];
        } else {
            last_key = Some(group);
            grouped.time.push(open(target, input.time[i]));
            grouped.open.push(input.open[i]);
            grouped.high.push(input.high[i]);
            grouped.low.push(input.low[i]);
            grouped.close.push(input.close[i]);
            grouped.volume.push(input.volume[i]);
            grouped.day.push(input.day[i]);
        }
        let at = grouped.close.len() - 1;
        of.push(at);
        forming.open.push(grouped.open[at]);
        forming.high.push(grouped.high[at]);
        forming.low.push(grouped.low[at]);
        forming.close.push(grouped.close[at]);
        forming.volume.push(grouped.volume[at]);
    }
    Some(Higher {
        input: grouped,
        of,
        forming,
    })
}

/// Carries a value computed per grouped bar back to the bars of the chart.
///
/// With `confirmed`, a chart bar gets the value of the last grouped bar that has closed, so a
/// finished chart never shows a number that was not known yet (the first grouped bar gives no
/// value). Without it, a chart bar gets the value of the grouped bar still forming, which
/// changes until that bar closes.
pub fn carry_back(values: &[f64], of: &[usize], confirmed: bool) -> Vec<f64> {
    of.iter()
        .map(|&at| {
            if confirmed {
                at.checked_sub(1).map_or(f64::NAN, |before| values[before])
            } else {
                values[at]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minute bars from midnight UTC, `n` of them, closing at their position.
    fn minutes(n: usize) -> StudyInput {
        StudyInput {
            time: (0..n as i64).map(|i| i * 60_000).collect(),
            open: (0..n).map(|i| i as f64).collect(),
            high: (0..n).map(|i| i as f64 + 0.5).collect(),
            low: (0..n).map(|i| i as f64 - 0.5).collect(),
            close: (0..n).map(|i| i as f64 + 0.25).collect(),
            volume: vec![1.0; n],
            day: vec![0; n],
        }
    }

    #[test]
    fn minutes_group_into_five_minute_bars() {
        let five = Timeframe::from_code("5m").unwrap();
        let h = higher(&minutes(12), five).unwrap();
        assert_eq!(h.input.len(), 3);
        assert_eq!(h.input.time, vec![0, 300_000, 600_000]);
        assert_eq!(h.of, vec![0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2]);
        // The first grouped bar opens with minute 0 and closes with minute 4.
        assert_eq!(h.input.open[0], 0.0);
        assert_eq!(h.input.close[0], 4.25);
        assert_eq!(h.input.high[0], 4.5);
        assert_eq!(h.input.low[0], -0.5);
        assert_eq!(h.input.volume[0], 5.0);
        // The last one is still forming: two minutes so far.
        assert_eq!(h.input.volume[2], 2.0);
    }

    #[test]
    fn each_chart_bar_knows_its_grouped_bar_as_it_was_then() {
        let five = Timeframe::from_code("5m").unwrap();
        let h = higher(&minutes(8), five).unwrap();
        // At minute 2 the first grouped bar has seen minutes 0 to 2 only.
        assert_eq!(h.forming.close[2], 2.25);
        assert_eq!(h.forming.high[2], 2.5);
        assert_eq!(h.forming.low[2], -0.5);
        assert_eq!(h.forming.volume[2], 3.0);
        // At minute 6 the second one has seen minutes 5 and 6.
        assert_eq!(h.forming.open[6], 5.0);
        assert_eq!(h.forming.close[6], 6.25);
        assert_eq!(h.forming.volume[6], 2.0);
    }

    #[test]
    fn a_timeframe_that_is_not_higher_groups_nothing() {
        let one = Timeframe::from_code("1m").unwrap();
        assert!(higher(&minutes(10), one).is_none());
        assert!(higher(&minutes(10), Timeframe::Ticks).is_none());
    }

    #[test]
    fn a_value_reaches_the_chart_only_once_its_bar_has_closed() {
        let of = [0, 0, 1, 1, 2];
        let values = [10.0, 20.0, 30.0];
        let confirmed = carry_back(&values, &of, true);
        assert!(confirmed[0].is_nan() && confirmed[1].is_nan());
        assert_eq!(&confirmed[2..], &[10.0, 10.0, 20.0]);
        assert_eq!(
            carry_back(&values, &of, false),
            vec![10.0, 10.0, 20.0, 20.0, 30.0]
        );
    }

    #[test]
    fn hours_group_from_the_start_of_the_day() {
        let h = Timeframe::from_code("1h").unwrap();
        let grouped = higher(&minutes(130), h).unwrap();
        assert_eq!(grouped.input.len(), 3);
        assert_eq!(grouped.input.time, vec![0, 3_600_000, 7_200_000]);
    }
}
