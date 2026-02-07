//! Task panel rendering.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use super::theme;

/// Render the task panel with a placeholder task list.
pub fn render(frame: &mut Frame, area: Rect) {
    // Hardcoded demo tasks for Phase 1
    let tasks = [
        ("Build TUI layout", true),
        ("Add keyboard navigation", true),
        ("Implement message input", true),
        ("Add scrolling support", false),
        ("Connect to chat backend", false),
    ];

    let items: Vec<ListItem> = tasks
        .iter()
        .map(|(task, done)| {
            let checkbox = if *done { "[✓]" } else { "[ ]" };
            let style = if *done {
                theme::dimmed()
            } else {
                theme::normal()
            };

            let line = Line::from(vec![
                Span::styled(checkbox, style),
                Span::raw(" "),
                Span::styled(*task, style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .title("Tasks")
        .borders(Borders::ALL)
        .border_style(theme::normal());

    let list = List::new(items).block(block);

    frame.render_widget(list, area);
}
