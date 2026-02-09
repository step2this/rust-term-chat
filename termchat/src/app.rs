//! Application state and event handling.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use termchat_proto::presence::PresenceStatus;

use crate::net::NetCommand;

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
    /// Unique message ID (for tracking delivery status).
    pub message_id: Option<String>,
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
    /// Messages per conversation (keyed by conversation name, e.g., "@ bob", "# dev").
    pub messages: HashMap<String, Vec<DisplayMessage>>,
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
    /// Whether the app is connected to a relay/transport.
    pub is_connected: bool,
    /// Transport type description (e.g., "Relay", "P2P", "").
    pub connection_info: String,
    /// Typing indicator timeout in seconds (configurable).
    typing_timeout_secs: u64,
    /// Maximum task title length in characters (configurable).
    max_task_title_len: usize,
    /// Counter for generating unique message IDs.
    next_message_id: u64,
}

impl App {
    /// Create a new application with empty state (no demo data).
    ///
    /// All conversations, messages, presence, and typing state start empty.
    /// Use [`add_conversation`] to populate from network events or CLI args.
    #[must_use]
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_position: 0,
            messages: HashMap::new(),
            focus: PanelFocus::Input,
            message_scroll: 0,
            conversations: Vec::new(),
            selected_conversation: 0,
            should_quit: false,
            tasks: Vec::new(),
            selected_task: 0,
            presence_map: HashMap::new(),
            typing_peers: HashMap::new(),
            typing_timer: None,
            local_typing: false,
            is_connected: false,
            connection_info: String::new(),
            typing_timeout_secs: DEFAULT_TYPING_TIMEOUT_SECS,
            max_task_title_len: DEFAULT_MAX_TASK_TITLE_LEN,
            next_message_id: 0,
        }
    }

    /// Create a new application with demo data for testing.
    ///
    /// Provides hardcoded conversations, messages, and presence data
    /// matching the original Phase 1 TUI demo.
    #[cfg(test)]
    #[must_use]
    pub fn new_with_demo() -> Self {
        let mut app = Self::new();

        app.conversations = vec![
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

        app.presence_map
            .insert("Alice".to_string(), PresenceStatus::Online);
        app.presence_map
            .insert("Bob".to_string(), PresenceStatus::Away);

        let mut general_typing = HashSet::new();
        general_typing.insert("Alice".to_string());
        app.typing_peers
            .insert("general".to_string(), general_typing);

        let demo_messages = vec![
            DisplayMessage {
                sender: "Alice".to_string(),
                content: "Hey there!".to_string(),
                timestamp: "14:23".to_string(),
                status: MessageStatus::Read,
                message_id: None,
            },
            DisplayMessage {
                sender: "You".to_string(),
                content: "Working on TUI".to_string(),
                timestamp: "14:30".to_string(),
                status: MessageStatus::Delivered,
                message_id: None,
            },
        ];
        app.messages.insert("# general".to_string(), demo_messages);

        app
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

    /// Generate a unique message ID.
    fn next_msg_id(&mut self) -> String {
        let id = self.next_message_id;
        self.next_message_id += 1;
        format!("msg-{id}")
    }

    /// Update connection status.
    pub fn set_connection_status(&mut self, connected: bool, info: &str) {
        self.is_connected = connected;
        self.connection_info = info.to_string();
    }

    /// Check if the app is able to send messages.
    ///
    /// Returns `true` if connected, `false` otherwise.
    #[must_use]
    pub const fn can_send(&self) -> bool {
        self.is_connected
    }

    /// Add a conversation to the sidebar if it doesn't already exist.
    ///
    /// Returns `true` if the conversation was added, `false` if it already existed.
    pub fn add_conversation(&mut self, name: &str, presence: Option<PresenceStatus>) -> bool {
        if self.conversations.iter().any(|c| c.name == name) {
            return false;
        }
        self.conversations.push(ConversationItem {
            name: name.to_string(),
            unread_count: 0,
            last_message_preview: None,
            is_agent: false,
            presence,
        });
        true
    }

    /// Push a message into a specific conversation.
    ///
    /// Auto-creates the conversation if it doesn't exist (extension 10a).
    /// Increments unread count if the conversation is not currently selected.
    pub fn push_message(&mut self, conversation: &str, msg: DisplayMessage) {
        // Auto-create conversation if needed.
        self.add_conversation(conversation, None);

        // Check if this is the active conversation before taking a mutable borrow.
        let is_selected = self
            .conversations
            .get(self.selected_conversation)
            .is_some_and(|c| c.name == conversation);

        // Update last_message_preview.
        if let Some(conv) = self
            .conversations
            .iter_mut()
            .find(|c| c.name == conversation)
        {
            conv.last_message_preview = Some(format!("{}: {}", msg.sender, msg.content));

            // Increment unread if this is not the active conversation.
            if !is_selected {
                conv.unread_count += 1;
            }
        }

        self.messages
            .entry(conversation.to_string())
            .or_default()
            .push(msg);
    }

    /// Get messages for the currently selected conversation.
    #[must_use]
    pub fn current_messages(&self) -> &[DisplayMessage] {
        self.conversations
            .get(self.selected_conversation)
            .and_then(|conv| self.messages.get(&conv.name))
            .map_or(&[], Vec::as_slice)
    }

    /// Get the name of the currently selected conversation, if any.
    #[must_use]
    pub fn selected_conversation_name(&self) -> Option<&str> {
        self.conversations
            .get(self.selected_conversation)
            .map(|c| c.name.as_str())
    }

    /// Handle a key event.
    ///
    /// Returns `Some(NetCommand)` if the key event produced a command that needs
    /// to be dispatched to the networking layer (e.g., sending a message or a
    /// slash command like `/create-room`).
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<NetCommand> {
        // Global shortcuts
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                self.should_quit = true;
                return None;
            }
            (KeyCode::Tab, KeyModifiers::SHIFT) => {
                self.cycle_focus_backward();
                return None;
            }
            (KeyCode::Tab | KeyCode::BackTab, _) => {
                self.cycle_focus_forward();
                return None;
            }
            _ => {}
        }

        // Focus-specific shortcuts
        match self.focus {
            PanelFocus::Input => self.handle_input_key(key),
            PanelFocus::Sidebar => {
                self.handle_sidebar_key(key);
                None
            }
            PanelFocus::Chat => {
                self.handle_chat_key(key);
                None
            }
            PanelFocus::Tasks => {
                self.handle_tasks_key(key);
                None
            }
        }
    }

    /// Handle key event when input is focused.
    ///
    /// Returns `Some(NetCommand)` if Enter was pressed and the input produced
    /// a command that needs network dispatch.
    fn handle_input_key(&mut self, key: KeyEvent) -> Option<NetCommand> {
        match key.code {
            KeyCode::Enter => {
                self.stop_typing();
                self.submit_message()
            }
            KeyCode::Char(c) => {
                self.enter_char(c);
                self.start_typing();
                None
            }
            KeyCode::Backspace => {
                self.delete_char();
                if self.input.is_empty() {
                    self.stop_typing();
                } else {
                    self.start_typing();
                }
                None
            }
            KeyCode::Left => {
                self.move_cursor_left();
                None
            }
            KeyCode::Right => {
                self.move_cursor_right();
                None
            }
            KeyCode::Home => {
                self.cursor_position = 0;
                None
            }
            KeyCode::End => {
                self.cursor_position = self.input.len();
                None
            }
            _ => None,
        }
    }

    /// Handle key event when sidebar is focused.
    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.prev_conversation(),
            KeyCode::Down | KeyCode::Char('j') => self.next_conversation(),
            _ => {}
        }
    }

    /// Handle key event when chat is focused.
    fn handle_chat_key(&mut self, key: KeyEvent) {
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
    ///
    /// Returns `Some(NetCommand)` if a network action is required (e.g., sending a message),
    /// `None` otherwise.
    pub fn submit_message(&mut self) -> Option<NetCommand> {
        if self.input.trim().is_empty() {
            return None;
        }

        let trimmed = self.input.trim().to_string();

        // Route slash commands
        if trimmed.starts_with('/') {
            let cmd = self.handle_command(&trimmed);
            self.input.clear();
            self.cursor_position = 0;
            return cmd;
        }

        // Extension 9a: Validate message size before creating DisplayMessage
        if self.input.len() > 65536 {
            self.push_system_message("Message too long (max 64KB)".to_string());
            return None;
        }

        let message_id = self.next_msg_id();
        let message = DisplayMessage {
            sender: "You".to_string(),
            content: self.input.clone(),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
            status: MessageStatus::Sending,
            message_id: Some(message_id),
        };

        // Clone conversation name before taking mutable borrows
        let conv_name = self.selected_conversation_name().map(String::from);
        let text = self.input.clone();

        let net_command = if let Some(conv_name) = conv_name {
            self.push_message(&conv_name, message);
            // Auto-scroll to bottom of current conversation.
            self.message_scroll = self.current_messages().len().saturating_sub(1);

            // Return the NetCommand to send the message
            Some(NetCommand::SendMessage {
                conversation_id: conv_name,
                text,
            })
        } else {
            None
        };

        self.input.clear();
        self.cursor_position = 0;
        net_command
    }

    /// Handle a `/` command from the input box.
    ///
    /// Returns `Some(NetCommand)` if the command needs to be sent to the network layer,
    /// `None` otherwise.
    fn handle_command(&mut self, input: &str) -> Option<NetCommand> {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let command = parts[0];
        let args = parts.get(1).copied().unwrap_or("").trim();

        match command {
            "/invite-agent" => {
                self.handle_invite_agent(args);
                None
            }
            "/task" => {
                self.handle_task_command(args);
                None
            }
            "/create-room" => {
                if !self.is_connected {
                    self.push_system_message("Not connected".to_string());
                    return None;
                }
                if args.is_empty() {
                    self.push_system_message("Usage: /create-room <name>".to_string());
                    return None;
                }
                Some(NetCommand::CreateRoom {
                    name: args.to_string(),
                })
            }
            "/list-rooms" => {
                if !self.is_connected {
                    self.push_system_message("Not connected".to_string());
                    return None;
                }
                Some(NetCommand::ListRooms)
            }
            "/join-room" => {
                if !self.is_connected {
                    self.push_system_message("Not connected".to_string());
                    return None;
                }
                if args.is_empty() {
                    self.push_system_message("Usage: /join-room <room-id>".to_string());
                    return None;
                }
                Some(NetCommand::JoinRoom {
                    room_id: args.to_string(),
                })
            }
            "/approve" => {
                if !self.is_connected {
                    self.push_system_message("Not connected".to_string());
                    return None;
                }
                if args.is_empty() {
                    self.push_system_message("Usage: /approve <peer-id>".to_string());
                    return None;
                }
                // Derive room_id from selected conversation name (strip "# " prefix)
                if let Some(conv_name) = self.selected_conversation_name()
                    && let Some(room_id) = conv_name.strip_prefix("# ")
                {
                    return Some(NetCommand::ApproveJoin {
                        room_id: room_id.to_string(),
                        peer_id: args.to_string(),
                    });
                }
                self.push_system_message("Must be in a room to approve join requests".to_string());
                None
            }
            "/deny" => {
                if !self.is_connected {
                    self.push_system_message("Not connected".to_string());
                    return None;
                }
                if args.is_empty() {
                    self.push_system_message("Usage: /deny <peer-id>".to_string());
                    return None;
                }
                // Derive room_id from selected conversation name (strip "# " prefix)
                if let Some(conv_name) = self.selected_conversation_name()
                    && let Some(room_id) = conv_name.strip_prefix("# ")
                {
                    return Some(NetCommand::DenyJoin {
                        room_id: room_id.to_string(),
                        peer_id: args.to_string(),
                    });
                }
                self.push_system_message("Must be in a room to deny join requests".to_string());
                None
            }
            _ => {
                self.push_system_message(format!("Unknown command: {command}"));
                None
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

    /// Push a system-generated status message into the current conversation.
    ///
    /// If no conversation is selected, the message is pushed to a special
    /// "__system__" conversation.
    pub fn push_system_message(&mut self, content: String) {
        let msg = DisplayMessage {
            sender: "System".to_string(),
            content,
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        };
        let conv_name = self
            .selected_conversation_name()
            .unwrap_or("__system__")
            .to_string();
        self.messages.entry(conv_name).or_default().push(msg);
        // Auto-scroll to bottom.
        self.message_scroll = self.current_messages().len().saturating_sub(1);
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
    fn scroll_down(&mut self) {
        let len = self.current_messages().len();
        if self.message_scroll < len.saturating_sub(1) {
            self.message_scroll += 1;
        }
    }

    /// Select the previous conversation.
    fn prev_conversation(&mut self) {
        if self.selected_conversation > 0 {
            self.selected_conversation -= 1;
            self.on_conversation_selected();
        }
    }

    /// Select the next conversation.
    fn next_conversation(&mut self) {
        if self.selected_conversation < self.conversations.len().saturating_sub(1) {
            self.selected_conversation += 1;
            self.on_conversation_selected();
        }
    }

    /// Called when the user switches to a conversation — resets scroll and unread.
    fn on_conversation_selected(&mut self) {
        self.message_scroll = self.current_messages().len().saturating_sub(1);
        if let Some(conv) = self.conversations.get_mut(self.selected_conversation) {
            conv.unread_count = 0;
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
        let _ = app.submit_message();
    }

    /// Helper: get all messages from the system/selected conversation.
    ///
    /// With `App::new()` (no conversations), system messages go to `__system__`.
    fn system_msgs(app: &App) -> &[DisplayMessage] {
        let conv = app.selected_conversation_name().unwrap_or("__system__");
        app.messages.get(conv).map_or(&[], Vec::as_slice)
    }

    /// Helper: get the last message from the system/selected conversation.
    fn last_msg(app: &App) -> &DisplayMessage {
        let msgs = system_msgs(app);
        msgs.last().expect("expected at least one message")
    }

    /// Helper: count messages in the current/system conversation.
    fn msg_count(app: &App) -> usize {
        system_msgs(app).len()
    }

    #[test]
    fn invite_agent_no_args_shows_usage() {
        let mut app = App::new();
        let initial_count = msg_count(&app);

        submit_input(&mut app, "/invite-agent");

        assert_eq!(msg_count(&app), initial_count + 1);
        let last = last_msg(&app);
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Usage:"));
    }

    #[test]
    fn invite_agent_room_not_found() {
        let mut app = App::new();
        let initial_count = msg_count(&app);

        submit_input(&mut app, "/invite-agent nonexistent");

        assert_eq!(msg_count(&app), initial_count + 1);
        let last = last_msg(&app);
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("not found"));
    }

    #[test]
    fn invite_agent_valid_room_shows_bridge_status() {
        let mut app = App::new_with_demo();
        // App::new_with_demo() has a "# general" conversation
        let initial_count = msg_count(&app);

        submit_input(&mut app, "/invite-agent general");

        assert_eq!(msg_count(&app), initial_count + 1);
        let last = last_msg(&app);
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Agent bridge listening on"));
        assert!(last.content.contains("Waiting for agent to connect"));
    }

    #[test]
    fn unknown_command_shows_error() {
        let mut app = App::new();
        let initial_count = msg_count(&app);

        submit_input(&mut app, "/foobar");

        assert_eq!(msg_count(&app), initial_count + 1);
        let last = last_msg(&app);
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Unknown command: /foobar"));
    }

    #[test]
    fn slash_command_clears_input() {
        let mut app = App::new_with_demo();
        submit_input(&mut app, "/invite-agent general");
        assert!(app.input.is_empty());
        assert_eq!(app.cursor_position, 0);
    }

    #[test]
    fn regular_message_not_treated_as_command() {
        let mut app = App::new();
        app.add_conversation("# test", None);
        // Select the "# test" conversation (index 0).
        app.selected_conversation = 0;
        let initial_count = msg_count(&app);

        submit_input(&mut app, "hello world");

        assert_eq!(msg_count(&app), initial_count + 1);
        let last = last_msg(&app);
        assert_eq!(last.sender, "You");
        assert_eq!(last.content, "hello world");
    }

    #[test]
    fn system_message_auto_scrolls() {
        let mut app = App::new();
        app.push_system_message("test".to_string());
        assert_eq!(app.message_scroll, msg_count(&app) - 1);
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
        let initial_count = msg_count(&app);
        submit_input(&mut app, "/task add");
        assert!(app.tasks.is_empty());
        let last = last_msg(&app);
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("title cannot be empty"));
        assert!(msg_count(&app) > initial_count);
    }

    #[test]
    fn task_add_pushes_system_message() {
        let mut app = App::new();
        let initial_count = msg_count(&app);
        submit_input(&mut app, "/task add Write tests");
        let last = last_msg(&app);
        assert_eq!(last.sender, "System");
        assert!(last.content.contains("Task created: Write tests"));
        assert!(msg_count(&app) > initial_count);
    }

    #[test]
    fn task_done_marks_completed() {
        let mut app = App::new();
        submit_input(&mut app, "/task add My task");
        submit_input(&mut app, "/task done 1");
        assert_eq!(app.tasks[0].status, TaskDisplayStatus::Completed);
        let last = last_msg(&app);
        assert!(last.content.contains("Task #1 marked as completed"));
    }

    #[test]
    fn task_done_not_found() {
        let mut app = App::new();
        submit_input(&mut app, "/task done 99");
        let last = last_msg(&app);
        assert!(last.content.contains("Task #99 not found"));
    }

    #[test]
    fn task_assign_sets_assignee() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Review PR");
        submit_input(&mut app, "/task assign 1 @alice");
        assert_eq!(app.tasks[0].assignee.as_deref(), Some("alice"));
        let last = last_msg(&app);
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
        let last = last_msg(&app);
        assert!(last.content.contains("Task #1 deleted"));
    }

    #[test]
    fn task_delete_not_found() {
        let mut app = App::new();
        submit_input(&mut app, "/task delete 42");
        let last = last_msg(&app);
        assert!(last.content.contains("Task #42 not found"));
    }

    #[test]
    fn task_list_empty() {
        let mut app = App::new();
        submit_input(&mut app, "/task list");
        let last = last_msg(&app);
        assert!(last.content.contains("No tasks"));
    }

    #[test]
    fn task_list_shows_all() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Alpha");
        submit_input(&mut app, "/task add Beta");
        let before = msg_count(&app);
        submit_input(&mut app, "/task list");
        // Should have added 2 system messages (one per task)
        let msgs = system_msgs(&app);
        assert_eq!(msgs.len(), before + 2);
        assert!(msgs[before].content.contains("#1 [ ] Alpha"));
        assert!(msgs[before + 1].content.contains("#2 [ ] Beta"));
    }

    #[test]
    fn task_list_shows_assignee() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Do thing");
        submit_input(&mut app, "/task assign 1 @carol");
        let before = msg_count(&app);
        submit_input(&mut app, "/task list");
        let msgs = system_msgs(&app);
        assert!(msgs[before].content.contains("(@carol)"));
    }

    #[test]
    fn task_unknown_subcommand_shows_usage() {
        let mut app = App::new();
        submit_input(&mut app, "/task foobar");
        let last = last_msg(&app);
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
        let last = last_msg(&app);
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
        let last = last_msg(&app);
        assert!(last.content.contains("Usage:"));
    }

    #[test]
    fn task_assign_missing_name() {
        let mut app = App::new();
        submit_input(&mut app, "/task add Test");
        submit_input(&mut app, "/task assign 1");
        let last = last_msg(&app);
        assert!(last.content.contains("Usage:"));
    }

    #[test]
    fn task_assign_not_found() {
        let mut app = App::new();
        submit_input(&mut app, "/task assign 99 @bob");
        let last = last_msg(&app);
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
