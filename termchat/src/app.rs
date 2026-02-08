//! Application state and event handling.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use termchat_proto::presence::PresenceStatus;

/// Which panel is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFocus {
    /// Input box is focused (default).
    Input,
    /// Sidebar conversation list is focused.
    Sidebar,
    /// Chat message list is focused.
    Chat,
    /// Task panel is focused.
    Tasks,
}

/// Display status for a task in the task panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskDisplayStatus {
    /// Task is open / not started.
    Open,
    /// Task is currently being worked on.
    InProgress,
    /// Task has been completed.
    Completed,
}

impl TaskDisplayStatus {
    /// Get the display symbol for this status.
    #[must_use]
    pub const fn symbol(&self) -> &'static str {
        match self {
            Self::Open => "[ ]",
            Self::InProgress => "[~]",
            Self::Completed => "[x]",
        }
    }

    /// Cycle to the next status: `Open` -> `InProgress` -> `Completed` -> `Open`.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Open => Self::InProgress,
            Self::InProgress => Self::Completed,
            Self::Completed => Self::Open,
        }
    }
}

/// A task for display in the task panel.
#[derive(Debug, Clone)]
pub struct DisplayTask {
    /// Unique task ID (for CRDT integration).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Current display status.
    pub status: TaskDisplayStatus,
    /// Optional assignee name.
    pub assignee: Option<String>,
    /// Sequential task number (displayed as #N).
    pub number: usize,
}

/// A message for display in the chat panel.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    /// Sender's display name.
    pub sender: String,
    /// Message content.
    pub content: String,
    /// Formatted timestamp (e.g., "14:23").
    pub timestamp: String,
    /// Status indicator (e.g., "sent", "delivered", "read").
    pub status: MessageStatus,
}

/// Message delivery status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    /// Message is being sent.
    Sending,
    /// Message was sent but not confirmed.
    Sent,
    /// Message was delivered to recipient.
    Delivered,
    /// Message was read by recipient.
    Read,
    /// Message failed to send.
    Failed,
}

impl MessageStatus {
    /// Get the display symbol for this status.
    #[must_use]
    pub const fn symbol(&self) -> &'static str {
        match self {
            Self::Sending => "\u{22ef}",
            Self::Sent => "\u{2713}",
            Self::Delivered | Self::Read => "\u{2713}\u{2713}",
            Self::Failed => "\u{2717}",
        }
    }
}

/// A conversation item for the sidebar.
#[derive(Debug, Clone)]
pub struct ConversationItem {
    /// Display name (e.g., "# general", "@ Alice").
    pub name: String,
    /// Number of unread messages.
    pub unread_count: usize,
    /// Preview of the last message.
    pub last_message_preview: Option<String>,
    /// Whether this conversation represents an AI agent participant.
    pub is_agent: bool,
    /// Presence status for DM conversations (None for rooms).
    pub presence: Option<PresenceStatus>,
}

/// Default duration after which typing indicator expires (3 seconds).
const DEFAULT_TYPING_TIMEOUT_SECS: u64 = 3;

/// Default maximum task title length in characters.
const DEFAULT_MAX_TASK_TITLE_LEN: usize = 256;

/// Main application state.
pub struct App {
    /// Current text input.
    pub input: String,
    /// Cursor position in input (character index).
    pub cursor_position: usize,
    /// Messages in the current conversation.
    pub messages: Vec<DisplayMessage>,
    /// Which panel is focused.
    pub focus: PanelFocus,
    /// Scroll offset for message list.
    pub message_scroll: usize,
    /// List of conversations/rooms.
    pub conversations: Vec<ConversationItem>,
    /// Selected conversation index.
    pub selected_conversation: usize,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Tasks displayed in the task panel.
    pub tasks: Vec<DisplayTask>,
    /// Currently selected task index.
    pub selected_task: usize,
    /// Presence status per peer (`peer_id` -> status).
    pub presence_map: HashMap<String, PresenceStatus>,
    /// Typing peers per room (`room_id` -> set of typing peer names).
    pub typing_peers: HashMap<String, HashSet<String>>,
    /// When the local user last typed (for typing timeout detection).
    pub typing_timer: Option<Instant>,
    /// Whether the local user is currently shown as typing.
    pub local_typing: bool,
    /// Typing indicator timeout in seconds (configurable).
    typing_timeout_secs: u64,
    /// Maximum task title length in characters (configurable).
    max_task_title_len: usize,
}

impl App {
    /// Create a new application with demo data.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn new() -> Self {
        let conversations = vec![
            ConversationItem {
                name: "# general".to_string(),
                unread_count: 0,
                last_message_preview: Some("You: Working on TUI".to_string()),
                is_agent: false,
                presence: None,
            },
            ConversationItem {
                name: "# dev".to_string(),
                unread_count: 3,
                last_message_preview: Some("Bob: Check out the PR".to_string()),
                is_agent: false,
                presence: None,
            },
            ConversationItem {
                name: "@ Alice".to_string(),
                unread_count: 1,
                last_message_preview: Some("Alice: See you tomorrow!".to_string()),
                is_agent: false,
                presence: Some(PresenceStatus::Online),
            },
        ];

        // Demo presence data
        let mut presence_map = HashMap::new();
        presence_map.insert("Alice".to_string(), PresenceStatus::Online);
        presence_map.insert("Bob".to_string(), PresenceStatus::Away);

        // Demo typing data — Alice is typing in general
        let mut typing_peers = HashMap::new();
        let mut general_typing = HashSet::new();
        general_typing.insert("Alice".to_string());
        typing_peers.insert("general".to_string(), general_typing);

        let messages = vec![
            DisplayMessage {
                sender: "Alice".to_string(),
                content: "Hey there!".to_string(),
                timestamp: "14:23".to_string(),
                status: MessageStatus::Read,
            },
            DisplayMessage {
                sender: "Bob".to_string(),
                content: "Hello!".to_string(),
                timestamp: "14:25".to_string(),
                status: MessageStatus::Read,
            },
            DisplayMessage {
                sender: "Alice".to_string(),
                content: "How's the new terminal chat app coming along?".to_string(),
                timestamp: "14:27".to_string(),
                status: MessageStatus::Read,
            },
            DisplayMessage {
                sender: "You".to_string(),
                content: "Working on TUI".to_string(),
                timestamp: "14:30".to_string(),
                status: MessageStatus::Delivered,
            },
            DisplayMessage {
                sender: "Bob".to_string(),
                content: "Nice! Can't wait to try it out.".to_string(),
                timestamp: "14:31".to_string(),
                status: MessageStatus::Read,
            },
            DisplayMessage {
                sender: "Alice".to_string(),
                content: "Me too! The terminal-first approach is really cool.".to_string(),
                timestamp: "14:32".to_string(),
                status: MessageStatus::Read,
            },
            DisplayMessage {
                sender: "You".to_string(),
                content: "Thanks! Building it with Ratatui and it's pretty straightforward so far."
                    .to_string(),
                timestamp: "14:35".to_string(),
                status: MessageStatus::Sent,
            },
            DisplayMessage {
                sender: "Bob".to_string(),
                content: "Ratatui is awesome. Are you using crossterm for the backend?".to_string(),
                timestamp: "14:36".to_string(),
                status: MessageStatus::Read,
            },
            DisplayMessage {
                sender: "You".to_string(),
                content: "Yep! Crossterm handles all the terminal events perfectly.".to_string(),
                timestamp: "14:38".to_string(),
                status: MessageStatus::Delivered,
            },
            DisplayMessage {
                sender: "Alice".to_string(),
                content: "Looking forward to the P2P networking part!".to_string(),
                timestamp: "14:40".to_string(),
                status: MessageStatus::Read,
            },
        ];

        Self {
            input: String::new(),
            cursor_position: 0,
            messages,
            focus: PanelFocus::Input,
            message_scroll: 0,
            conversations,
            selected_conversation: 0,
            should_quit: false,
            tasks: Vec::new(),
            selected_task: 0,
            presence_map,
            typing_peers,
            typing_timer: None,
            local_typing: false,
            typing_timeout_secs: DEFAULT_TYPING_TIMEOUT_SECS,
            max_task_title_len: DEFAULT_MAX_TASK_TITLE_LEN,
        }
    }

    /// Set the typing indicator timeout in seconds.
    #[must_use]
    pub const fn with_typing_timeout(mut self, secs: u64) -> Self {
        self.typing_timeout_secs = secs;
        self
    }

    /// Set the maximum task title length in characters.
    #[must_use]
    pub const fn with_max_task_title_len(mut self, len: usize) -> Self {
        self.max_task_title_len = len;
        self
    }

    /// Handle a key event.
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        // Global shortcuts
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                self.should_quit = true;
                return;
            }
            (KeyCode::Tab, KeyModifiers::SHIFT) => {
                self.cycle_focus_backward();
                return;
            }
            (KeyCode::Tab | KeyCode::BackTab, _) => {
                self.cycle_focus_forward();
                return;
            }
            _ => {}
        }

        // Focus-specific shortcuts
        match self.focus {
            PanelFocus::Input => self.handle_input_key(key),
            PanelFocus::Sidebar => self.handle_sidebar_key(key),
            PanelFocus::Chat => self.handle_chat_key(key),
            PanelFocus::Tasks => self.handle_tasks_key(key),
        }
    }

    /// Handle key event when input is focused.
    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.stop_typing();
                self.submit_message();
            }
            KeyCode::Char(c) => {
                self.enter_char(c);
                self.start_typing();
            }
            KeyCode::Backspace => {
                self.delete_char();
                if self.input.is_empty() {
                    self.stop_typing();
                } else {
                    self.start_typing();
                }
            }
            KeyCode::Left => self.move_cursor_left(),
            KeyCode::Right => self.move_cursor_right(),
            KeyCode::Home => self.cursor_position = 0,
            KeyCode::End => self.cursor_position = self.input.len(),
            _ => {}
        }
    }

    /// Handle key event when sidebar is focused.
    const fn handle_sidebar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.prev_conversation(),
            KeyCode::Down | KeyCode::Char('j') => self.next_conversation(),
            _ => {}
        }
    }

    /// Handle key event when chat is focused.
    const fn handle_chat_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(),
            _ => {}
        }
    }

    /// Handle key event when task panel is focused.
    fn handle_tasks_key(&mut self, key: KeyEvent) {
        if self.tasks.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_task + 1 < self.tasks.len() {
                    self.selected_task += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_task = self.selected_task.saturating_sub(1);
            }
            KeyCode::Enter => {
                let task = &mut self.tasks[self.selected_task];
                task.status = task.status.next();
            }
            _ => {}
        }
    }

    /// Handle the `/task` command with subcommands.
    fn handle_task_command(&mut self, args: &str) {
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let subcommand = parts[0];
        let sub_args = parts.get(1).copied().unwrap_or("").trim();

        match subcommand {
            "add" => self.task_cmd_add(sub_args),
            "done" => self.task_cmd_done(sub_args),
            "assign" => self.task_cmd_assign(sub_args),
            "delete" => self.task_cmd_delete(sub_args),
            "list" => self.task_cmd_list(),
            _ => {
                self.push_system_message("Usage: /task add|done|assign|delete|list".to_string());
            }
        }
    }

    /// `/task add <title>` — create a new task.
    fn task_cmd_add(&mut self, title: &str) {
        if title.is_empty() {
            self.push_system_message("Task title cannot be empty".to_string());
            return;
        }
        if title.len() > self.max_task_title_len {
            self.push_system_message(format!(
                "Task title too long (max {} characters)",
                self.max_task_title_len
            ));
            return;
        }
        let number = self.tasks.iter().map(|t| t.number).max().unwrap_or(0) + 1;
        self.tasks.push(DisplayTask {
            id: format!("local-{number}"),
            title: title.to_string(),
            status: TaskDisplayStatus::Open,
            assignee: None,
            number,
        });
        self.push_system_message(format!("Task created: {title}"));
    }

    /// `/task done <number>` — mark a task as completed.
    fn task_cmd_done(&mut self, args: &str) {
        let Some(number) = args.parse::<usize>().ok() else {
            self.push_system_message("Usage: /task done <number>".to_string());
            return;
        };
        if let Some(task) = self.tasks.iter_mut().find(|t| t.number == number) {
            task.status = TaskDisplayStatus::Completed;
            self.push_system_message(format!("Task #{number} marked as completed"));
        } else {
            self.push_system_message(format!("Task #{number} not found"));
        }
    }

    /// `/task assign <number> @<name>` — assign a task.
    fn task_cmd_assign(&mut self, args: &str) {
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        if parts.len() < 2 {
            self.push_system_message("Usage: /task assign <number> @<name>".to_string());
            return;
        }
        let Some(number) = parts[0].parse::<usize>().ok() else {
            self.push_system_message("Usage: /task assign <number> @<name>".to_string());
            return;
        };
        let name = parts[1].trim_start_matches('@').trim();
        if name.is_empty() {
            self.push_system_message("Usage: /task assign <number> @<name>".to_string());
            return;
        }
        if let Some(task) = self.tasks.iter_mut().find(|t| t.number == number) {
            task.assignee = Some(name.to_string());
            self.push_system_message(format!("Task #{number} assigned to {name}"));
        } else {
            self.push_system_message(format!("Task #{number} not found"));
        }
    }

    /// `/task delete <number>` — remove a task.
    fn task_cmd_delete(&mut self, args: &str) {
        let Some(number) = args.parse::<usize>().ok() else {
            self.push_system_message("Usage: /task delete <number>".to_string());
            return;
        };
        let before = self.tasks.len();
        self.tasks.retain(|t| t.number != number);
        if self.tasks.len() < before {
            // Adjust selected_task if needed
            if self.selected_task >= self.tasks.len() && !self.tasks.is_empty() {
                self.selected_task = self.tasks.len() - 1;
            }
            self.push_system_message(format!("Task #{number} deleted"));
        } else {
            self.push_system_message(format!("Task #{number} not found"));
        }
    }

    /// `/task list` — list all tasks as system messages.
    fn task_cmd_list(&mut self) {
        if self.tasks.is_empty() {
            self.push_system_message("No tasks".to_string());
            return;
        }
        let lines: Vec<String> = self
            .tasks
            .iter()
            .map(|task| {
                let assignee_str = task
                    .assignee
                    .as_ref()
                    .map_or(String::new(), |a| format!(" (@{a})"));
                let status = task.status.symbol();
                format!("#{} {status} {}{assignee_str}", task.number, task.title)
            })
            .collect();
        for line in lines {
            self.push_system_message(line);
        }
    }

    /// Cycle focus forward: Input -> Sidebar -> Chat -> Tasks -> Input.
    const fn cycle_focus_forward(&mut self) {
        self.focus = match self.focus {
            PanelFocus::Input => PanelFocus::Sidebar,
            PanelFocus::Sidebar => PanelFocus::Chat,
            PanelFocus::Chat => PanelFocus::Tasks,
            PanelFocus::Tasks => PanelFocus::Input,
        };
    }

    /// Cycle focus backward: Input -> Tasks -> Chat -> Sidebar -> Input.
    const fn cycle_focus_backward(&mut self) {
        self.focus = match self.focus {
            PanelFocus::Input => PanelFocus::Tasks,
            PanelFocus::Tasks => PanelFocus::Chat,
            PanelFocus::Chat => PanelFocus::Sidebar,
            PanelFocus::Sidebar => PanelFocus::Input,
        };
    }

    /// Submit the current input as a message or handle a `/` command.
    fn submit_message(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }

        let trimmed = self.input.trim().to_string();

        // Route slash commands
        if trimmed.starts_with('/') {
            self.handle_command(&trimmed);
            self.input.clear();
            self.cursor_position = 0;
            return;
        }

        let message = DisplayMessage {
            sender: "You".to_string(),
            content: self.input.clone(),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
            status: MessageStatus::Sent,
        };

        self.messages.push(message);
        self.input.clear();
        self.cursor_position = 0;

        // Auto-scroll to bottom
        self.message_scroll = self.messages.len().saturating_sub(1);
    }

    /// Handle a `/` command from the input box.
    fn handle_command(&mut self, input: &str) {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let command = parts[0];
        let args = parts.get(1).copied().unwrap_or("").trim();

        match command {
            "/invite-agent" => self.handle_invite_agent(args),
            "/task" => self.handle_task_command(args),
            _ => {
                self.push_system_message(format!("Unknown command: {command}"));
            }
        }
    }

    /// Handle the `/invite-agent <room-name>` command.
    ///
    /// Validates that a room name was provided, looks up the room via
    /// `RoomManager`, and initiates the agent bridge flow.
    fn handle_invite_agent(&mut self, room_name: &str) {
        if room_name.is_empty() {
            self.push_system_message("Usage: /invite-agent <room-name>".to_string());
            return;
        }

        // Room lookup will be wired to RoomManager once it's integrated
        // into the App. For now, check conversations list as a placeholder.
        let room_display = format!("# {room_name}");
        let room_exists = self.conversations.iter().any(|c| c.name == room_display);

        if !room_exists {
            self.push_system_message(format!("Room '{room_name}' not found"));
            return;
        }

        // Bridge spawning will be wired when AgentBridge is ready.
        // For now, show the status message indicating the command was accepted.
        let socket_path = format!("/tmp/termchat-agent-{}.sock", std::process::id());
        self.push_system_message(format!(
            "Agent bridge listening on {socket_path}. Waiting for agent to connect..."
        ));
    }

    /// Push a system-generated status message into the chat panel.
    pub fn push_system_message(&mut self, content: String) {
        self.messages.push(DisplayMessage {
            sender: "System".to_string(),
            content,
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
            status: MessageStatus::Delivered,
        });
        // Auto-scroll to bottom
        self.message_scroll = self.messages.len().saturating_sub(1);
    }

    /// Insert a character at the cursor position.
    fn enter_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    /// Delete the character before the cursor.
    fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.input.remove(self.cursor_position - 1);
            self.cursor_position -= 1;
        }
    }

    /// Move cursor left.
    const fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    /// Move cursor right.
    const fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    /// Scroll message list up.
    const fn scroll_up(&mut self) {
        if self.message_scroll > 0 {
            self.message_scroll -= 1;
        }
    }

    /// Scroll message list down.
    const fn scroll_down(&mut self) {
        if self.message_scroll < self.messages.len().saturating_sub(1) {
            self.message_scroll += 1;
        }
    }

    /// Select the previous conversation.
    const fn prev_conversation(&mut self) {
        if self.selected_conversation > 0 {
            self.selected_conversation -= 1;
        }
    }

    /// Select the next conversation.
    const fn next_conversation(&mut self) {
        if self.selected_conversation < self.conversations.len().saturating_sub(1) {
            self.selected_conversation += 1;
        }
    }

    /// Mark the local user as typing (resets the typing timer).
    fn start_typing(&mut self) {
        self.typing_timer = Some(Instant::now());
        self.local_typing = true;
    }

    /// Mark the local user as no longer typing.
    const fn stop_typing(&mut self) {
        self.typing_timer = None;
        self.local_typing = false;
    }

    /// Check if the typing timer has expired and update state accordingly.
    ///
    /// Should be called on each tick of the event loop.
    pub fn tick_typing(&mut self) {
        if let Some(started) = self.typing_timer
            && started.elapsed().as_secs() >= self.typing_timeout_secs
        {
            self.stop_typing();
        }
    }

    /// Update a peer's presence status.
    pub fn set_peer_presence(&mut self, peer_id: &str, status: PresenceStatus) {
        self.presence_map.insert(peer_id.to_string(), status);
        // Update matching DM conversation items
        let dm_prefix = format!("@ {peer_id}");
        for conv in &mut self.conversations {
            if conv.name == dm_prefix {
                conv.presence = Some(status);
            }
        }
    }

    /// Set a remote peer as typing in a room.
    pub fn set_peer_typing(&mut self, room_id: &str, peer_name: &str, is_typing: bool) {
        let entry = self.typing_peers.entry(room_id.to_string()).or_default();
        if is_typing {
            entry.insert(peer_name.to_string());
        } else {
            entry.remove(peer_name);
            if entry.is_empty() {
                self.typing_peers.remove(room_id);
            }
        }
    }

    /// Get the typing peers for the currently selected conversation.
    #[must_use]
    pub fn current_typing_peers(&self) -> Vec<&str> {
        if let Some(conv) = self.conversations.get(self.selected_conversation) {
            // Extract room name from "# room_name" format
            let room_id = conv.name.strip_prefix("# ").unwrap_or(&conv.name);
            if let Some(peers) = self.typing_peers.get(room_id) {
                return peers.iter().map(String::as_str).collect();
            }
        }
        Vec::new()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an App and set its input, then submit.
    fn submit_input(app: &mut App, text: &str) {
        app.input = text.to_string();
        app.cursor_position = text.len();
        app.submit_message();
    }

    #[test]
    fn invite_agent_no_args_shows_usage() {
        let mut app = App::new();
        let initial_count = app.messages.len();

        submit_input(&mut app, "/invite-agent");

        assert_eq!(app.messages.len(), initial_count + 1);
        let last = &app.messages[app.messages.len() - 1];
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Usage:"));
    }

    #[test]
    fn invite_agent_room_not_found() {
        let mut app = App::new();
        let initial_count = app.messages.len();

        submit_input(&mut app, "/invite-agent nonexistent");

        assert_eq!(app.messages.len(), initial_count + 1);
        let last = &app.messages[app.messages.len() - 1];
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("not found"));
    }

    #[test]
    fn invite_agent_valid_room_shows_bridge_status() {
        let mut app = App::new();
        // App::new() has a "# general" conversation
        let initial_count = app.messages.len();

        submit_input(&mut app, "/invite-agent general");

        assert_eq!(app.messages.len(), initial_count + 1);
        let last = &app.messages[app.messages.len() - 1];
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Agent bridge listening on"));
        assert!(last.content.contains("Waiting for agent to connect"));
    }

    #[test]
    fn unknown_command_shows_error() {
        let mut app = App::new();
        let initial_count = app.messages.len();

        submit_input(&mut app, "/foobar");

        assert_eq!(app.messages.len(), initial_count + 1);
        let last = &app.messages[app.messages.len() - 1];
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Unknown command: /foobar"));
    }

    #[test]
    fn slash_command_clears_input() {
        let mut app = App::new();
        submit_input(&mut app, "/invite-agent general");
        assert!(app.input.is_empty());
        assert_eq!(app.cursor_position, 0);
    }

    #[test]
    fn regular_message_not_treated_as_command() {
        let mut app = App::new();
        let initial_count = app.messages.len();

        submit_input(&mut app, "hello world");

        assert_eq!(app.messages.len(), initial_count + 1);
        let last = &app.messages[app.messages.len() - 1];
        assert_eq!(last.sender, "You");
        assert_eq!(last.content, "hello world");
    }

    #[test]
    fn system_message_auto_scrolls() {
        let mut app = App::new();
        app.push_system_message("test".to_string());
        assert_eq!(app.message_scroll, app.messages.len() - 1);
    }

    #[test]
    fn focus_cycle_forward_includes_tasks() {
        let mut app = App::new();
        assert_eq!(app.focus, PanelFocus::Input);
        app.cycle_focus_forward();
        assert_eq!(app.focus, PanelFocus::Sidebar);
        app.cycle_focus_forward();
        assert_eq!(app.focus, PanelFocus::Chat);
        app.cycle_focus_forward();
        assert_eq!(app.focus, PanelFocus::Tasks);
        app.cycle_focus_forward();
        assert_eq!(app.focus, PanelFocus::Input);
    }

    #[test]
    fn focus_cycle_backward_includes_tasks() {
        let mut app = App::new();
        assert_eq!(app.focus, PanelFocus::Input);
        app.cycle_focus_backward();
        assert_eq!(app.focus, PanelFocus::Tasks);
        app.cycle_focus_backward();
        assert_eq!(app.focus, PanelFocus::Chat);
        app.cycle_focus_backward();
        assert_eq!(app.focus, PanelFocus::Sidebar);
        app.cycle_focus_backward();
        assert_eq!(app.focus, PanelFocus::Input);
    }

    #[test]
    fn new_app_has_empty_tasks() {
        let app = App::new();
        assert!(app.tasks.is_empty());
        assert_eq!(app.selected_task, 0);
    }

    #[test]
    fn task_display_status_symbols() {
        assert_eq!(TaskDisplayStatus::Open.symbol(), "[ ]");
        assert_eq!(TaskDisplayStatus::InProgress.symbol(), "[~]");
        assert_eq!(TaskDisplayStatus::Completed.symbol(), "[x]");
    }

    #[test]
    fn task_display_status_next_cycle() {
        assert_eq!(
            TaskDisplayStatus::Open.next(),
            TaskDisplayStatus::InProgress
        );
        assert_eq!(
            TaskDisplayStatus::InProgress.next(),
            TaskDisplayStatus::Completed
        );
        assert_eq!(TaskDisplayStatus::Completed.next(), TaskDisplayStatus::Open);
    }

    // --- /task command tests (task #8) ---

    #[test]
    fn task_add_creates_task() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Fix the bug");
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "Fix the bug");
        assert_eq!(app.tasks[0].number, 1);
        assert_eq!(app.tasks[0].status, TaskDisplayStatus::Open);
        assert!(app.tasks[0].assignee.is_none());
    }

    #[test]
    fn task_add_auto_increments_number() {
        let mut app = App::new();
        submit_input(&mut app, "/task add First");
        submit_input(&mut app, "/task add Second");
        submit_input(&mut app, "/task add Third");
        assert_eq!(app.tasks[0].number, 1);
        assert_eq!(app.tasks[1].number, 2);
        assert_eq!(app.tasks[2].number, 3);
    }

    #[test]
    fn task_add_no_title_shows_error() {
        let mut app = App::new();
        let initial_count = app.messages.len();
        submit_input(&mut app, "/task add");
        assert!(app.tasks.is_empty());
        let last = &app.messages[app.messages.len() - 1];
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("title cannot be empty"));
        assert!(app.messages.len() > initial_count);
    }

    #[test]
    fn task_add_pushes_system_message() {
        let mut app = App::new();
        let initial_count = app.messages.len();
        submit_input(&mut app, "/task add Write tests");
        let last = &app.messages[app.messages.len() - 1];
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Task created: Write tests"));
        assert!(app.messages.len() > initial_count);
    }

    #[test]
    fn task_done_marks_completed() {
        let mut app = App::new();
        submit_input(&mut app, "/task add My task");
        submit_input(&mut app, "/task done 1");
        assert_eq!(app.tasks[0].status, TaskDisplayStatus::Completed);
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Task #1 marked as completed"));
    }

    #[test]
    fn task_done_not_found() {
        let mut app = App::new();
        submit_input(&mut app, "/task done 99");
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Task #99 not found"));
    }

    #[test]
    fn task_assign_sets_assignee() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Review PR");
        submit_input(&mut app, "/task assign 1 @alice");
        assert_eq!(app.tasks[0].assignee.as_deref(), Some("alice"));
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Task #1 assigned to alice"));
    }

    #[test]
    fn task_assign_without_at_prefix() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Review PR");
        submit_input(&mut app, "/task assign 1 bob");
        assert_eq!(app.tasks[0].assignee.as_deref(), Some("bob"));
    }

    #[test]
    fn task_delete_removes_task() {
        let mut app = App::new();
        submit_input(&mut app, "/task add First");
        submit_input(&mut app, "/task add Second");
        assert_eq!(app.tasks.len(), 2);
        submit_input(&mut app, "/task delete 1");
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "Second");
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Task #1 deleted"));
    }

    #[test]
    fn task_delete_not_found() {
        let mut app = App::new();
        submit_input(&mut app, "/task delete 42");
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Task #42 not found"));
    }

    #[test]
    fn task_list_empty() {
        let mut app = App::new();
        submit_input(&mut app, "/task list");
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("No tasks"));
    }

    #[test]
    fn task_list_shows_all() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Alpha");
        submit_input(&mut app, "/task add Beta");
        let before = app.messages.len();
        submit_input(&mut app, "/task list");
        // Should have added 2 system messages (one per task)
        assert_eq!(app.messages.len(), before + 2);
        assert!(app.messages[before].content.contains("#1 [ ] Alpha"));
        assert!(app.messages[before + 1].content.contains("#2 [ ] Beta"));
    }

    #[test]
    fn task_list_shows_assignee() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Do thing");
        submit_input(&mut app, "/task assign 1 @carol");
        let before = app.messages.len();
        submit_input(&mut app, "/task list");
        assert!(app.messages[before].content.contains("(@carol)"));
    }

    #[test]
    fn task_unknown_subcommand_shows_usage() {
        let mut app = App::new();
        submit_input(&mut app, "/task foobar");
        let last = &app.messages[app.messages.len() - 1];
        assert!(
            last.content
                .contains("Usage: /task add|done|assign|delete|list")
        );
    }

    // --- Keyboard handling tests (task #10) ---

    /// Helper: create a key event for a simple key code.
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Helper: add some tasks to the app and focus the task panel.
    fn app_with_tasks() -> App {
        let mut app = App::new();
        submit_input(&mut app, "/task add First");
        submit_input(&mut app, "/task add Second");
        submit_input(&mut app, "/task add Third");
        app.focus = PanelFocus::Tasks;
        app.selected_task = 0;
        app
    }

    #[test]
    fn tasks_key_down_moves_selection() {
        let mut app = app_with_tasks();
        app.handle_tasks_key(key(KeyCode::Down));
        assert_eq!(app.selected_task, 1);
        app.handle_tasks_key(key(KeyCode::Char('j')));
        assert_eq!(app.selected_task, 2);
    }

    #[test]
    fn tasks_key_down_clamps_at_end() {
        let mut app = app_with_tasks();
        app.selected_task = 2;
        app.handle_tasks_key(key(KeyCode::Down));
        assert_eq!(app.selected_task, 2);
    }

    #[test]
    fn tasks_key_up_moves_selection() {
        let mut app = app_with_tasks();
        app.selected_task = 2;
        app.handle_tasks_key(key(KeyCode::Up));
        assert_eq!(app.selected_task, 1);
        app.handle_tasks_key(key(KeyCode::Char('k')));
        assert_eq!(app.selected_task, 0);
    }

    #[test]
    fn tasks_key_up_clamps_at_zero() {
        let mut app = app_with_tasks();
        app.selected_task = 0;
        app.handle_tasks_key(key(KeyCode::Up));
        assert_eq!(app.selected_task, 0);
    }

    #[test]
    fn tasks_key_enter_toggles_status() {
        let mut app = app_with_tasks();
        assert_eq!(app.tasks[0].status, TaskDisplayStatus::Open);
        app.handle_tasks_key(key(KeyCode::Enter));
        assert_eq!(app.tasks[0].status, TaskDisplayStatus::InProgress);
        app.handle_tasks_key(key(KeyCode::Enter));
        assert_eq!(app.tasks[0].status, TaskDisplayStatus::Completed);
        app.handle_tasks_key(key(KeyCode::Enter));
        assert_eq!(app.tasks[0].status, TaskDisplayStatus::Open);
    }

    #[test]
    fn tasks_key_empty_tasks_does_nothing() {
        let mut app = App::new();
        app.focus = PanelFocus::Tasks;
        // Should not panic
        app.handle_tasks_key(key(KeyCode::Down));
        app.handle_tasks_key(key(KeyCode::Up));
        app.handle_tasks_key(key(KeyCode::Enter));
        assert_eq!(app.selected_task, 0);
    }

    // --- Command validation extension tests (task #9) ---

    #[test]
    fn task_add_title_too_long() {
        let mut app = App::new();
        let long_title = "x".repeat(257);
        submit_input(&mut app, &format!("/task add {long_title}"));
        assert!(app.tasks.is_empty());
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("title too long"));
    }

    #[test]
    fn task_add_title_exactly_max_length() {
        let mut app = App::new();
        let title = "y".repeat(256);
        submit_input(&mut app, &format!("/task add {title}"));
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, title);
    }

    #[test]
    fn task_done_invalid_number() {
        let mut app = App::new();
        submit_input(&mut app, "/task done abc");
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Usage:"));
    }

    #[test]
    fn task_assign_missing_name() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Test");
        submit_input(&mut app, "/task assign 1");
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Usage:"));
    }

    #[test]
    fn task_assign_not_found() {
        let mut app = App::new();
        submit_input(&mut app, "/task assign 99 @bob");
        let last = &app.messages[app.messages.len() - 1];
        assert!(last.content.contains("Task #99 not found"));
    }

    #[test]
    fn task_delete_adjusts_selected_task() {
        let mut app = App::new();
        submit_input(&mut app, "/task add A");
        submit_input(&mut app, "/task add B");
        app.selected_task = 1; // pointing at B
        submit_input(&mut app, "/task delete 2"); // delete B
        assert_eq!(app.selected_task, 0); // should adjust down
    }
}
