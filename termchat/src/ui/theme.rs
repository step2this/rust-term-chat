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

/// Presence: online indicator color.
pub const PRESENCE_ONLINE: Color = Color::Green;

/// Presence: away indicator color.
pub const PRESENCE_AWAY: Color = Color::Yellow;

/// Presence: offline indicator color.
pub const PRESENCE_OFFLINE: Color = Color::DarkGray;

/// Color for sender names in chat.
pub const SENDER_COLORS: [Color; 12] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::LightCyan,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightRed,
    Color::Rgb(255, 165, 0),
    Color::Rgb(180, 120, 255),
];

/// Panel title color for the chat panel.
pub const CHAT_TITLE: Color = Color::Cyan;

/// Panel title color for the sidebar panel.
pub const SIDEBAR_TITLE: Color = Color::Blue;

/// Panel title color for the tasks panel.
pub const TASKS_TITLE: Color = Color::Green;

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

/// Style for system messages (italic, dim blue).
#[must_use]
pub fn system_message() -> Style {
    Style::default()
        .fg(Color::Rgb(100, 140, 180))
        .add_modifier(Modifier::ITALIC)
}

/// Style for timestamps (dark gray).
#[must_use]
pub fn timestamp() -> Style {
    Style::default().fg(Color::Rgb(120, 120, 120))
}

/// Style for the input cursor (bright white, bold).
#[must_use]
pub fn input_cursor() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

/// Style for the status bar background (dark background with white foreground).
#[must_use]
pub fn status_bar_bg() -> Style {
    Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 50))
}

/// Style for panel titles with a given color (bold).
#[must_use]
pub fn panel_title(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

/// Style for unread count badges (bold yellow on dark background).
#[must_use]
pub fn unread_badge() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .bg(Color::Rgb(30, 30, 50))
        .add_modifier(Modifier::BOLD)
}
