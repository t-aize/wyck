use std::cell::Cell;
use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::{commands, input::Input, theme};
use crate::openapi::account::book::Tone;

const MAX_ENTRIES: usize = 200;
const MAX_HISTORY: usize = 100;

struct Entry {
    text: String,
    tone: Tone,
    command: bool,
}

pub struct Console {
    pub input: Input,
    pub focused: bool,
    entries: VecDeque<Entry>,
    history: VecDeque<String>,
    history_index: Option<usize>,
    draft: String,
    scroll: usize,
    scroll_limit: Cell<usize>,
}

impl Default for Console {
    fn default() -> Self {
        Self {
            input: Input::default(),
            focused: true,
            entries: VecDeque::new(),
            history: VecDeque::new(),
            history_index: None,
            draft: String::new(),
            scroll: 0,
            scroll_limit: Cell::new(0),
        }
    }
}

impl Console {
    pub fn push(&mut self, tone: Tone, text: impl Into<String>) {
        self.append(tone, text.into(), false);
    }

    fn append(&mut self, tone: Tone, text: String, command: bool) {
        let text = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(4096)
            .collect();
        self.entries.push_back(Entry {
            text,
            tone,
            command,
        });
        if self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.scroll = 0;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.scroll = 0;
        self.scroll_limit.set(0);
    }

    pub fn paste(&mut self, text: &str) {
        if self.focused {
            self.history_index = None;
            self.input.insert(text);
        }
    }

    pub fn suggestions(&self) -> Vec<String> {
        let text = self.input.text();
        if text.contains(char::is_whitespace) || text.is_empty() {
            return Vec::new();
        }
        let prefix = text.trim_start_matches('/').to_ascii_lowercase();
        commands::NAMES
            .iter()
            .filter(|name| name.starts_with(&prefix))
            .map(|name| format!("/{name}"))
            .collect()
    }

    pub fn key(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_add(5).min(self.scroll_limit.get())
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(5),
            KeyCode::Enter if self.focused => {
                let text = self.input.text().trim().to_owned();
                if text.is_empty() {
                    return None;
                }
                if self.history.back() != Some(&text) {
                    self.history.push_back(text.clone());
                }
                if self.history.len() > MAX_HISTORY {
                    self.history.pop_front();
                }
                self.history_index = None;
                self.draft.clear();
                self.input.clear();
                self.append(Tone::Info, text.clone(), true);
                return Some(text);
            }
            KeyCode::Up if self.focused && !self.history.is_empty() => {
                let index = match self.history_index {
                    Some(i) => i.saturating_sub(1),
                    None => {
                        self.draft = self.input.text().to_owned();
                        self.history.len() - 1
                    }
                };
                self.history_index = Some(index);
                self.input.set(&self.history[index]);
            }
            KeyCode::Down if self.focused => {
                if let Some(index) = self.history_index {
                    if index + 1 < self.history.len() {
                        self.history_index = Some(index + 1);
                        self.input.set(&self.history[index + 1]);
                    } else {
                        self.history_index = None;
                        self.input.set(&self.draft);
                    }
                }
            }
            KeyCode::Tab if self.focused => {
                let matches = self.suggestions();
                if matches.len() == 1 {
                    self.input.set(&format!("{} ", matches[0]));
                } else if let Some(first) = matches.first() {
                    let common: String = first
                        .chars()
                        .enumerate()
                        .take_while(|(i, c)| matches.iter().all(|m| m.chars().nth(*i) == Some(*c)))
                        .map(|(_, c)| c)
                        .collect();
                    self.input.set(&common);
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) && self.focused => {
                self.input.clear();
                self.history_index = None;
            }
            _ if self.focused => {
                self.input.key(key);
                if !matches!(
                    key.code,
                    KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
                ) {
                    self.history_index = None;
                }
            }
            _ => {}
        }
        None
    }

    pub fn draw_log(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" Activity ")
            .title_style(Style::default().fg(theme::MUTED));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let mut lines = Vec::new();
        for entry in &self.entries {
            for (index, text) in entry.text.lines().enumerate() {
                let prefix = if index > 0 {
                    "  "
                } else if entry.command {
                    "> "
                } else {
                    match entry.tone {
                        Tone::Success => "+ ",
                        Tone::Warning => "! ",
                        Tone::Error => "x ",
                        _ => "  ",
                    }
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        prefix,
                        theme::bold(if entry.command {
                            theme::ACCENT
                        } else {
                            theme::tone(entry.tone)
                        }),
                    ),
                    Span::styled(
                        text.to_owned(),
                        Style::default().fg(if entry.command {
                            theme::TEXT
                        } else {
                            theme::tone(entry.tone)
                        }),
                    ),
                ]));
            }
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let bottom = paragraph
            .line_count(inner.width)
            .saturating_sub(usize::from(inner.height));
        self.scroll_limit.set(bottom);
        let offset = bottom.saturating_sub(self.scroll);
        frame.render_widget(
            paragraph.scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
            inner,
        );
    }

    pub fn draw_input(&self, frame: &mut Frame, area: Rect, active: bool, footer: &str) {
        let [suggestions, prompt, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(self.suggestions().join("   "))
                .style(Style::default().fg(theme::ACCENT)),
            suggestions,
        );
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(if active { theme::MUTED } else { theme::BORDER }));
        let inner = block.inner(prompt);
        frame.render_widget(block, prompt);
        let [mark, input] =
            Layout::horizontal([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
        frame.render_widget(Paragraph::new("> ").style(theme::bold(theme::ACCENT)), mark);
        self.input.render(frame, input, active, false, "/help");
        frame.render_widget(
            Paragraph::new(footer).style(Style::default().fg(theme::MUTED)),
            status,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn history_restores_the_draft_and_completion_is_unambiguous() {
        let mut console = Console::default();
        console.input.set("/positions");
        assert_eq!(console.key(key(KeyCode::Enter)), Some("/positions".into()));
        console.input.set("draft");
        console.key(key(KeyCode::Up));
        assert_eq!(console.input.text(), "/positions");
        console.key(key(KeyCode::Down));
        assert_eq!(console.input.text(), "draft");
        console.input.set("/pos");
        console.key(key(KeyCode::Tab));
        assert_eq!(console.input.text(), "/positions ");
    }

    #[test]
    fn pasted_commands_require_enter_and_history_stays_bounded() {
        let mut console = Console::default();
        console.paste("/quit\n");
        assert_eq!(console.input.text(), "/quit ");
        assert!(console.entries.is_empty());
        for i in 0..250 {
            console.input.set(&format!("/add {i}"));
            console.key(key(KeyCode::Enter));
        }
        assert_eq!(console.history.len(), MAX_HISTORY);
        assert_eq!(console.entries.len(), MAX_ENTRIES);
    }

    #[test]
    fn scrolling_cannot_move_past_the_oldest_visible_message() {
        let mut console = Console::default();
        for i in 0..12 {
            console.push(Tone::Info, format!("message {i}"));
        }
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
        terminal
            .draw(|frame| console.draw_log(frame, frame.area()))
            .unwrap();
        for _ in 0..20 {
            console.key(key(KeyCode::PageUp));
        }
        assert_eq!(console.scroll, 5);
        console.key(key(KeyCode::PageDown));
        assert_eq!(console.scroll, 0);
    }
}
