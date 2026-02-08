//! Theme and styling constants for the TUI.

use ratatui::style::{Color, Modifier, Style};

/// Primary foreground color.
pub const FG_PRIMARY: Color = Color::White;

/// Secondary foreground color (dimmed text).
pub const FG_SECONDARY: Color = Color::Gray;

/// Primary background color.
pub const BG_PRIMARY: Color = Color::Black;

/// Highlight color for focused elements.
pub const HIGHLIGHT: Color = Color::Cyan;

/// Success/online indicator color.
pub const SUCCESS: Color = Color::Green;

/// Warning/away indicator color.
pub const WARNING: Color = Color::Yellow;

/// Error/offline indicator color.
pub const ERROR: Color = Color::Red;

/// Agent indicator color.
pub const AGENT: Color = Color::LightMagenta;

/// Color for sender names in chat.
pub const SENDER_COLORS: [Color; 6] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::LightCyan,
];

/// Normal text style.
#[must_use]
pub fn normal() -> Style {
    Style::default().fg(FG_PRIMARY)
}

/// Dimmed text style (timestamps, metadata).
#[must_use]
pub fn dimmed() -> Style {
    Style::default().fg(FG_SECONDARY)
}

/// Bold text style.
#[must_use]
pub fn bold() -> Style {
    Style::default().fg(FG_PRIMARY).add_modifier(Modifier::BOLD)
}

/// Highlighted text style (focused panel borders).
#[must_use]
pub fn highlighted() -> Style {
    Style::default().fg(HIGHLIGHT).add_modifier(Modifier::BOLD)
}

/// Selected item style (in lists).
#[must_use]
pub fn selected() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(HIGHLIGHT)
        .add_modifier(Modifier::BOLD)
}

/// Get a color for a sender based on their name.
#[must_use]
pub fn sender_color(name: &str) -> Color {
    let hash = name.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
    SENDER_COLORS[(hash as usize) % SENDER_COLORS.len()]
}
