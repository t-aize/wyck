use chrono::{DateTime, Datelike, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use ratatui::style::Color;
use ratatui::text::Span;

use super::theme;

const SYDNEY: Color = Color::Rgb(244, 114, 182);
const TOKYO: Color = Color::Rgb(251, 191, 36);
const LONDON: Color = Color::Rgb(96, 165, 250);
const NEW_YORK: Color = Color::Rgb(167, 139, 250);

// The session and ICT windows used by the original OpenTUI header, in each city's time zone.
pub fn labels(now: DateTime<Utc>, killzone: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (zone, start, end, name, color) in [
        (chrono_tz::Australia::Sydney, 8, 17, "SYD", SYDNEY),
        (chrono_tz::Asia::Tokyo, 9, 18, "TOK", TOKYO),
        (chrono_tz::Europe::London, 8, 17, "LON", LONDON),
        (chrono_tz::America::New_York, 8, 17, "NY", NEW_YORK),
    ] {
        if in_window(now, zone, start, end) {
            if !spans.is_empty() {
                spans.push(Span::styled("/", theme::bold(theme::DIM)));
            }
            spans.push(Span::styled(name, theme::bold(color)));
        }
    }
    if spans.is_empty() {
        spans.push(Span::styled("off session", theme::bold(theme::DIM)));
    }
    if killzone {
        for (start, end, name, color) in [
            (20, 24, "ASIA", TOKYO),
            (2, 5, "LON", LONDON),
            (7, 10, "NY", NEW_YORK),
            (10, 12, "LON CLOSE", Color::Rgb(56, 189, 248)),
        ] {
            if in_window(now, chrono_tz::America::New_York, start, end) {
                spans.push(Span::styled(format!(" ({name})"), theme::bold(color)));
                break;
            }
        }
    }
    spans
}

fn in_window(now: DateTime<Utc>, zone: Tz, start: u32, end: u32) -> bool {
    let local = now.with_timezone(&zone);
    !matches!(local.weekday(), Weekday::Sat | Weekday::Sun) && (start..end).contains(&local.hour())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_windows_follow_dst_and_local_weekdays() {
        let winter = "2026-01-05T08:00:00Z".parse().unwrap();
        let summer = "2026-07-06T07:00:00Z".parse().unwrap();
        assert!(in_window(winter, chrono_tz::Europe::London, 8, 17));
        assert!(in_window(summer, chrono_tz::Europe::London, 8, 17));
        let sunday = "2026-10-04T23:00:00Z".parse().unwrap();
        assert!(in_window(sunday, chrono_tz::Australia::Sydney, 8, 17));
        assert!(!in_window(sunday, chrono_tz::America::New_York, 8, 17));
    }
}
