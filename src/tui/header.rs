use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::{sessions, theme};
use crate::openapi::Environment;

pub struct Market {
    pub symbol: String,
    pub bid: String,
    pub ask: String,
    pub spread: String,
    pub spread_color: Color,
    pub direction: &'static str,
    pub color: Color,
    pub history: Vec<i64>,
}

pub struct Header<'a> {
    pub context: &'a str,
    pub environment: Option<Environment>,
    pub status: &'a str,
    pub market: Option<Market>,
    pub balance: Option<String>,
}

impl Header<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        self.render_at(frame, area, Utc::now());
    }

    fn render_at(&self, frame: &mut Frame, area: Rect, now: DateTime<Utc>) {
        if area.is_empty() {
            return;
        }
        frame.render_widget(Block::default().style(theme::base().bg(theme::PANEL)), area);
        let symbol = self.market.as_ref().map_or(self.context, |m| &m.symbol);
        let left = Line::from(vec![
            Span::styled("  WYCK", theme::bold(theme::ACCENT)),
            Span::styled(format!("  {symbol}  "), Style::default().fg(theme::MUTED)),
        ]);
        let left_width = u16::try_from(left.width()).unwrap_or(8).min(area.width / 3);
        let minimum_quote = self
            .market
            .as_ref()
            .map_or(0, |m| 2 + m.bid.len() + 3 + m.ask.len());
        let mut detail = 4;
        let mut right = self.status_line(now, detail);
        while detail > 0
            && usize::from(left_width) + minimum_quote + right.width() + 4 > usize::from(area.width)
        {
            detail -= 1;
            right = self.status_line(now, detail);
        }
        let right_width = u16::try_from(right.width())
            .unwrap_or(10)
            .min(area.width / 2);
        let [identity, _, prices, _, state] = Layout::horizontal([
            Constraint::Length(left_width),
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
            Constraint::Length(right_width),
        ])
        .areas(area);
        frame.render_widget(Paragraph::new(left), identity);
        frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), state);
        if let Some(market) = &self.market {
            let mut spans = vec![
                Span::styled(
                    format!("{} ", market.direction),
                    Style::default().fg(market.color),
                ),
                Span::styled(market.bid.clone(), theme::bold(theme::ACCENT)),
            ];
            let mut append = |text: String, color: Color| {
                if Line::from(spans.clone()).width() + Span::raw(&text).width()
                    <= usize::from(prices.width)
                {
                    spans.push(Span::styled(text, Style::default().fg(color)));
                }
            };
            append(format!(" / {}", market.ask), theme::DIM);
            append(format!(" \u{b7} {}", market.spread), market.spread_color);
            if market.history.len() >= 2 {
                append(format!(" {}", sparkline(&market.history)), theme::DIM);
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
                prices,
            );
        }
    }

    fn status_line(&self, now: DateTime<Utc>, detail: u8) -> Line<'static> {
        let (label, color) = match self.status {
            "Connected" if self.environment == Some(Environment::Demo) => ("DEMO", theme::GREEN),
            "Connected" => ("LIVE", theme::GREEN),
            "Preview" => ("PREVIEW", theme::MUTED),
            "Disconnected" | "Load failed" => ("ERROR", theme::RED),
            "Stopped" => ("OFFLINE", theme::DIM),
            "Connecting..." | "Loading..." => ("CONNECTING", theme::MUTED),
            status if status.starts_with("Reconnecting") => ("RECONNECT", theme::YELLOW),
            status => (status, theme::MUTED),
        };
        let mut spans = vec![Span::styled(
            format!("\u{25cf} {label}"),
            theme::bold(color),
        )];
        if detail >= 1
            && let Some(balance) = &self.balance
        {
            spans.push(Span::styled(
                format!("  {balance}"),
                Style::default().fg(theme::ACCENT),
            ));
        }
        if detail >= 2 {
            spans.push(Span::styled(
                format!(
                    " \u{b7} {}",
                    now.with_timezone(&chrono_tz::Europe::Paris)
                        .format("%H:%M:%S")
                ),
                Style::default().fg(theme::MUTED),
            ));
        }
        if detail >= 3 {
            spans.push(Span::styled(" \u{b7} ", Style::default().fg(theme::MUTED)));
            spans.extend(sessions::labels(now, detail >= 4));
        }
        spans.push(Span::raw("  "));
        Line::from(spans)
    }
}

fn sparkline(values: &[i64]) -> String {
    let min = values.iter().min().copied().unwrap_or(0) as f64;
    let max = values.iter().max().copied().unwrap_or(0) as f64;
    values
        .iter()
        .map(|value| {
            let normalized = if max == min {
                0.5
            } else {
                (*value as f64 - min) / (max - min)
            };
            let level = ((normalized * 8.0) as u32).min(7);
            char::from_u32(0x2581 + level).unwrap_or(' ')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn the_old_header_groups_fit_one_row() {
        let now = "2026-10-01T12:30:00Z".parse().unwrap();
        for width in [80, 100, 140, 180] {
            let mut terminal = Terminal::new(TestBackend::new(width, 2)).unwrap();
            terminal
                .draw(|frame| {
                    Header {
                        context: "Sample account",
                        environment: Some(Environment::Demo),
                        status: "Connected",
                        market: Some(Market {
                            symbol: "EURUSD".into(),
                            bid: "1.08542".into(),
                            ask: "1.08550".into(),
                            spread: "0.00008".into(),
                            spread_color: theme::DIM,
                            direction: "\u{25b2}",
                            color: theme::GREEN,
                            history: vec![1, 2, 3, 2, 4],
                        }),
                        balance: Some("25,000.00 USD".into()),
                    }
                    .render_at(frame, Rect::new(0, 0, width, 1), now)
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let first: String = (0..width).map(|x| buffer[(x, 0)].symbol()).collect();
            let second: String = (0..width).map(|x| buffer[(x, 1)].symbol()).collect();
            assert!(
                first.contains("WYCK")
                    && first.contains("EURUSD")
                    && first.contains("DEMO")
                    && first.contains("1.08542 / 1.08550"),
                "{first}"
            );
            assert!(second.trim().is_empty());
            if width >= 140 {
                assert!(
                    first.contains("LON/NY (NY)")
                        && first.contains("14:30:00")
                        && first.contains("25,000.00 USD"),
                    "{first}"
                );
            }
        }
    }

    #[test]
    fn flat_price_series_uses_the_middle_level() {
        assert_eq!(sparkline(&[5, 5, 5]), "\u{2585}\u{2585}\u{2585}");
        assert_eq!(sparkline(&[0, 7]), "\u{2581}\u{2588}");
    }
}
