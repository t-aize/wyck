//! The small computations behind the account panel that do not need the account: how long a
//! position has been open, the average entry of several positions, the figures of the history,
//! what a search matches, and how a table is written as CSV.

use super::prefs::TimeStyle;

/// A span of time as the two largest units it has: `45s`, `12m 30s`, `3h 12m`, `2d 4h`.
pub fn duration_text(ms: i64) -> String {
    let seconds = (ms / 1_000).max(0);
    let (days, hours, minutes, secs) = (
        seconds / 86_400,
        seconds % 86_400 / 3_600,
        seconds % 3_600 / 60,
        seconds % 60,
    );
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// The average of prices weighted by volume, from `(price, volume)` pairs. `None` when there is
/// no volume.
pub fn weighted_entry(fills: &[(f64, i64)]) -> Option<f64> {
    let total: f64 = fills.iter().map(|(_, v)| *v as f64).sum();
    if total <= 0.0 {
        return None;
    }
    Some(fills.iter().map(|(p, v)| p * *v as f64).sum::<f64>() / total)
}

/// What the closed trades came to.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HistoryStats {
    pub closed: usize,
    pub wins: usize,
    pub losses: usize,
    pub total: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub best: f64,
    pub worst: f64,
}

impl HistoryStats {
    /// The figures of trades that ended with these results, each after costs.
    pub fn of(results: &[f64]) -> Self {
        let mut stats = Self::default();
        for &result in results {
            stats.closed += 1;
            stats.total += result;
            if result > 0.0 {
                stats.wins += 1;
                stats.gross_profit += result;
            } else if result < 0.0 {
                stats.losses += 1;
                stats.gross_loss += -result;
            }
            stats.best = stats.best.max(result);
            stats.worst = stats.worst.min(result);
        }
        stats
    }

    /// Wins over all the closed trades, in percent.
    pub fn win_rate(&self) -> Option<f64> {
        (self.closed > 0).then(|| self.wins as f64 / self.closed as f64 * 100.0)
    }

    /// What the winners made for each unit the losers lost. `None` with no loss.
    pub fn profit_factor(&self) -> Option<f64> {
        (self.gross_loss > 0.0).then(|| self.gross_profit / self.gross_loss)
    }

    pub fn average_win(&self) -> Option<f64> {
        (self.wins > 0).then(|| self.gross_profit / self.wins as f64)
    }

    pub fn average_loss(&self) -> Option<f64> {
        (self.losses > 0).then(|| -self.gross_loss / self.losses as f64)
    }

    /// What a trade makes on average.
    pub fn expectancy(&self) -> Option<f64> {
        (self.closed > 0).then(|| self.total / self.closed as f64)
    }
}

/// Whether a search matches: every word of it is found, in any case, in one of the texts.
pub fn matches(query: &str, texts: &[&str]) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let texts: Vec<String> = texts.iter().map(|t| t.to_lowercase()).collect();
    query
        .split_whitespace()
        .all(|word| texts.iter().any(|t| t.contains(word)))
}

/// A field of a CSV line: quoted when it holds a comma, a quote or a line break.
pub fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// A CSV line from its fields.
pub fn csv_line<'a>(fields: impl IntoIterator<Item = &'a str>) -> String {
    fields
        .into_iter()
        .map(csv_field)
        .collect::<Vec<_>>()
        .join(",")
}

/// A moment written in a style. `shifted_ms` is already in the zone the user reads.
pub fn format_time(shifted_ms: i64, style: TimeStyle) -> String {
    let Ok(t) = time::OffsetDateTime::from_unix_timestamp(shifted_ms.div_euclid(1_000)) else {
        return String::new();
    };
    match style {
        TimeStyle::Short => {
            let month = t.month().to_string();
            format!(
                "{} {} {:02}:{:02}:{:02}",
                &month[..3.min(month.len())],
                t.day(),
                t.hour(),
                t.minute(),
                t.second()
            )
        }
        TimeStyle::Iso => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second()
        ),
        TimeStyle::Clock => format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_is_written_with_its_two_largest_units() {
        assert_eq!(duration_text(45_000), "45s");
        assert_eq!(duration_text(12 * 60_000 + 30_000), "12m 30s");
        assert_eq!(duration_text(3 * 3_600_000 + 12 * 60_000), "3h 12m");
        assert_eq!(duration_text(2 * 86_400_000 + 4 * 3_600_000), "2d 4h");
        assert_eq!(duration_text(-5_000), "0s");
    }

    #[test]
    fn an_average_entry_is_weighted_by_volume() {
        assert_eq!(weighted_entry(&[]), None);
        assert_eq!(weighted_entry(&[(1.0, 0)]), None);
        let average = weighted_entry(&[(1.0, 100), (2.0, 300)]).unwrap();
        assert!((average - 1.75).abs() < 1e-12);
    }

    #[test]
    fn the_history_figures_add_up() {
        let stats = HistoryStats::of(&[100.0, -50.0, 30.0, -20.0, 0.0]);
        assert_eq!((stats.closed, stats.wins, stats.losses), (5, 2, 2));
        assert!((stats.total - 60.0).abs() < 1e-9);
        assert!((stats.win_rate().unwrap() - 40.0).abs() < 1e-9);
        assert!((stats.profit_factor().unwrap() - 130.0 / 70.0).abs() < 1e-9);
        assert!((stats.average_win().unwrap() - 65.0).abs() < 1e-9);
        assert!((stats.average_loss().unwrap() + 35.0).abs() < 1e-9);
        assert!((stats.expectancy().unwrap() - 12.0).abs() < 1e-9);
        assert_eq!((stats.best, stats.worst), (100.0, -50.0));
    }

    #[test]
    fn no_trades_and_no_losses_have_no_ratios() {
        let none = HistoryStats::of(&[]);
        assert_eq!(none.win_rate(), None);
        assert_eq!(none.expectancy(), None);
        let only_wins = HistoryStats::of(&[10.0, 20.0]);
        assert_eq!(only_wins.profit_factor(), None);
        assert_eq!(only_wins.average_loss(), None);
    }

    #[test]
    fn a_search_needs_every_word_somewhere() {
        assert!(matches("", &["EURUSD"]));
        assert!(matches("eur buy", &["EURUSD", "Buy", "12"]));
        assert!(!matches("eur sell", &["EURUSD", "Buy"]));
        assert!(matches("  USD ", &["eurusd"]));
    }

    #[test]
    fn csv_fields_are_quoted_when_they_need_it() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_line(["a", "b,c", ""]), "a,\"b,c\",");
    }

    #[test]
    fn a_time_is_written_in_each_style() {
        // 2026-09-25 16:35:12 UTC.
        let ms = 1_790_354_112_000;
        assert_eq!(format_time(ms, TimeStyle::Iso), "2026-09-25 16:35:12");
        assert_eq!(format_time(ms, TimeStyle::Clock), "16:35:12");
        assert_eq!(format_time(ms, TimeStyle::Short), "Sep 25 16:35:12");
    }
}
