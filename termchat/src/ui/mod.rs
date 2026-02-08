//! Terminal UI rendering.

pub mod chat_panel;
pub mod sidebar;
pub mod status_bar;
pub mod task_panel;
pub mod theme;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::app::App;

/// Main draw function for the entire UI.
pub fn draw(frame: &mut Frame, app: &App) {
    // Create main layout with status bar at bottom
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    // Create three-column layout for content
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Sidebar
            Constraint::Percentage(55), // Chat
            Constraint::Percentage(25), // Tasks
        ])
        .split(content_area);

    // Render each panel
    sidebar::render(frame, content_chunks[0], app);
    chat_panel::render(frame, content_chunks[1], app);
    task_panel::render(frame, content_chunks[2], app);

    // Render status bar
    status_bar::render(frame, status_area, app);
}
