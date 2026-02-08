//! Sidebar rendering for conversation list.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use termchat_proto::presence::PresenceStatus;

use super::theme;
use crate::app::{App, PanelFocus};

/// Get the presence dot symbol and color for a status.
const fn presence_indicator(status: PresenceStatus) -> (&'static str, ratatui::style::Color) {
    match status {
        PresenceStatus::Online => ("\u{25cf}", theme::PRESENCE_ONLINE),
        PresenceStatus::Away => ("\u{25cf}", theme::PRESENCE_AWAY),
        PresenceStatus::Offline => ("\u{25cb}", theme::PRESENCE_OFFLINE),
    }
}

/// Render the sidebar with the conversation list.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == PanelFocus::Sidebar;

    let items: Vec<ListItem> = app
        .conversations
        .iter()
        .enumerate()
        .map(|(idx, conv)| {
            let is_selected = idx == app.selected_conversation;

            let mut spans = Vec::new();

            // Add presence dot for DM conversations
            if let Some(status) = conv.presence {
                let (dot, color) = presence_indicator(status);
                spans.push(Span::styled(dot, theme::normal().fg(color)));
                spans.push(Span::raw(" "));
            }

            spans.push(Span::raw(&conv.name));

            // Add agent badge if applicable
            if conv.is_agent {
                spans.push(Span::raw(" "));
                spans.push(Span::styled("[A]", theme::normal().fg(theme::AGENT)));
            }

            // Add unread badge if present
            if conv.unread_count > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("({})", conv.unread_count),
                    theme::normal().fg(theme::WARNING),
                ));
            }

            // Add preview if present
            if let Some(preview) = &conv.last_message_preview {
                spans.push(Span::raw("\n  "));
                spans.push(Span::styled(
                    preview.chars().take(20).collect::<String>(),
                    theme::dimmed(),
                ));
                if preview.len() > 20 {
                    spans.push(Span::styled("…", theme::dimmed()));
                }
            }

            let line = Line::from(spans);
            let style = if is_selected && is_focused {
                theme::selected()
            } else if is_selected {
                theme::highlighted()
            } else {
                theme::normal()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let block = Block::default()
        .title("Conversations")
        .borders(Borders::ALL)
        .border_style(if is_focused {
            theme::highlighted()
        } else {
            theme::normal()
        });

    let list = List::new(items).block(block);

    frame.render_widget(list, area);
}
