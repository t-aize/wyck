use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{input::Input, theme};

pub struct Field {
    pub label: &'static str,
    pub input: Input,
    pub secret: bool,
}

impl Field {
    pub fn new(label: &'static str, value: &str, secret: bool) -> Self {
        let mut input = Input::default();
        input.set(value);
        Self {
            label,
            input,
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
    pub busy_label: &'static str,
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
            busy_label: "Working...",
            live: None,
        }
    }

    pub fn value(&self, index: usize) -> &str {
        self.fields[index].input.text().trim()
    }

    pub fn paste(&mut self, text: &str) {
        if !self.busy {
            self.warning = None;
            self.fields[self.focus].input.insert(text);
        }
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
            _ => {
                let before = self.fields[self.focus].input.text().to_owned();
                self.fields[self.focus].input.key(key);
                if before != self.fields[self.focus].input.text() {
                    self.warning = None;
                    self.error = None;
                }
            }
        }
        FormAction::None
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, width: u16) {
        let mut lines = Vec::new();
        if let Some(live) = self.live {
            let (text, color) = if live {
                ("LIVE (real money)", theme::RED)
            } else {
                ("Demo", theme::GREEN)
            };
            lines.push(Line::from(vec![
                Span::styled(text, theme::bold(color)),
                Span::styled("  Ctrl+E", Style::default().fg(theme::MUTED)),
            ]));
        }
        lines.push(Line::raw(""));
        if self.busy {
            lines.push(Line::styled(
                self.busy_label,
                Style::default().fg(theme::YELLOW),
            ));
        }
        if let Some(error) = &self.error {
            lines.push(Line::styled(error.clone(), Style::default().fg(theme::RED)));
        }
        if let Some(warning) = &self.warning {
            lines.push(Line::styled(
                warning.clone(),
                Style::default().fg(theme::YELLOW),
            ));
        }
        lines.push(Line::styled(self.hint, Style::default().fg(theme::MUTED)));

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let message_height = paragraph.line_count(width.min(area.width).saturating_sub(4));
        let height = u16::try_from(self.fields.len() * 3 + message_height + 3).unwrap_or(u16::MAX);
        let rect = centered(area, width, height);
        frame.render_widget(Clear, rect);
        let block = theme::popup(format!(" {} ", self.title));
        let mut inner = block.inner(rect);
        frame.render_widget(block, rect);
        inner.x = inner.x.saturating_add(1);
        inner.width = inner.width.saturating_sub(2);
        for (index, field) in self.fields.iter().enumerate() {
            let offset = u16::try_from(index * 3).unwrap_or(u16::MAX);
            if offset + 1 >= inner.height {
                break;
            }
            let label = Rect::new(inner.x, inner.y + offset, inner.width, 1);
            frame.render_widget(
                Paragraph::new(field.label).style(Style::default().fg(if index == self.focus {
                    theme::ACCENT
                } else {
                    theme::MUTED
                })),
                label,
            );
            let input = Rect::new(inner.x, inner.y + offset + 1, inner.width, 1);
            field.input.render(
                frame,
                input,
                index == self.focus && !self.busy,
                field.secret,
                "",
            );
        }
        let offset = u16::try_from(self.fields.len() * 3)
            .unwrap_or(u16::MAX)
            .min(inner.height);
        frame.render_widget(
            paragraph,
            Rect::new(
                inner.x,
                inner.y + offset,
                inner.width,
                inner.height - offset,
            ),
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
