//! Chat panel rendering (message list + input box).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::theme;
use crate::app::{App, PanelFocus};

/// Render the chat panel (messages + input box).
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Split into message area and input area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    render_messages(frame, chunks[0], app);
    render_input(frame, chunks[1], app);
}

/// Render the message list.
fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == PanelFocus::Chat;

    let items: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let sender_style = theme::normal().fg(theme::sender_color(&msg.sender));
            let timestamp_style = theme::dimmed();
            let status_style = theme::dimmed();

            let line = Line::from(vec![
                Span::styled(&msg.timestamp, timestamp_style),
                Span::raw(" "),
                Span::styled(&msg.sender, sender_style),
                Span::raw(": "),
                Span::styled(&msg.content, theme::normal()),
                Span::raw(" "),
                Span::styled(msg.status.symbol(), status_style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .title("Chat")
        .borders(Borders::ALL)
        .border_style(if is_focused {
            theme::highlighted()
        } else {
            theme::normal()
        });

    let list = List::new(items).block(block);

    frame.render_widget(list, area);
}

/// Render the input box.
fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == PanelFocus::Input;

    // Build the input text with cursor
    let mut display_text = app.input.clone();
    if is_focused {
        // Insert cursor character at cursor position
        if app.cursor_position >= display_text.len() {
            display_text.push('█');
        } else {
            display_text.insert(app.cursor_position, '█');
        }
    }

    let input_line = if display_text.is_empty() && !is_focused {
        Line::from(Span::styled("Type a message...", theme::dimmed()))
    } else {
        Line::from(Span::styled(display_text, theme::normal()))
    };

    let block = Block::default()
        .title("Input")
        .borders(Borders::ALL)
        .border_style(if is_focused {
            theme::highlighted()
        } else {
            theme::normal()
        });

    let paragraph = Paragraph::new(input_line).block(block);

    frame.render_widget(paragraph, area);
}
