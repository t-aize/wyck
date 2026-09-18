//! The first-run screen: a small form that creates the first connection profile.
//!
//! Shown whenever [`wyck_config::WyckConfig`] has no active profile yet — either a
//! genuinely fresh install, or a profile whose stored token couldn't be resolved (see
//! `main.rs`'s startup sequence).

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use secrecy::SecretString;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

/// Which field currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    DisplayName,
    Service,
    Endpoint,
    Token,
}

impl Field {
    const ORDER: [Field; 4] = [
        Field::DisplayName,
        Field::Service,
        Field::Endpoint,
        Field::Token,
    ];

    fn label(self) -> &'static str {
        match self {
            Field::DisplayName => "Display name",
            Field::Service => "Service",
            Field::Endpoint => "Endpoint URI",
            Field::Token => "Token",
        }
    }

    fn next(self) -> Self {
        let index = Self::ORDER
            .iter()
            .position(|&field| field == self)
            .unwrap_or(0);
        Self::ORDER[(index + 1) % Self::ORDER.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ORDER
            .iter()
            .position(|&field| field == self)
            .unwrap_or(0);
        Self::ORDER[(index + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

/// The outcome of feeding a key event to the form: either nothing notable happened, or
/// the user asked to leave the screen — either by submitting a complete form or by
/// cancelling.
pub enum FirstRunOutcome {
    /// Nothing to act on yet; keep showing the form.
    Continue,
    /// The user pressed Enter with every required field filled in.
    Submit {
        display_name: String,
        service: String,
        endpoint: String,
        token: SecretString,
    },
    /// The user pressed Esc.
    Cancelled,
}

/// State for the profile-creation form: one [`Input`] per field, which one is focused,
/// and the last validation error (if the user tried to submit an incomplete form).
pub struct FirstRunScreen {
    display_name: Input,
    service: Input,
    endpoint: Input,
    token: Input,
    focus: Field,
    error: Option<String>,
}

impl Default for FirstRunScreen {
    fn default() -> Self {
        Self {
            display_name: Input::default(),
            // Pre-filled with the most common defaults so the common case is just
            // "type a display name, paste a token, Enter" — Tab past the rest.
            service: Input::new("ctrader-remote".to_owned()),
            endpoint: Input::new("https://mcp.spotware.com/mcp".to_owned()),
            token: Input::default(),
            focus: Field::DisplayName,
            error: None,
        }
    }
}

impl FirstRunScreen {
    /// Feeds one terminal event to the form: routes navigation keys (Tab/Shift+Tab,
    /// Enter, Esc) here, and forwards everything else to the focused [`Input`].
    pub fn handle_event(&mut self, event: &Event) -> FirstRunOutcome {
        let Event::Key(key) = event else {
            return FirstRunOutcome::Continue;
        };
        // tui-input only cares about presses/repeats; ignore key-release events here
        // too so we don't accidentally treat a release as a second navigation step.
        if key.kind != crossterm::event::KeyEventKind::Press {
            return FirstRunOutcome::Continue;
        }

        match key {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => return FirstRunOutcome::Cancelled,
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                self.focus = self.focus.next();
                return FirstRunOutcome::Continue;
            }
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => {
                self.focus = self.focus.previous();
                return FirstRunOutcome::Continue;
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                return self.try_submit();
            }
            _ => {}
        }

        self.focused_input_mut().handle_event(event);
        self.error = None;
        FirstRunOutcome::Continue
    }

    fn try_submit(&mut self) -> FirstRunOutcome {
        let display_name = self.display_name.value().trim();
        let service = self.service.value().trim();
        let endpoint = self.endpoint.value().trim();
        let token = self.token.value();

        if display_name.is_empty() {
            self.error = Some("Display name is required.".to_owned());
            self.focus = Field::DisplayName;
            return FirstRunOutcome::Continue;
        }
        if service.is_empty() {
            self.error = Some("Service is required.".to_owned());
            self.focus = Field::Service;
            return FirstRunOutcome::Continue;
        }
        if endpoint.is_empty() {
            self.error = Some("Endpoint URI is required.".to_owned());
            self.focus = Field::Endpoint;
            return FirstRunOutcome::Continue;
        }
        if token.is_empty() {
            self.error = Some("Token is required.".to_owned());
            self.focus = Field::Token;
            return FirstRunOutcome::Continue;
        }

        FirstRunOutcome::Submit {
            display_name: display_name.to_owned(),
            service: service.to_owned(),
            endpoint: endpoint.to_owned(),
            token: SecretString::from(token.to_owned()),
        }
    }

    fn focused_input_mut(&mut self) -> &mut Input {
        match self.focus {
            Field::DisplayName => &mut self.display_name,
            Field::Service => &mut self.service,
            Field::Endpoint => &mut self.endpoint,
            Field::Token => &mut self.token,
        }
    }

    /// Renders the form into `area`.
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let rows = Layout::vertical([
            Constraint::Length(3), // display name
            Constraint::Length(3), // service
            Constraint::Length(3), // endpoint
            Constraint::Length(3), // token
            Constraint::Length(2), // error / hint line
        ])
        .split(area);

        self.draw_field(frame, rows[0], Field::DisplayName, false);
        self.draw_field(frame, rows[1], Field::Service, false);
        self.draw_field(frame, rows[2], Field::Endpoint, false);
        self.draw_field(frame, rows[3], Field::Token, true);

        let hint = match &self.error {
            Some(message) => Line::from(Span::styled(
                message.as_str(),
                Style::default().fg(Color::Red),
            )),
            None => Line::from(Span::styled(
                "Tab/Shift+Tab to move between fields · Enter to save and connect · Esc to quit",
                Style::default().fg(Color::DarkGray),
            )),
        };
        frame.render_widget(Paragraph::new(hint), rows[4]);
    }

    fn draw_field(&self, frame: &mut Frame, area: Rect, field: Field, mask: bool) {
        let input = self.input_for(field);
        let focused = self.focus == field;

        let border_style = if focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(field.label());

        let display_value: std::borrow::Cow<'_, str> = if mask {
            std::borrow::Cow::Owned("•".repeat(input.value().chars().count()))
        } else {
            std::borrow::Cow::Borrowed(input.value())
        };

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(display_value.as_ref()), inner);

        if focused {
            let cursor_x = inner.x + input.visual_cursor() as u16;
            frame.set_cursor_position((cursor_x.min(inner.right().saturating_sub(1)), inner.y));
        }
    }

    fn input_for(&self, field: Field) -> &Input {
        match field {
            Field::DisplayName => &self.display_name,
            Field::Service => &self.service,
            Field::Endpoint => &self.endpoint,
            Field::Token => &self.token,
        }
    }
}
