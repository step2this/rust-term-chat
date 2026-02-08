//! Task panel rendering.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::theme;
use crate::app::{App, PanelFocus, TaskDisplayStatus};

/// Render the task panel with interactive task list from app state.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == PanelFocus::Tasks;
    let border_style = if focused {
        theme::highlighted()
    } else {
        theme::normal()
    };

    let block = Block::default()
        .title("Tasks")
        .borders(Borders::ALL)
        .border_style(border_style);

    if app.tasks.is_empty() {
        let paragraph = Paragraph::new("No tasks yet. Use /task add <title> to create one.")
            .style(theme::dimmed())
            .alignment(Alignment::Center)
            .block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let is_selected = focused && i == app.selected_task;

            let status_style = match task.status {
                TaskDisplayStatus::Completed => theme::dimmed(),
                TaskDisplayStatus::InProgress => theme::highlighted(),
                TaskDisplayStatus::Open => theme::normal(),
            };

            let assignee_str = task
                .assignee
                .as_ref()
                .map_or(String::new(), |a| format!(" (@{a})"));

            let line_style = if is_selected {
                theme::selected()
            } else {
                status_style
            };

            let line = Line::from(vec![
                Span::styled(format!("#{} ", task.number), line_style),
                Span::styled(task.status.symbol(), line_style),
                Span::styled(format!(" {}{assignee_str}", task.title), line_style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
