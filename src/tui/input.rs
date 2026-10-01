use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

use super::theme;

const MAX_BYTES: usize = 4096;

#[derive(Default)]
pub struct Input {
    text: String,
    cursor: usize,
}

impl Input {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set(&mut self, text: &str) {
        self.clear();
        self.insert(text);
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, text: &str) {
        for c in text.chars() {
            let c = if c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            };
            if !c.is_control() && self.text.len() + c.len_utf8() <= MAX_BYTES {
                self.text.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
        }
    }

    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    fn next(&self) -> usize {
        self.cursor
            + self.text[self.cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8)
    }

    pub fn key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Home | KeyCode::Char('a') if ctrl || key.code == KeyCode::Home => {
                self.cursor = 0
            }
            KeyCode::End | KeyCode::Char('e') if ctrl || key.code == KeyCode::End => {
                self.cursor = self.text.len()
            }
            KeyCode::Left => self.cursor = self.previous(),
            KeyCode::Right => self.cursor = self.next(),
            KeyCode::Backspace => {
                let at = self.previous();
                self.text.drain(at..self.cursor);
                self.cursor = at;
            }
            KeyCode::Delete => {
                self.text.drain(self.cursor..self.next());
            }
            KeyCode::Char('u') if ctrl => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char('k') if ctrl => {
                self.text.truncate(self.cursor);
            }
            KeyCode::Char('w') if ctrl => {
                let end = self.cursor;
                while self.cursor > 0 && self.text[..self.cursor].ends_with(char::is_whitespace) {
                    self.cursor = self.previous();
                }
                while self.cursor > 0 && !self.text[..self.cursor].ends_with(char::is_whitespace) {
                    self.cursor = self.previous();
                }
                self.text.drain(self.cursor..end);
            }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.insert(&c.to_string())
            }
            _ => {}
        }
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: bool,
        secret: bool,
        placeholder: &str,
    ) {
        if area.is_empty() {
            return;
        }
        let shown = if secret {
            "*".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        };
        let cursor = if secret {
            self.text[..self.cursor].chars().count()
        } else {
            self.cursor
        };
        let mut start = 0;
        while Span::raw(&shown[start..cursor]).width() >= usize::from(area.width) && start < cursor
        {
            start += shown[start..].chars().next().map_or(0, char::len_utf8);
        }
        let (text, color) = if shown.is_empty() {
            (placeholder, theme::MUTED)
        } else {
            (&shown[start..], theme::TEXT)
        };
        frame.render_widget(Paragraph::new(text).style(Style::default().fg(color)), area);
        if focused {
            let offset = u16::try_from(Span::raw(&shown[start..cursor]).width()).unwrap_or(0);
            frame.set_cursor_position((area.x + offset, area.y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_at_unicode_boundaries_and_removes_words() {
        let mut input = Input::default();
        input.set("a\u{20ac}b");
        input.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        input.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(input.text(), "ab");
        input.insert("xy");
        assert_eq!(input.text(), "axyb");
        input.set("buy EURUSD 0.01");
        input.key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(input.text(), "buy EURUSD ");
    }

    #[test]
    fn paste_cannot_inject_terminal_controls_or_unbounded_text() {
        let mut input = Input::default();
        input.insert("a\n\t\u{1b}b");
        assert_eq!(input.text(), "a  b");
        input.insert(&"x".repeat(MAX_BYTES));
        assert_eq!(input.text().len(), MAX_BYTES);
    }
}
