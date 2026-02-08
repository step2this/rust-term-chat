//! JSON lines wire protocol for agent - bridge communication.
//!
//! Defines the [`AgentMessage`] (agent -> bridge) and [`BridgeMessage`]
//! (bridge -> agent) enums, along with serialization helpers and
//! agent ID validation utilities.

use serde::{Deserialize, Serialize};

use super::AgentError;

/// Current protocol version for the agent bridge handshake.
pub const PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Agent -> Bridge messages
// ---------------------------------------------------------------------------

/// Messages sent from an external agent to the bridge.
///
/// All variants are JSON-serialized as `{"type": "<snake_case_variant>", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    /// Initial handshake from the agent declaring its identity.
    Hello {
        /// Protocol version the agent supports (must match [`PROTOCOL_VERSION`]).
        protocol_version: u32,
        /// Unique identifier for this agent (validated and sanitized on receipt).
        agent_id: String,
        /// Human-readable display name shown in the room.
        display_name: String,
        /// List of capability strings (e.g. `["chat", "code_review"]`).
        capabilities: Vec<String>,
    },
    /// Agent wants to send a text message to the room.
    SendMessage {
        /// The message content to deliver.
        content: String,
    },
    /// Agent is gracefully disconnecting.
    Goodbye,
    /// Response to a [`BridgeMessage::Ping`].
    Pong,
}

// ---------------------------------------------------------------------------
// Bridge -> Agent messages
// ---------------------------------------------------------------------------

/// Messages sent from the bridge to an external agent.
///
/// All variants are JSON-serialized as `{"type": "<snake_case_variant>", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeMessage {
    /// Handshake response after a successful [`AgentMessage::Hello`].
    Welcome {
        /// The room the agent has joined.
        room_id: String,
        /// Human-readable room name.
        room_name: String,
        /// Current room membership list.
        members: Vec<BridgeMemberInfo>,
        /// Recent message history for context.
        history: Vec<BridgeHistoryEntry>,
    },
    /// A chat message from another room participant.
    RoomMessage {
        /// Peer ID of the sender.
        sender_id: String,
        /// Display name of the sender.
        sender_name: String,
        /// Text content of the message.
        content: String,
        /// ISO 8601 timestamp string (e.g. `"2025-01-15T12:34:56Z"`).
        timestamp: String,
    },
    /// A room membership change notification.
    MembershipUpdate {
        /// What changed: `"joined"`, `"left"`, `"promoted"`, or `"demoted"`.
        action: String,
        /// Peer ID of the affected member.
        peer_id: String,
        /// Display name of the affected member.
        display_name: String,
        /// Whether the affected member is an AI agent.
        is_agent: bool,
    },
    /// An error from the bridge.
    Error {
        /// Machine-readable error code (e.g. `"invalid_agent_id"`, `"room_not_found"`).
        code: String,
        /// Human-readable error description.
        message: String,
    },
    /// Heartbeat probe sent periodically by the bridge.
    Ping,
}

// ---------------------------------------------------------------------------
// Helper structs
// ---------------------------------------------------------------------------

/// Room member information sent to agents in the [`BridgeMessage::Welcome`] payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeMemberInfo {
    /// Member's peer ID.
    pub peer_id: String,
    /// Member's display name.
    pub display_name: String,
    /// Whether this member is a room admin.
    pub is_admin: bool,
    /// Whether this member is an AI agent.
    pub is_agent: bool,
}

/// A single history entry sent to agents in the [`BridgeMessage::Welcome`] payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeHistoryEntry {
    /// Peer ID of the message sender.
    pub sender_id: String,
    /// Display name of the message sender.
    pub sender_name: String,
    /// Text content of the message.
    pub content: String,
    /// ISO 8601 timestamp string.
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Agent ID validation
// ---------------------------------------------------------------------------

/// Maximum length for an agent ID in characters.
const MAX_AGENT_ID_LEN: usize = 64;

/// Prefix prepended to agent IDs to form unique peer IDs.
const AGENT_PEER_PREFIX: &str = "agent:";

/// Validates and sanitizes an agent ID string.
///
/// Processing steps:
/// 1. Strip all control characters (Unicode category Cc).
/// 2. Trim leading and trailing whitespace.
/// 3. Truncate to [`MAX_AGENT_ID_LEN`] characters.
/// 4. If the result is empty, return an error.
///
/// # Errors
///
/// Returns [`AgentError::InvalidAgentId`] if the ID is empty or contains
/// only invalid characters after sanitization.
pub fn validate_agent_id(id: &str) -> Result<String, AgentError> {
    let sanitized: String = id.chars().filter(|c| !c.is_control()).collect();
    let sanitized = sanitized.trim();

    if sanitized.is_empty() {
        return Err(AgentError::InvalidAgentId(
            "agent ID is empty after sanitization".to_string(),
        ));
    }

    // Truncate to MAX_AGENT_ID_LEN characters (not bytes)
    let truncated: String = sanitized.chars().take(MAX_AGENT_ID_LEN).collect();
    Ok(truncated)
}

/// Creates a unique peer ID for an agent, avoiding collisions with existing peers.
///
/// The base ID is prefixed with `"agent:"`. If the resulting ID already exists
/// in `existing`, a numeric suffix (`-2`, `-3`, ...) is appended until a unique
/// ID is found.
#[must_use]
pub fn make_unique_agent_peer_id(base_id: &str, existing: &[String]) -> String {
    let candidate = format!("{AGENT_PEER_PREFIX}{base_id}");
    if !existing.contains(&candidate) {
        return candidate;
    }

    let mut suffix = 2u64;
    loop {
        let candidate = format!("{AGENT_PEER_PREFIX}{base_id}-{suffix}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

// ---------------------------------------------------------------------------
// Encode / Decode helpers (JSON lines)
// ---------------------------------------------------------------------------

/// Serializes a value to a JSON line (JSON + trailing newline).
///
/// # Errors
///
/// Returns [`AgentError::JsonError`] if serialization fails.
pub fn encode_line<T: Serialize>(value: &T) -> Result<String, AgentError> {
    let mut json = serde_json::to_string(value)?;
    json.push('\n');
    Ok(json)
}

/// Deserializes a JSON line into a value.
///
/// The input is trimmed before parsing so trailing newlines are tolerated.
///
/// # Errors
///
/// Returns [`AgentError::JsonError`] if deserialization fails.
pub fn decode_line<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, AgentError> {
    Ok(serde_json::from_str(line.trim())?)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- AgentMessage round-trip tests ---

    #[test]
    fn agent_hello_round_trip() {
        let msg = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "claude-42".to_string(),
            display_name: "Claude".to_string(),
            capabilities: vec!["chat".to_string(), "code_review".to_string()],
        };
        let line = encode_line(&msg).expect("encode");
        assert!(line.ends_with('\n'));
        let decoded: AgentMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn agent_hello_json_shape() {
        let msg = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "bot-1".to_string(),
            display_name: "Bot".to_string(),
            capabilities: vec![],
        };
        let json = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(json["type"], "hello");
        assert_eq!(json["protocol_version"], 1);
        assert_eq!(json["agent_id"], "bot-1");
        assert_eq!(json["display_name"], "Bot");
    }

    #[test]
    fn agent_send_message_round_trip() {
        let msg = AgentMessage::SendMessage {
            content: "Hello, world!".to_string(),
        };
        let line = encode_line(&msg).expect("encode");
        let decoded: AgentMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn agent_send_message_json_shape() {
        let msg = AgentMessage::SendMessage {
            content: "hi".to_string(),
        };
        let json = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(json["type"], "send_message");
        assert_eq!(json["content"], "hi");
    }

    #[test]
    fn agent_goodbye_round_trip() {
        let msg = AgentMessage::Goodbye;
        let line = encode_line(&msg).expect("encode");
        let decoded: AgentMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn agent_goodbye_json_shape() {
        let json = serde_json::to_value(&AgentMessage::Goodbye).expect("to_value");
        assert_eq!(json["type"], "goodbye");
    }

    #[test]
    fn agent_pong_round_trip() {
        let msg = AgentMessage::Pong;
        let line = encode_line(&msg).expect("encode");
        let decoded: AgentMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn agent_pong_json_shape() {
        let json = serde_json::to_value(&AgentMessage::Pong).expect("to_value");
        assert_eq!(json["type"], "pong");
    }

    // --- BridgeMessage round-trip tests ---

    #[test]
    fn bridge_welcome_round_trip() {
        let msg = BridgeMessage::Welcome {
            room_id: "room-abc".to_string(),
            room_name: "General".to_string(),
            members: vec![
                BridgeMemberInfo {
                    peer_id: "peer-alice".to_string(),
                    display_name: "Alice".to_string(),
                    is_admin: true,
                    is_agent: false,
                },
                BridgeMemberInfo {
                    peer_id: "agent:claude-1".to_string(),
                    display_name: "Claude".to_string(),
                    is_admin: false,
                    is_agent: true,
                },
            ],
            history: vec![BridgeHistoryEntry {
                sender_id: "peer-alice".to_string(),
                sender_name: "Alice".to_string(),
                content: "Hey everyone".to_string(),
                timestamp: "2025-01-15T12:00:00Z".to_string(),
            }],
        };
        let line = encode_line(&msg).expect("encode");
        let decoded: BridgeMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn bridge_welcome_json_shape() {
        let msg = BridgeMessage::Welcome {
            room_id: "r1".to_string(),
            room_name: "Test".to_string(),
            members: vec![],
            history: vec![],
        };
        let json = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(json["type"], "welcome");
        assert_eq!(json["room_id"], "r1");
        assert_eq!(json["room_name"], "Test");
    }

    #[test]
    fn bridge_welcome_empty_members_and_history() {
        let msg = BridgeMessage::Welcome {
            room_id: "r1".to_string(),
            room_name: "Empty".to_string(),
            members: vec![],
            history: vec![],
        };
        let line = encode_line(&msg).expect("encode");
        let decoded: BridgeMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn bridge_room_message_round_trip() {
        let msg = BridgeMessage::RoomMessage {
            sender_id: "peer-bob".to_string(),
            sender_name: "Bob".to_string(),
            content: "Hello from Bob".to_string(),
            timestamp: "2025-01-15T12:05:00Z".to_string(),
        };
        let line = encode_line(&msg).expect("encode");
        let decoded: BridgeMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn bridge_room_message_json_shape() {
        let msg = BridgeMessage::RoomMessage {
            sender_id: "p1".to_string(),
            sender_name: "User".to_string(),
            content: "hi".to_string(),
            timestamp: "t".to_string(),
        };
        let json = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(json["type"], "room_message");
        assert_eq!(json["sender_id"], "p1");
    }

    #[test]
    fn bridge_membership_update_round_trip() {
        let msg = BridgeMessage::MembershipUpdate {
            action: "joined".to_string(),
            peer_id: "peer-charlie".to_string(),
            display_name: "Charlie".to_string(),
            is_agent: false,
        };
        let line = encode_line(&msg).expect("encode");
        let decoded: BridgeMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn bridge_membership_update_json_shape() {
        let msg = BridgeMessage::MembershipUpdate {
            action: "left".to_string(),
            peer_id: "p".to_string(),
            display_name: "D".to_string(),
            is_agent: true,
        };
        let json = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(json["type"], "membership_update");
        assert_eq!(json["action"], "left");
        assert!(json["is_agent"].as_bool().expect("is_agent is bool"));
    }

    #[test]
    fn bridge_error_round_trip() {
        let msg = BridgeMessage::Error {
            code: "room_not_found".to_string(),
            message: "The room does not exist".to_string(),
        };
        let line = encode_line(&msg).expect("encode");
        let decoded: BridgeMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn bridge_error_json_shape() {
        let msg = BridgeMessage::Error {
            code: "timeout".to_string(),
            message: "timed out".to_string(),
        };
        let json = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(json["type"], "error");
        assert_eq!(json["code"], "timeout");
    }

    #[test]
    fn bridge_ping_round_trip() {
        let msg = BridgeMessage::Ping;
        let line = encode_line(&msg).expect("encode");
        let decoded: BridgeMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn bridge_ping_json_shape() {
        let json = serde_json::to_value(&BridgeMessage::Ping).expect("to_value");
        assert_eq!(json["type"], "ping");
    }

    // --- Helper struct tests ---

    #[test]
    fn bridge_member_info_round_trip() {
        let info = BridgeMemberInfo {
            peer_id: "peer-alice".to_string(),
            display_name: "Alice".to_string(),
            is_admin: true,
            is_agent: false,
        };
        let json = serde_json::to_string(&info).expect("encode");
        let decoded: BridgeMemberInfo = serde_json::from_str(&json).expect("decode");
        assert_eq!(info, decoded);
    }

    #[test]
    fn bridge_history_entry_round_trip() {
        let entry = BridgeHistoryEntry {
            sender_id: "peer-alice".to_string(),
            sender_name: "Alice".to_string(),
            content: "hello".to_string(),
            timestamp: "2025-01-15T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&entry).expect("encode");
        let decoded: BridgeHistoryEntry = serde_json::from_str(&json).expect("decode");
        assert_eq!(entry, decoded);
    }

    // --- Decode edge cases ---

    #[test]
    fn decode_line_trims_trailing_newline() {
        let line = "{\"type\":\"pong\"}\n";
        let decoded: AgentMessage = decode_line(line).expect("decode");
        assert_eq!(decoded, AgentMessage::Pong);
    }

    #[test]
    fn decode_line_trims_crlf() {
        let line = "{\"type\":\"ping\"}\r\n";
        let decoded: BridgeMessage = decode_line(line).expect("decode");
        assert_eq!(decoded, BridgeMessage::Ping);
    }

    #[test]
    fn decode_invalid_json_returns_error() {
        let result: Result<AgentMessage, _> = decode_line("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn decode_unknown_type_returns_error() {
        let result: Result<AgentMessage, _> = decode_line("{\"type\":\"unknown_variant\"}");
        assert!(result.is_err());
    }

    #[test]
    fn decode_missing_type_field_returns_error() {
        let result: Result<AgentMessage, _> = decode_line("{\"content\":\"hi\"}");
        assert!(result.is_err());
    }

    // --- Unicode / special content ---

    #[test]
    fn agent_send_message_with_unicode() {
        let msg = AgentMessage::SendMessage {
            content: "Hello \u{1F600} \u{65E5}\u{672C}\u{8A9E}".to_string(),
        };
        let line = encode_line(&msg).expect("encode");
        let decoded: AgentMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn agent_send_message_with_newlines_in_content() {
        let msg = AgentMessage::SendMessage {
            content: "line one\nline two\nline three".to_string(),
        };
        // JSON encoding escapes the newlines, so the outer line delimiter is unambiguous
        let line = encode_line(&msg).expect("encode");
        assert_eq!(line.trim().matches('\n').count(), 0);
        let decoded: AgentMessage = decode_line(&line).expect("decode");
        assert_eq!(msg, decoded);
    }

    // --- Agent ID validation tests ---

    #[test]
    fn validate_agent_id_normal() {
        let result = validate_agent_id("claude-42");
        assert_eq!(result.expect("valid"), "claude-42");
    }

    #[test]
    fn validate_agent_id_strips_control_chars() {
        let result = validate_agent_id("claude\x00bot\x07");
        assert_eq!(result.expect("valid"), "claudebot");
    }

    #[test]
    fn validate_agent_id_trims_whitespace() {
        let result = validate_agent_id("  my-agent  ");
        assert_eq!(result.expect("valid"), "my-agent");
    }

    #[test]
    fn validate_agent_id_empty_returns_error() {
        let result = validate_agent_id("");
        assert!(result.is_err());
    }

    #[test]
    fn validate_agent_id_only_control_chars_returns_error() {
        let result = validate_agent_id("\x00\x01\x02");
        assert!(result.is_err());
    }

    #[test]
    fn validate_agent_id_only_whitespace_returns_error() {
        let result = validate_agent_id("   ");
        assert!(result.is_err());
    }

    #[test]
    fn validate_agent_id_truncates_to_max_length() {
        let long_id = "a".repeat(100);
        let result = validate_agent_id(&long_id).expect("valid");
        assert_eq!(result.len(), MAX_AGENT_ID_LEN);
    }

    #[test]
    fn validate_agent_id_exactly_max_length() {
        let id = "b".repeat(MAX_AGENT_ID_LEN);
        let result = validate_agent_id(&id).expect("valid");
        assert_eq!(result.len(), MAX_AGENT_ID_LEN);
    }

    #[test]
    fn validate_agent_id_unicode() {
        let result = validate_agent_id("\u{30AF}\u{30ED}\u{30FC}\u{30C9}");
        assert_eq!(result.expect("valid"), "\u{30AF}\u{30ED}\u{30FC}\u{30C9}");
    }

    // --- make_unique_agent_peer_id tests ---

    #[test]
    fn unique_peer_id_no_conflict() {
        let existing: Vec<String> = vec![];
        let id = make_unique_agent_peer_id("claude-1", &existing);
        assert_eq!(id, "agent:claude-1");
    }

    #[test]
    fn unique_peer_id_with_one_conflict() {
        let existing = vec!["agent:claude-1".to_string()];
        let id = make_unique_agent_peer_id("claude-1", &existing);
        assert_eq!(id, "agent:claude-1-2");
    }

    #[test]
    fn unique_peer_id_with_multiple_conflicts() {
        let existing = vec![
            "agent:bot".to_string(),
            "agent:bot-2".to_string(),
            "agent:bot-3".to_string(),
        ];
        let id = make_unique_agent_peer_id("bot", &existing);
        assert_eq!(id, "agent:bot-4");
    }

    #[test]
    fn unique_peer_id_unrelated_existing_no_conflict() {
        let existing = vec!["peer-alice".to_string(), "peer-bob".to_string()];
        let id = make_unique_agent_peer_id("claude", &existing);
        assert_eq!(id, "agent:claude");
    }
}
