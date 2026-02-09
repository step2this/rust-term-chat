//! Chat panel rendering (message list + input box).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::theme;
use crate::app::{App, PanelFocus};

/// Render the chat panel (messages + typing indicator + input box).
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let typing_peers = app.current_typing_peers();
    let has_typing = !typing_peers.is_empty();

    // Split into message area, optional typing indicator, and input area
    let constraints = if has_typing {
        vec![
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ]
    } else {
        vec![Constraint::Min(3), Constraint::Length(3)]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    render_messages(frame, chunks[0], app);

    if has_typing {
        render_typing_indicator(frame, chunks[1], &typing_peers);
        render_input(frame, chunks[2], app);
    } else {
        render_input(frame, chunks[1], app);
    }
}

/// Render the message list.
fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == PanelFocus::Chat;

    // Show empty state if no conversations exist
    let items: Vec<ListItem> = if app.conversations.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No conversations — connect with --remote-peer or /join-room",
            theme::dimmed(),
        )))]
    } else {
        app.current_messages()
            .iter()
            .map(|msg| {
                let timestamp_style = theme::timestamp();
                let status_style = theme::dimmed();
                let is_agent = msg.sender.starts_with("agent:");

                let (display_sender, sender_style) = if is_agent {
                    // Strip the "agent:" prefix and show [Agent] badge
                    let name = msg.sender.strip_prefix("agent:").unwrap_or(&msg.sender);
                    (format!("[Agent] {name}"), theme::normal().fg(theme::AGENT))
                } else if msg.sender == "System" {
                    (msg.sender.clone(), theme::system_message())
                } else {
                    (
                        msg.sender.clone(),
                        theme::normal().fg(theme::sender_color(&msg.sender)),
                    )
                };

                let content_style = if msg.sender == "System" {
                    theme::system_message()
                } else {
                    theme::normal()
                };

                let line = Line::from(vec![
                    Span::styled(&msg.timestamp, timestamp_style),
                    Span::raw(" "),
                    Span::styled(display_sender, sender_style),
                    Span::raw(": "),
                    Span::styled(&msg.content, content_style),
                    Span::raw(" "),
                    Span::styled(msg.status.symbol(), status_style),
                ]);

                ListItem::new(line)
            })
            .collect()
    };

    // Update title to show selected conversation name
    let title = app.selected_conversation_name().map_or_else(
        || "Chat: (none)".to_string(),
        |conv_name| format!("Chat: {conv_name}"),
    );

    let block = Block::default()
        .title(title)
        .title_style(theme::panel_title(theme::CHAT_TITLE))
        .borders(Borders::ALL)
        .border_style(if is_focused {
            theme::highlighted()
        } else {
            theme::normal()
        });

    let list = List::new(items).block(block);

    frame.render_widget(list, area);
}

/// Render the typing indicator line (e.g., "Alice is typing...").
fn render_typing_indicator(frame: &mut Frame, area: Rect, typing_peers: &[&str]) {
    let text = match typing_peers.len() {
        0 => return,
        1 => format!("{} is typing...", typing_peers[0]),
        2 => format!("{} and {} are typing...", typing_peers[0], typing_peers[1]),
        n => format!("{} and {} others are typing...", typing_peers[0], n - 1),
    };

    let line = Line::from(Span::styled(
        format!("  {text}"),
        theme::dimmed().add_modifier(ratatui::style::Modifier::ITALIC),
    ));

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
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
