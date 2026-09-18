//! The dashboard screen: connection status plus, once connected, the account snapshot.
//!
//! This is intentionally minimal for now — a proof that the full chain (`wyck-config`
//! profile -> `ctrader-mcp` connection -> live account data -> render) works end to
//! end. Positions, P&L, and order placement are follow-up work (see the project's own
//! TUI roadmap).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

use crate::engine::AccountSnapshot;

/// The dashboard's connection state machine. Distinct from [`crate::engine::EngineEvent`]
/// because the UI only cares about "what to draw right now", not the full event
/// history — [`crate::app::App`] folds each event into this on arrival.
pub enum ConnectionStatus {
    Connecting,
    Connected(AccountSnapshot),
    Failed(String),
}

pub struct DashboardScreen {
    /// The profile's display name, shown in the title regardless of connection state.
    display_name: String,
    status: ConnectionStatus,
}

impl DashboardScreen {
    /// Starts the screen in the `Connecting` state — used when
    /// [`crate::app::App`] auto-connects the active profile on startup.
    pub fn connecting(display_name: String) -> Self {
        Self {
            display_name,
            status: ConnectionStatus::Connecting,
        }
    }

    pub fn set_status(&mut self, status: ConnectionStatus) {
        self.status = status;
    }

    /// Renders the dashboard into `area`.
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

        self.draw_status_line(frame, rows[0]);

        match &self.status {
            ConnectionStatus::Connected(snapshot) => {
                self.draw_account_table(frame, rows[1], snapshot)
            }
            ConnectionStatus::Connecting => {
                let paragraph =
                    Paragraph::new("Connecting…").style(Style::default().fg(Color::Yellow));
                frame.render_widget(paragraph, rows[1]);
            }
            ConnectionStatus::Failed(message) => {
                let paragraph = Paragraph::new(format!("Connection failed: {message}"))
                    .style(Style::default().fg(Color::Red));
                frame.render_widget(paragraph, rows[1]);
            }
        }

        let hint = Paragraph::new("r to refresh · q/Esc to quit")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, rows[2]);
    }

    fn draw_status_line(&self, frame: &mut Frame, area: Rect) {
        let (label, color) = match &self.status {
            ConnectionStatus::Connecting => ("connecting", Color::Yellow),
            ConnectionStatus::Connected(_) => ("connected", Color::Green),
            ConnectionStatus::Failed(_) => ("disconnected", Color::Red),
        };

        let line = Line::from(vec![
            Span::styled(
                self.display_name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  —  "),
            Span::styled(label, Style::default().fg(color)),
        ]);

        let block = Block::default().borders(Borders::ALL).title("wyck");
        frame.render_widget(Paragraph::new(line).block(block), area);
    }

    fn draw_account_table(&self, frame: &mut Frame, area: Rect, snapshot: &AccountSnapshot) {
        let currency = snapshot.account_currency.as_deref().unwrap_or("?");
        let money = |value: Option<f64>| match value {
            Some(value) => format!("{value:.2} {currency}"),
            None => "—".to_owned(),
        };

        let rows = [
            Row::new([
                "Trader ID".to_owned(),
                snapshot
                    .trader_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "—".to_owned()),
            ]),
            Row::new(["Balance".to_owned(), money(snapshot.balance)]),
            Row::new(["Equity".to_owned(), money(snapshot.equity)]),
            Row::new(["Free margin".to_owned(), money(snapshot.free_margin)]),
            Row::new([
                "Server".to_owned(),
                snapshot
                    .server_version
                    .clone()
                    .unwrap_or_else(|| "—".to_owned()),
            ]),
        ];

        let table = Table::new(rows, [Constraint::Length(16), Constraint::Min(0)])
            .block(Block::default().borders(Borders::ALL).title("Account"));
        frame.render_widget(table, area);
    }
}
