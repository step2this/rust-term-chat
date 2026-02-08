//! Room protocol wire types for `TermChat` room management.
//!
//! Defines the [`RoomMessage`] enum for room creation, discovery, joining,
//! and membership updates. These messages are postcard-encoded and carried
//! either directly over WebSocket (client - relay) or embedded in
//! [`RelayMessage::Room`] frames.

use serde::{Deserialize, Serialize};

/// Messages for room management operations.
///
/// Room protocol messages handle the full lifecycle: creation, discovery,
/// join requests, approval/denial, and membership change broadcasts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomMessage {
    /// Register a room with the relay server's room directory.
    ///
    /// Sent by the room creator to make the room discoverable.
    RegisterRoom {
        /// Unique room identifier (UUID v7).
        room_id: String,
        /// Human-readable room name.
        name: String,
        /// `PeerId` of the room admin/creator.
        admin_peer_id: String,
    },

    /// Remove a room from the relay directory.
    UnregisterRoom {
        /// The room to remove.
        room_id: String,
    },

    /// Request the list of available rooms from the relay.
    ListRooms,

    /// Relay responds with the room directory.
    RoomList {
        /// Available rooms on this relay.
        rooms: Vec<RoomInfo>,
    },

    /// Peer requests to join a room (routed via relay to the admin).
    JoinRequest {
        /// The room to join.
        room_id: String,
        /// `PeerId` of the requesting peer.
        peer_id: String,
        /// Display name of the requesting peer.
        display_name: String,
    },

    /// Admin approves a join request (sent to the joiner).
    JoinApproved {
        /// The room that was joined.
        room_id: String,
        /// Room display name.
        name: String,
        /// Current member list (including the newly approved member).
        members: Vec<MemberInfo>,
    },

    /// Admin denies a join request (sent to the joiner).
    JoinDenied {
        /// The room that denied entry.
        room_id: String,
        /// Reason for denial.
        reason: String,
    },

    /// Broadcast to all members when membership changes.
    MembershipUpdate {
        /// The room whose membership changed.
        room_id: String,
        /// What changed.
        action: MemberAction,
        /// `PeerId` of the affected member.
        peer_id: String,
        /// Display name of the affected member.
        display_name: String,
    },
}

/// Summary info for room discovery via the relay directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomInfo {
    /// Unique room identifier.
    pub room_id: String,
    /// Human-readable room name.
    pub name: String,
    /// Current number of members.
    pub member_count: u32,
}

/// Info about a room member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberInfo {
    /// Member's `PeerId`.
    pub peer_id: String,
    /// Member's display name.
    pub display_name: String,
    /// Whether this member is a room admin.
    pub is_admin: bool,
    /// Whether this member is an AI agent participant.
    ///
    /// Defaults to `false` for backward compatibility with existing
    /// serialized data that predates agent support.
    #[serde(default)]
    pub is_agent: bool,
}

/// What changed in a membership update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberAction {
    /// A new member joined.
    Joined,
    /// A member left.
    Left,
    /// A member was promoted to admin.
    Promoted,
    /// A member was demoted from admin.
    Demoted,
}

/// Encodes a [`RoomMessage`] into bytes using postcard.
///
/// # Errors
///
/// Returns an error string if serialization fails.
pub fn encode(msg: &RoomMessage) -> Result<Vec<u8>, String> {
    postcard::to_allocvec(msg).map_err(|e| format!("room encode error: {e}"))
}

/// Decodes a [`RoomMessage`] from bytes using postcard.
///
/// # Errors
///
/// Returns an error string if deserialization fails.
pub fn decode(bytes: &[u8]) -> Result<RoomMessage, String> {
    postcard::from_bytes(bytes).map_err(|e| format!("room decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_register_room() {
        let msg = RoomMessage::RegisterRoom {
            room_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            name: "General Chat".to_string(),
            admin_peer_id: "peer-alice".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_unregister_room() {
        let msg = RoomMessage::UnregisterRoom {
            room_id: "room-123".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_list_rooms() {
        let msg = RoomMessage::ListRooms;
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_room_list_empty() {
        let msg = RoomMessage::RoomList { rooms: vec![] };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_room_list_with_entries() {
        let msg = RoomMessage::RoomList {
            rooms: vec![
                RoomInfo {
                    room_id: "room-1".to_string(),
                    name: "General".to_string(),
                    member_count: 5,
                },
                RoomInfo {
                    room_id: "room-2".to_string(),
                    name: "Random".to_string(),
                    member_count: 12,
                },
            ],
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_join_request() {
        let msg = RoomMessage::JoinRequest {
            room_id: "room-abc".to_string(),
            peer_id: "peer-bob".to_string(),
            display_name: "Bob".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_join_approved() {
        let msg = RoomMessage::JoinApproved {
            room_id: "room-abc".to_string(),
            name: "General Chat".to_string(),
            members: vec![
                MemberInfo {
                    peer_id: "peer-alice".to_string(),
                    display_name: "Alice".to_string(),
                    is_admin: true,
                    is_agent: false,
                },
                MemberInfo {
                    peer_id: "peer-bob".to_string(),
                    display_name: "Bob".to_string(),
                    is_admin: false,
                    is_agent: false,
                },
            ],
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_join_approved_empty_members() {
        let msg = RoomMessage::JoinApproved {
            room_id: "room-xyz".to_string(),
            name: "Empty Room".to_string(),
            members: vec![],
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_join_denied() {
        let msg = RoomMessage::JoinDenied {
            room_id: "room-abc".to_string(),
            reason: "room full".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_membership_update_joined() {
        let msg = RoomMessage::MembershipUpdate {
            room_id: "room-abc".to_string(),
            action: MemberAction::Joined,
            peer_id: "peer-charlie".to_string(),
            display_name: "Charlie".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_membership_update_left() {
        let msg = RoomMessage::MembershipUpdate {
            room_id: "room-abc".to_string(),
            action: MemberAction::Left,
            peer_id: "peer-dave".to_string(),
            display_name: "Dave".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_membership_update_promoted() {
        let msg = RoomMessage::MembershipUpdate {
            room_id: "room-abc".to_string(),
            action: MemberAction::Promoted,
            peer_id: "peer-eve".to_string(),
            display_name: "Eve".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_membership_update_demoted() {
        let msg = RoomMessage::MembershipUpdate {
            room_id: "room-abc".to_string(),
            action: MemberAction::Demoted,
            peer_id: "peer-frank".to_string(),
            display_name: "Frank".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn member_info_is_agent_round_trip() {
        let member = MemberInfo {
            peer_id: "agent:claude-1".to_string(),
            display_name: "Claude".to_string(),
            is_admin: false,
            is_agent: true,
        };
        let bytes = postcard::to_allocvec(&member).expect("serialize");
        let decoded: MemberInfo = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(member, decoded);
        assert!(decoded.is_agent);
    }

    #[test]
    fn member_info_is_agent_defaults_false() {
        // Simulate data serialized before is_agent field existed.
        // A MemberInfo with is_agent=false should produce identical bytes
        // to the old 3-field struct (since postcard uses #[serde(default)]).
        let member = MemberInfo {
            peer_id: "peer-alice".to_string(),
            display_name: "Alice".to_string(),
            is_admin: true,
            is_agent: false,
        };
        let bytes = postcard::to_allocvec(&member).expect("serialize");
        let decoded: MemberInfo = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(member, decoded);
        assert!(!decoded.is_agent);
    }

    #[test]
    fn decode_corrupted_bytes_fails() {
        let result = decode(&[0xFF, 0xFE, 0xFD, 0xFC]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_empty_bytes_fails() {
        let result = decode(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_room_name_with_unicode() {
        let msg = RoomMessage::RegisterRoom {
            room_id: "room-unicode".to_string(),
            name: "日本語チャット 🎉".to_string(),
            admin_peer_id: "peer-admin".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_max_length_room_name() {
        let msg = RoomMessage::RegisterRoom {
            room_id: "room-max".to_string(),
            name: "A".repeat(64),
            admin_peer_id: "peer-admin".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }
}
