//! Application state and event handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Which panel is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFocus {
    /// Input box is focused (default).
    Input,
    /// Sidebar conversation list is focused.
    Sidebar,
    /// Chat message list is focused.
    Chat,
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
}

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
}

impl App {
    /// Create a new application with demo data.
    #[must_use]
    pub fn new() -> Self {
        let conversations = vec![
            ConversationItem {
                name: "# general".to_string(),
                unread_count: 0,
                last_message_preview: Some("You: Working on TUI".to_string()),
            },
            ConversationItem {
                name: "# dev".to_string(),
                unread_count: 3,
                last_message_preview: Some("Bob: Check out the PR".to_string()),
            },
            ConversationItem {
                name: "@ Alice".to_string(),
                unread_count: 1,
                last_message_preview: Some("Alice: See you tomorrow!".to_string()),
            },
        ];

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
        }
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
        }
    }

    /// Handle key event when input is focused.
    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.submit_message(),
            KeyCode::Char(c) => self.enter_char(c),
            KeyCode::Backspace => self.delete_char(),
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

    /// Cycle focus forward: Input -> Sidebar -> Chat -> Input.
    const fn cycle_focus_forward(&mut self) {
        self.focus = match self.focus {
            PanelFocus::Input => PanelFocus::Sidebar,
            PanelFocus::Sidebar => PanelFocus::Chat,
            PanelFocus::Chat => PanelFocus::Input,
        };
    }

    /// Cycle focus backward: Input -> Chat -> Sidebar -> Input.
    const fn cycle_focus_backward(&mut self) {
        self.focus = match self.focus {
            PanelFocus::Input => PanelFocus::Chat,
            PanelFocus::Chat => PanelFocus::Sidebar,
            PanelFocus::Sidebar => PanelFocus::Input,
        };
    }

    /// Submit the current input as a message.
    fn submit_message(&mut self) {
        if self.input.trim().is_empty() {
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
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
