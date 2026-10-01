use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders};

use crate::openapi::account::book::Tone;

pub const BG: Color = Color::Rgb(10, 10, 10);
pub const PANEL: Color = Color::Rgb(20, 20, 20);
pub const TEXT: Color = Color::Rgb(250, 250, 250);
pub const MUTED: Color = Color::Rgb(163, 163, 163);
pub const DIM: Color = Color::Rgb(82, 82, 82);
pub const BORDER: Color = Color::Rgb(39, 39, 42);
pub const ACCENT: Color = Color::Rgb(229, 229, 229);
pub const GREEN: Color = Color::Rgb(61, 214, 140);
pub const RED: Color = Color::Rgb(240, 85, 90);
pub const YELLOW: Color = Color::Rgb(225, 192, 111);
pub const SELECTED: Color = Color::Rgb(38, 38, 38);

pub fn base() -> Style {
    Style::default().fg(TEXT).bg(BG)
}

pub fn bold(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub fn tone(tone: Tone) -> Color {
    match tone {
        Tone::Success => GREEN,
        Tone::Warning => YELLOW,
        Tone::Error => RED,
        _ => MUTED,
    }
}

pub fn popup(title: String) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .title_style(bold(ACCENT))
        .style(base().bg(PANEL))
        .title(title)
}
