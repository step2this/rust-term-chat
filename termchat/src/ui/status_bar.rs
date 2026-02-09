//! Status bar rendering.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;
use crate::app::{App, PanelFocus};

/// Render the status bar at the bottom of the screen.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let help_text = match app.focus {
        PanelFocus::Input => "Enter: send | Tab: switch panel | Esc: quit | ←→: move cursor",
        PanelFocus::Sidebar => "Tab: switch panel | ↑↓/jk: navigate | Esc: quit",
        PanelFocus::Chat => "Tab: switch panel | ↑↓/jk: scroll | Esc: quit",
        PanelFocus::Tasks => {
            "Tab: switch panel | ↑↓/jk: navigate | Enter: toggle status | Esc: quit"
        }
    };

    let (dot_color, status_text) = if app.is_connected {
        (
            theme::SUCCESS,
            format!("Connected via {}", app.connection_info),
        )
    } else if app.connection_info == "Reconnecting" {
        (theme::WARNING, "Reconnecting...".to_string())
    } else {
        (theme::PRESENCE_OFFLINE, "Disconnected".to_string())
    };

    let status_line = Line::from(vec![
        Span::styled("TermChat v0.1.0", theme::bold()),
        Span::raw(" | "),
        Span::styled("●", theme::normal().fg(dot_color)),
        Span::raw(format!(" {status_text}")),
        Span::raw(" | "),
        Span::styled(help_text, theme::dimmed()),
    ]);

    let paragraph = Paragraph::new(status_line).style(theme::status_bar_bg());
    frame.render_widget(paragraph, area);
}
