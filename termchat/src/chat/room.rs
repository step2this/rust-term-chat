//! Room state management for `TermChat`.
//!
//! Contains the [`Room`] struct for room metadata and the [`RoomManager`]
//! for tracking local rooms, handling join request queues, and fan-out
//! message sending. This module is the client-side counterpart to the
//! relay's room registry.
//!
//! # Room Lifecycle
//!
//! 1. Creator calls `RoomManager::create_room()` with a name
//! 2. Manager validates the name, generates a `RoomId`, creates the Room
//! 3. Manager sends `RegisterRoom` to relay for discovery
//! 4. Other peers discover via `ListRooms` and send `JoinRequest`
//! 5. Admin approves/denies via `approve_join()`/`deny_join()`
//! 6. Approved members can send messages via `broadcast_to_room()`

use std::collections::HashMap;

use tokio::sync::mpsc;
use uuid::Uuid;

use termchat_proto::message::ConversationId;
use termchat_proto::room::{MemberInfo, RoomMessage};

/// Maximum number of rooms a single client can manage locally.
pub const MAX_ROOMS: usize = 64;

/// Maximum number of members allowed in a single room.
pub const MAX_MEMBERS: usize = 256;

/// Maximum length of a room name in characters.
pub const MAX_NAME_LEN: usize = 64;

/// Errors that can occur during room operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RoomError {
    /// Room name is empty or becomes empty after sanitization.
    #[error("room name cannot be empty")]
    NameEmpty,

    /// Room name exceeds the maximum allowed length.
    #[error("room name too long (max {MAX_NAME_LEN} characters)")]
    NameTooLong,

    /// Room name contains only invalid characters.
    #[error("room name contains only invalid characters")]
    NameInvalidChars,

    /// A room with the same name already exists locally.
    #[error("a room named '{0}' already exists")]
    DuplicateName(String),

    /// Maximum number of local rooms reached.
    #[error("room limit reached (max {MAX_ROOMS})")]
    RoomLimitReached,

    /// No room found with the given ID.
    #[error("room not found: {0}")]
    RoomNotFound(String),

    /// The operation requires admin privileges.
    #[error("not admin of room {0}")]
    NotAdmin(String),

    /// The room has reached its maximum member capacity.
    #[error("room is full (max {MAX_MEMBERS} members)")]
    RoomFull,

    /// The peer is already a member of the room.
    #[error("peer {0} is already a member")]
    AlreadyMember(String),
}

/// Events emitted by the [`RoomManager`] for UI or application layer consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomEvent {
    /// A new room was created locally.
    RoomCreated {
        /// The unique ID of the created room.
        room_id: String,
        /// The display name of the room.
        name: String,
    },
    /// A join request was received from a peer.
    JoinRequestReceived {
        /// The room being requested to join.
        room_id: String,
        /// The requesting peer's ID.
        peer_id: String,
        /// The requesting peer's display name.
        display_name: String,
    },
    /// A new member joined the room.
    MemberJoined {
        /// The room that gained a member.
        room_id: String,
        /// The new member's peer ID.
        peer_id: String,
        /// The new member's display name.
        display_name: String,
    },
    /// A peer's join request was denied.
    MemberDenied {
        /// The room that denied the request.
        room_id: String,
        /// The denied peer's ID.
        peer_id: String,
    },
}

/// Local representation of a chat room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Room {
    /// Unique room identifier (UUID v7).
    pub room_id: String,
    /// Human-readable room name.
    pub name: String,
    /// Current list of room members.
    pub members: Vec<MemberInfo>,
    /// Whether the local user is the admin of this room.
    pub is_admin: bool,
    /// Timestamp when the room was created (milliseconds since UNIX epoch).
    pub created_at: u64,
    /// Conversation identifier derived from the room ID.
    pub conversation_id: ConversationId,
}

/// Manages local room state, join request queues, and offline registration.
///
/// The `RoomManager` is a plain struct (not generic over Transport/Crypto)
/// that manages room state and emits [`RoomEvent`]s through an mpsc channel.
/// The application layer is responsible for wiring events to actual network
/// operations.
pub struct RoomManager {
    /// All locally tracked rooms, keyed by `room_id`.
    rooms: HashMap<String, Room>,
    /// Pending join requests per room: `room_id -> Vec<(peer_id, display_name)>`.
    pending_join_requests: HashMap<String, Vec<(String, String)>>,
    /// Room IDs queued for relay registration (when offline).
    pending_registrations: Vec<String>,
    /// Channel for emitting room events.
    event_sender: mpsc::Sender<RoomEvent>,
}

impl RoomManager {
    /// Creates a new `RoomManager` and its event receiver.
    ///
    /// The caller should consume events from the returned receiver to
    /// drive UI updates and network operations.
    #[must_use]
    pub fn new() -> (Self, mpsc::Receiver<RoomEvent>) {
        let (tx, rx) = mpsc::channel(256);
        let manager = Self {
            rooms: HashMap::new(),
            pending_join_requests: HashMap::new(),
            pending_registrations: Vec::new(),
            event_sender: tx,
        };
        (manager, rx)
    }

    /// Creates a new room with the given name and admin identity.
    ///
    /// Validates the name, checks for duplicate names and room limits,
    /// generates a UUID v7 room ID, and emits a [`RoomEvent::RoomCreated`].
    ///
    /// # Errors
    ///
    /// Returns [`RoomError`] if:
    /// - The name is empty, too long, or contains only invalid characters
    /// - A room with the same name already exists
    /// - The local room limit ([`MAX_ROOMS`]) has been reached
    pub fn create_room(
        &mut self,
        name: &str,
        admin_peer_id: &str,
        admin_display_name: &str,
    ) -> Result<Room, RoomError> {
        let sanitized = validate_room_name(name)?;

        // Check for duplicate local name
        if self.rooms.values().any(|r| r.name == sanitized) {
            return Err(RoomError::DuplicateName(sanitized));
        }

        // Check room limit
        if self.rooms.len() >= MAX_ROOMS {
            return Err(RoomError::RoomLimitReached);
        }

        let room_uuid = Uuid::now_v7();
        let room_id = room_uuid.to_string();
        let conversation_id = ConversationId::from_uuid(room_uuid);

        let now_millis = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        )
        .unwrap_or(u64::MAX);

        let admin_member = MemberInfo {
            peer_id: admin_peer_id.to_string(),
            display_name: admin_display_name.to_string(),
            is_admin: true,
        };

        let room = Room {
            room_id: room_id.clone(),
            name: sanitized.clone(),
            members: vec![admin_member],
            is_admin: true,
            created_at: now_millis,
            conversation_id,
        };

        self.rooms.insert(room_id.clone(), room.clone());

        // Emit event (best-effort; if receiver is dropped, silently ignore)
        let _ = self.event_sender.try_send(RoomEvent::RoomCreated {
            room_id,
            name: sanitized,
        });

        Ok(room)
    }

    /// Queues a room for relay registration when offline.
    ///
    /// The queued registrations can be drained later with
    /// [`drain_pending_registrations`](Self::drain_pending_registrations)
    /// when the relay connection is restored.
    pub fn queue_registration(&mut self, room_id: &str) {
        self.pending_registrations.push(room_id.to_string());
    }

    /// Drains all pending relay registrations, returning `RegisterRoom` messages.
    ///
    /// Returns an empty vec if there are no pending registrations. Rooms
    /// that no longer exist locally are silently skipped.
    pub fn drain_pending_registrations(&mut self) -> Vec<RoomMessage> {
        let ids: Vec<String> = self.pending_registrations.drain(..).collect();
        let mut messages = Vec::new();
        for room_id in ids {
            if let Some(room) = self.rooms.get(&room_id) {
                // Find the admin peer_id
                if let Some(admin) = room.members.iter().find(|m| m.is_admin) {
                    messages.push(RoomMessage::RegisterRoom {
                        room_id: room_id.clone(),
                        name: room.name.clone(),
                        admin_peer_id: admin.peer_id.clone(),
                    });
                }
            }
        }
        messages
    }

    /// Handles an incoming join request from a peer.
    ///
    /// The request is queued for admin approval. Emits a
    /// [`RoomEvent::JoinRequestReceived`].
    ///
    /// # Errors
    ///
    /// Returns [`RoomError::RoomNotFound`] if the room doesn't exist locally,
    /// or [`RoomError::NotAdmin`] if the local user is not the admin.
    pub fn handle_join_request(
        &mut self,
        room_id: &str,
        peer_id: &str,
        display_name: &str,
    ) -> Result<(), RoomError> {
        let room = self
            .rooms
            .get(room_id)
            .ok_or_else(|| RoomError::RoomNotFound(room_id.to_string()))?;

        if !room.is_admin {
            return Err(RoomError::NotAdmin(room_id.to_string()));
        }

        let queue = self
            .pending_join_requests
            .entry(room_id.to_string())
            .or_default();
        queue.push((peer_id.to_string(), display_name.to_string()));

        let _ = self.event_sender.try_send(RoomEvent::JoinRequestReceived {
            room_id: room_id.to_string(),
            peer_id: peer_id.to_string(),
            display_name: display_name.to_string(),
        });

        Ok(())
    }

    /// Approves a pending join request, adding the peer as a room member.
    ///
    /// Returns the new member's [`MemberInfo`] and the full updated member list.
    /// If the peer is already a member, returns their existing info and the
    /// current member list (idempotent).
    ///
    /// # Errors
    ///
    /// Returns [`RoomError`] if:
    /// - The room doesn't exist ([`RoomError::RoomNotFound`])
    /// - The local user is not admin ([`RoomError::NotAdmin`])
    /// - The room is full ([`RoomError::RoomFull`])
    pub fn approve_join(
        &mut self,
        room_id: &str,
        peer_id: &str,
    ) -> Result<(MemberInfo, Vec<MemberInfo>), RoomError> {
        let room = self
            .rooms
            .get(room_id)
            .ok_or_else(|| RoomError::RoomNotFound(room_id.to_string()))?;

        if !room.is_admin {
            return Err(RoomError::NotAdmin(room_id.to_string()));
        }

        // Check if already a member (idempotent)
        if let Some(existing) = room.members.iter().find(|m| m.peer_id == peer_id) {
            return Ok((existing.clone(), room.members.clone()));
        }

        // Check member limit
        if room.members.len() >= MAX_MEMBERS {
            return Err(RoomError::RoomFull);
        }

        // Find the display name from pending requests
        let display_name = self
            .pending_join_requests
            .get(room_id)
            .and_then(|queue| {
                queue
                    .iter()
                    .find(|(pid, _)| pid == peer_id)
                    .map(|(_, name)| name.clone())
            })
            .unwrap_or_else(|| peer_id.to_string());

        // Remove from pending queue
        if let Some(queue) = self.pending_join_requests.get_mut(room_id) {
            queue.retain(|(pid, _)| pid != peer_id);
        }

        let new_member = MemberInfo {
            peer_id: peer_id.to_string(),
            display_name: display_name.clone(),
            is_admin: false,
        };

        // Must re-borrow mutably after the immutable borrow above
        let room = self
            .rooms
            .get_mut(room_id)
            .ok_or_else(|| RoomError::RoomNotFound(room_id.to_string()))?;
        room.members.push(new_member.clone());
        let members = room.members.clone();

        let _ = self.event_sender.try_send(RoomEvent::MemberJoined {
            room_id: room_id.to_string(),
            peer_id: peer_id.to_string(),
            display_name,
        });

        Ok((new_member, members))
    }

    /// Denies a pending join request, removing it from the queue.
    ///
    /// Returns the denied peer's display name.
    ///
    /// # Errors
    ///
    /// Returns [`RoomError`] if:
    /// - The room doesn't exist ([`RoomError::RoomNotFound`])
    /// - The local user is not admin ([`RoomError::NotAdmin`])
    pub fn deny_join(&mut self, room_id: &str, peer_id: &str) -> Result<String, RoomError> {
        let room = self
            .rooms
            .get(room_id)
            .ok_or_else(|| RoomError::RoomNotFound(room_id.to_string()))?;

        if !room.is_admin {
            return Err(RoomError::NotAdmin(room_id.to_string()));
        }

        let display_name = self
            .pending_join_requests
            .get_mut(room_id)
            .and_then(|queue| {
                queue.iter().position(|(pid, _)| pid == peer_id).map(|pos| {
                    let (_, name) = queue.remove(pos);
                    name
                })
            })
            .unwrap_or_else(|| peer_id.to_string());

        let _ = self.event_sender.try_send(RoomEvent::MemberDenied {
            room_id: room_id.to_string(),
            peer_id: peer_id.to_string(),
        });

        Ok(display_name)
    }

    /// Returns the list of pending join requests for a room.
    ///
    /// Each entry is a `(peer_id, display_name)` tuple. Returns an empty
    /// vec if the room has no pending requests or doesn't exist.
    #[must_use]
    pub fn pending_requests(&self, room_id: &str) -> Vec<(String, String)> {
        self.pending_join_requests
            .get(room_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Returns the member list for a room.
    ///
    /// # Errors
    ///
    /// Returns [`RoomError::RoomNotFound`] if the room doesn't exist.
    pub fn get_room_members(&self, room_id: &str) -> Result<Vec<MemberInfo>, RoomError> {
        let room = self
            .rooms
            .get(room_id)
            .ok_or_else(|| RoomError::RoomNotFound(room_id.to_string()))?;
        Ok(room.members.clone())
    }

    /// Returns a reference to a room by its ID.
    ///
    /// # Errors
    ///
    /// Returns [`RoomError::RoomNotFound`] if the room doesn't exist.
    pub fn get_room(&self, room_id: &str) -> Result<&Room, RoomError> {
        self.rooms
            .get(room_id)
            .ok_or_else(|| RoomError::RoomNotFound(room_id.to_string()))
    }

    /// Finds a room by its display name.
    ///
    /// Returns `None` if no room with the given name exists.
    #[must_use]
    pub fn get_room_by_name(&self, name: &str) -> Option<&Room> {
        self.rooms.values().find(|r| r.name == name)
    }

    /// Returns a list of all locally tracked rooms.
    #[must_use]
    pub fn list_rooms(&self) -> Vec<&Room> {
        self.rooms.values().collect()
    }
}

/// Validates and sanitizes a room name.
///
/// Rules:
/// - Empty names return [`RoomError::NameEmpty`]
/// - Names longer than [`MAX_NAME_LEN`] characters return [`RoomError::NameTooLong`]
/// - Control characters are stripped; if the result is empty, returns
///   [`RoomError::NameInvalidChars`]
/// - Leading and trailing whitespace is trimmed
///
/// Returns the sanitized name on success.
///
/// # Errors
///
/// Returns [`RoomError`] if the name is empty, too long, or contains only
/// control characters.
pub fn validate_room_name(name: &str) -> Result<String, RoomError> {
    if name.is_empty() {
        return Err(RoomError::NameEmpty);
    }

    if name.chars().count() > MAX_NAME_LEN {
        return Err(RoomError::NameTooLong);
    }

    // Strip control characters
    let sanitized: String = name.chars().filter(|c| !c.is_control()).collect();
    let sanitized = sanitized.trim().to_string();

    if sanitized.is_empty() {
        return Err(RoomError::NameInvalidChars);
    }

    Ok(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Name validation tests ---

    #[test]
    fn validate_name_normal() {
        let result = validate_room_name("General Chat");
        assert_eq!(result.unwrap(), "General Chat");
    }

    #[test]
    fn validate_name_empty() {
        let result = validate_room_name("");
        assert_eq!(result, Err(RoomError::NameEmpty));
    }

    #[test]
    fn validate_name_too_long() {
        let long_name = "a".repeat(MAX_NAME_LEN + 1);
        let result = validate_room_name(&long_name);
        assert_eq!(result, Err(RoomError::NameTooLong));
    }

    #[test]
    fn validate_name_exactly_max_length() {
        let name = "a".repeat(MAX_NAME_LEN);
        let result = validate_room_name(&name);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), MAX_NAME_LEN);
    }

    #[test]
    fn validate_name_control_chars_stripped() {
        let result = validate_room_name("Hello\x00World\x07");
        assert_eq!(result.unwrap(), "HelloWorld");
    }

    #[test]
    fn validate_name_only_control_chars() {
        let result = validate_room_name("\x00\x01\x02\x03");
        assert_eq!(result, Err(RoomError::NameInvalidChars));
    }

    #[test]
    fn validate_name_trims_whitespace() {
        let result = validate_room_name("  General  ");
        assert_eq!(result.unwrap(), "General");
    }

    #[test]
    fn validate_name_unicode() {
        let result = validate_room_name("日本語チャット");
        assert_eq!(result.unwrap(), "日本語チャット");
    }

    #[test]
    fn validate_name_mixed_control_and_whitespace() {
        let result = validate_room_name(" \t\n ");
        // \t and \n are control characters, space is not
        // After stripping control chars: "  ", after trim: ""
        assert_eq!(result, Err(RoomError::NameInvalidChars));
    }

    // --- Room creation tests ---

    #[test]
    fn create_room_success() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

        assert_eq!(room.name, "General");
        assert!(room.is_admin);
        assert_eq!(room.members.len(), 1);
        assert_eq!(room.members[0].peer_id, "peer-alice");
        assert_eq!(room.members[0].display_name, "Alice");
        assert!(room.members[0].is_admin);
        assert!(!room.room_id.is_empty());
    }

    #[test]
    fn create_room_emits_event() {
        let (mut mgr, mut rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(
            event,
            RoomEvent::RoomCreated {
                room_id: room.room_id.clone(),
                name: "General".to_string(),
            }
        );
    }

    #[test]
    fn create_room_duplicate_name_fails() {
        let (mut mgr, _rx) = RoomManager::new();
        mgr.create_room("General", "peer-alice", "Alice").unwrap();

        let result = mgr.create_room("General", "peer-bob", "Bob");
        assert_eq!(result, Err(RoomError::DuplicateName("General".to_string())));
    }

    #[test]
    fn create_room_limit_reached() {
        let (mut mgr, _rx) = RoomManager::new();
        for i in 0..MAX_ROOMS {
            mgr.create_room(&format!("Room {i}"), "peer-alice", "Alice")
                .unwrap();
        }

        let result = mgr.create_room("One More", "peer-alice", "Alice");
        assert_eq!(result, Err(RoomError::RoomLimitReached));
    }

    #[test]
    fn create_room_sanitizes_name() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr
            .create_room("  Test\x00Room  ", "peer-alice", "Alice")
            .unwrap();
        assert_eq!(room.name, "TestRoom");
    }

    #[test]
    fn create_room_conversation_id_derives_from_room_id() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("Test", "peer-alice", "Alice").unwrap();

        let expected_conv_id = ConversationId::from_uuid(Uuid::parse_str(&room.room_id).unwrap());
        assert_eq!(room.conversation_id.as_uuid(), expected_conv_id.as_uuid());
    }

    // --- Join request handling tests ---

    #[test]
    fn handle_join_request_success() {
        let (mut mgr, mut rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();
        let _ = rx.try_recv(); // drain RoomCreated

        mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
            .unwrap();

        let pending = mgr.pending_requests(&room.room_id);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0], ("peer-bob".to_string(), "Bob".to_string()));

        let event = rx.try_recv().unwrap();
        assert_eq!(
            event,
            RoomEvent::JoinRequestReceived {
                room_id: room.room_id.clone(),
                peer_id: "peer-bob".to_string(),
                display_name: "Bob".to_string(),
            }
        );
    }

    #[test]
    fn handle_join_request_room_not_found() {
        let (mut mgr, _rx) = RoomManager::new();
        let result = mgr.handle_join_request("nonexistent", "peer-bob", "Bob");
        assert_eq!(
            result,
            Err(RoomError::RoomNotFound("nonexistent".to_string()))
        );
    }

    // --- Approve join tests ---

    #[test]
    fn approve_join_success() {
        let (mut mgr, mut rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();
        let _ = rx.try_recv(); // drain RoomCreated

        mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
            .unwrap();
        let _ = rx.try_recv(); // drain JoinRequestReceived

        let (new_member, members) = mgr.approve_join(&room.room_id, "peer-bob").unwrap();
        assert_eq!(new_member.peer_id, "peer-bob");
        assert_eq!(new_member.display_name, "Bob");
        assert!(!new_member.is_admin);
        assert_eq!(members.len(), 2);

        let event = rx.try_recv().unwrap();
        assert_eq!(
            event,
            RoomEvent::MemberJoined {
                room_id: room.room_id.clone(),
                peer_id: "peer-bob".to_string(),
                display_name: "Bob".to_string(),
            }
        );
    }

    #[test]
    fn approve_join_already_member_idempotent() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

        mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
            .unwrap();
        mgr.approve_join(&room.room_id, "peer-bob").unwrap();

        // Second approve should be idempotent
        let (member, members) = mgr.approve_join(&room.room_id, "peer-bob").unwrap();
        assert_eq!(member.peer_id, "peer-bob");
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn approve_join_room_full() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("Full Room", "peer-admin", "Admin").unwrap();

        // Fill the room to capacity (admin is already member 1)
        for i in 1..MAX_MEMBERS {
            mgr.handle_join_request(&room.room_id, &format!("peer-{i}"), &format!("User {i}"))
                .unwrap();
            mgr.approve_join(&room.room_id, &format!("peer-{i}"))
                .unwrap();
        }

        // One more should fail
        mgr.handle_join_request(&room.room_id, "peer-overflow", "Overflow")
            .unwrap();
        let result = mgr.approve_join(&room.room_id, "peer-overflow");
        assert_eq!(result, Err(RoomError::RoomFull));
    }

    #[test]
    fn approve_join_not_admin() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

        // Manually mark as non-admin to simulate a non-admin room
        mgr.rooms.get_mut(&room.room_id).unwrap().is_admin = false;

        mgr.pending_join_requests
            .entry(room.room_id.clone())
            .or_default()
            .push(("peer-bob".to_string(), "Bob".to_string()));

        let result = mgr.approve_join(&room.room_id, "peer-bob");
        assert_eq!(result, Err(RoomError::NotAdmin(room.room_id.clone())));
    }

    #[test]
    fn approve_join_room_not_found() {
        let (mut mgr, _rx) = RoomManager::new();
        let result = mgr.approve_join("nonexistent", "peer-bob");
        assert_eq!(
            result,
            Err(RoomError::RoomNotFound("nonexistent".to_string()))
        );
    }

    // --- Deny join tests ---

    #[test]
    fn deny_join_success() {
        let (mut mgr, mut rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();
        let _ = rx.try_recv(); // drain RoomCreated

        mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
            .unwrap();
        let _ = rx.try_recv(); // drain JoinRequestReceived

        let display_name = mgr.deny_join(&room.room_id, "peer-bob").unwrap();
        assert_eq!(display_name, "Bob");

        let pending = mgr.pending_requests(&room.room_id);
        assert!(pending.is_empty());

        let event = rx.try_recv().unwrap();
        assert_eq!(
            event,
            RoomEvent::MemberDenied {
                room_id: room.room_id.clone(),
                peer_id: "peer-bob".to_string(),
            }
        );
    }

    #[test]
    fn deny_join_not_admin() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();
        mgr.rooms.get_mut(&room.room_id).unwrap().is_admin = false;

        let result = mgr.deny_join(&room.room_id, "peer-bob");
        assert_eq!(result, Err(RoomError::NotAdmin(room.room_id.clone())));
    }

    #[test]
    fn deny_join_room_not_found() {
        let (mut mgr, _rx) = RoomManager::new();
        let result = mgr.deny_join("nonexistent", "peer-bob");
        assert_eq!(
            result,
            Err(RoomError::RoomNotFound("nonexistent".to_string()))
        );
    }

    // --- Query method tests ---

    #[test]
    fn get_room_members_success() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

        let members = mgr.get_room_members(&room.room_id).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].peer_id, "peer-alice");
    }

    #[test]
    fn get_room_success() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

        let fetched = mgr.get_room(&room.room_id).unwrap();
        assert_eq!(fetched.name, "General");
    }

    #[test]
    fn get_room_not_found() {
        let (mgr, _rx) = RoomManager::new();
        let result = mgr.get_room("nonexistent");
        assert_eq!(
            result,
            Err(RoomError::RoomNotFound("nonexistent".to_string()))
        );
    }

    #[test]
    fn get_room_by_name_found() {
        let (mut mgr, _rx) = RoomManager::new();
        mgr.create_room("General", "peer-alice", "Alice").unwrap();

        let room = mgr.get_room_by_name("General");
        assert!(room.is_some());
        assert_eq!(room.unwrap().name, "General");
    }

    #[test]
    fn get_room_by_name_not_found() {
        let (mgr, _rx) = RoomManager::new();
        assert!(mgr.get_room_by_name("Nonexistent").is_none());
    }

    #[test]
    fn list_rooms_empty() {
        let (mgr, _rx) = RoomManager::new();
        assert!(mgr.list_rooms().is_empty());
    }

    #[test]
    fn list_rooms_multiple() {
        let (mut mgr, _rx) = RoomManager::new();
        mgr.create_room("Room A", "peer-alice", "Alice").unwrap();
        mgr.create_room("Room B", "peer-alice", "Alice").unwrap();

        let rooms = mgr.list_rooms();
        assert_eq!(rooms.len(), 2);
    }

    // --- Offline registration tests ---

    #[test]
    fn queue_and_drain_registrations() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

        mgr.queue_registration(&room.room_id);

        let messages = mgr.drain_pending_registrations();
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            RoomMessage::RegisterRoom {
                room_id,
                name,
                admin_peer_id,
            } => {
                assert_eq!(room_id, &room.room_id);
                assert_eq!(name, "General");
                assert_eq!(admin_peer_id, "peer-alice");
            }
            _ => panic!("expected RegisterRoom message"),
        }

        // Second drain should be empty
        let messages = mgr.drain_pending_registrations();
        assert!(messages.is_empty());
    }

    #[test]
    fn drain_registrations_skips_deleted_rooms() {
        let (mut mgr, _rx) = RoomManager::new();
        mgr.queue_registration("nonexistent-room");

        let messages = mgr.drain_pending_registrations();
        assert!(messages.is_empty());
    }

    // --- Pending requests for nonexistent room ---

    #[test]
    fn pending_requests_nonexistent_room() {
        let (mgr, _rx) = RoomManager::new();
        assert!(mgr.pending_requests("nonexistent").is_empty());
    }

    // --- Handle join request not admin ---

    #[test]
    fn handle_join_request_not_admin() {
        let (mut mgr, _rx) = RoomManager::new();
        let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();
        mgr.rooms.get_mut(&room.room_id).unwrap().is_admin = false;

        let result = mgr.handle_join_request(&room.room_id, "peer-bob", "Bob");
        assert_eq!(result, Err(RoomError::NotAdmin(room.room_id.clone())));
    }
}
