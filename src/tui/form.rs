use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub struct Field {
    pub label: &'static str,
    pub value: String,
    pub secret: bool,
}

impl Field {
    pub fn new(label: &'static str, value: &str, secret: bool) -> Self {
        Self {
            label,
            value: value.to_owned(),
            secret,
        }
    }
}

pub enum FormAction {
    None,
    Submit,
    Cancel,
}

pub struct Form {
    pub title: String,
    pub hint: &'static str,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub busy: bool,
    pub live: Option<bool>,
}

impl Form {
    pub fn new(title: impl Into<String>, hint: &'static str, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            hint,
            fields,
            focus: 0,
            error: None,
            warning: None,
            busy: false,
            live: None,
        }
    }

    pub fn value(&self, index: usize) -> &str {
        self.fields[index].value.trim()
    }

    pub fn key(&mut self, key: KeyEvent) -> FormAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return FormAction::Cancel,
            KeyCode::Char('c') if ctrl => return FormAction::Cancel,
            _ if self.busy => {}
            KeyCode::Char('e') if ctrl && self.live.is_some() => {
                self.live = self.live.map(|live| !live);
            }
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % self.fields.len(),
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
            }
            KeyCode::Enter => return FormAction::Submit,
            KeyCode::Backspace => {
                self.fields[self.focus].value.pop();
            }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.warning = None;
                self.fields[self.focus].value.push(c);
            }
            _ => {}
        }
        FormAction::None
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, width: u16) {
        let mut lines = vec![Line::raw("")];
        for (index, field) in self.fields.iter().enumerate() {
            let shown = if field.secret {
                "*".repeat(field.value.chars().count())
            } else {
                field.value.clone()
            };
            let focused = index == self.focus && !self.busy;
            let cursor = if focused { "_" } else { "" };
            let style = if focused {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:<14}", field.label),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(format!("{shown}{cursor}"), style),
            ]));
        }
        if let Some(live) = self.live {
            let (text, color) = if live {
                ("LIVE (real money)", Color::Red)
            } else {
                ("Demo", Color::Green)
            };
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled(" Environment   ", Style::default().fg(Color::Gray)),
                Span::styled(
                    text,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  (Ctrl+E to switch)", Style::default().fg(Color::DarkGray)),
            ]));
        }
        lines.push(Line::raw(""));
        if self.busy {
            lines.push(Line::styled(
                " Working...",
                Style::default().fg(Color::Yellow),
            ));
        }
        if let Some(error) = &self.error {
            lines.push(Line::styled(
                format!(" {error}"),
                Style::default().fg(Color::Red),
            ));
        }
        if let Some(warning) = &self.warning {
            lines.push(Line::styled(
                format!(" {warning}"),
                Style::default().fg(Color::Yellow),
            ));
        }
        lines.push(Line::styled(
            format!(" {}", self.hint),
            Style::default().fg(Color::DarkGray),
        ));

        let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
        let rect = centered(area, width, height);
        frame.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", self.title));
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(ratatui::widgets::Wrap { trim: false }),
            rect,
        );
    }
}

pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn form() -> Form {
        let fields = vec![Field::new("A", "", false), Field::new("B", "", true)];
        Form::new("t", "h", fields)
    }

    #[test]
    fn typing_fills_the_focused_field_and_tab_moves_on() {
        let mut form = form();
        form.key(press(KeyCode::Char('x')));
        form.key(press(KeyCode::Tab));
        form.key(press(KeyCode::Char('y')));
        form.key(press(KeyCode::Char('z')));
        form.key(press(KeyCode::Backspace));
        assert_eq!((form.value(0), form.value(1)), ("x", "y"));
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut form = form();
        assert!(matches!(
            form.key(press(KeyCode::Enter)),
            FormAction::Submit
        ));
        assert!(matches!(form.key(press(KeyCode::Esc)), FormAction::Cancel));
    }

    #[test]
    fn a_busy_form_ignores_typing_but_can_be_cancelled() {
        let mut form = form();
        form.busy = true;
        form.key(press(KeyCode::Char('x')));
        assert_eq!(form.value(0), "");
        assert!(matches!(form.key(press(KeyCode::Esc)), FormAction::Cancel));
    }

    #[test]
    fn ctrl_e_switches_the_environment_only_when_there_is_one() {
        let ctrl_e = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
        let mut form = form();
        form.key(ctrl_e);
        assert_eq!(form.live, None);
        form.live = Some(false);
        form.key(ctrl_e);
        assert_eq!(form.live, Some(true));
    }
}
