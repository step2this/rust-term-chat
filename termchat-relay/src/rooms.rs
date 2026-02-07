//! Room registry for the relay server.
//!
//! Maintains an in-memory directory of active rooms. Peers register rooms
//! after creation, and other peers can discover them via `ListRooms`.
//! The registry also routes `JoinRequest` messages to the room admin's `PeerId`.
//!
//! Room entries are ephemeral — lost on relay restart, same as the peer registry.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::Message;
use termchat_proto::relay::{self, RelayMessage};
use termchat_proto::room::{self, RoomInfo, RoomMessage};
use tokio::sync::RwLock;

use crate::relay::RelayState;

/// Maximum number of rooms the registry will hold.
const MAX_REGISTRY_ROOMS: usize = 1000;

/// An entry in the room registry tracking room metadata.
#[derive(Debug, Clone)]
pub struct RoomRegistryEntry {
    /// Unique room identifier.
    pub room_id: String,
    /// Human-readable room name.
    pub name: String,
    /// `PeerId` of the room admin/creator.
    pub admin_peer_id: String,
    /// Current number of members.
    pub member_count: u32,
}

/// Errors that can occur during room registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// A room with the same name (case-insensitive) already exists.
    #[error("a room with that name already exists")]
    NameConflict,
    /// The registry has reached its maximum capacity.
    #[error("room registry is full (max {MAX_REGISTRY_ROOMS} rooms)")]
    CapacityReached,
    /// The specified room was not found.
    #[error("room not found")]
    RoomNotFound,
    /// Failed to encode a protocol message.
    #[error("encoding failed: {0}")]
    EncodingFailed(String),
}

/// In-memory directory of registered rooms.
///
/// Thread-safe via [`RwLock`]. Supports register, unregister, list, and
/// admin lookup operations.
pub struct RoomRegistry {
    rooms: RwLock<HashMap<String, RoomRegistryEntry>>,
}

impl Default for RoomRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomRegistry {
    /// Creates a new, empty room registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a room in the directory.
    ///
    /// Returns an error if a room with the same name (case-insensitive)
    /// already exists, or if the registry has reached its capacity limit.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::NameConflict`] or [`RegistryError::CapacityReached`].
    pub async fn register(
        &self,
        room_id: &str,
        name: &str,
        admin_peer_id: &str,
    ) -> Result<(), RegistryError> {
        let mut rooms = self.rooms.write().await;

        if rooms.len() >= MAX_REGISTRY_ROOMS && !rooms.contains_key(room_id) {
            return Err(RegistryError::CapacityReached);
        }

        let name_lower = name.to_lowercase();
        for (id, entry) in rooms.iter() {
            if id != room_id && entry.name.to_lowercase() == name_lower {
                return Err(RegistryError::NameConflict);
            }
        }

        rooms.insert(
            room_id.to_string(),
            RoomRegistryEntry {
                room_id: room_id.to_string(),
                name: name.to_string(),
                admin_peer_id: admin_peer_id.to_string(),
                member_count: 1,
            },
        );
        drop(rooms);

        Ok(())
    }

    /// Removes a room from the directory.
    ///
    /// Returns `true` if the room existed and was removed, `false` otherwise.
    pub async fn unregister(&self, room_id: &str) -> bool {
        let mut rooms = self.rooms.write().await;
        rooms.remove(room_id).is_some()
    }

    /// Returns a list of all registered rooms as [`RoomInfo`] structs.
    pub async fn list(&self) -> Vec<RoomInfo> {
        let rooms = self.rooms.read().await;
        rooms
            .values()
            .map(|entry| RoomInfo {
                room_id: entry.room_id.clone(),
                name: entry.name.clone(),
                member_count: entry.member_count,
            })
            .collect()
    }

    /// Returns the admin `PeerId` for a room, if the room exists.
    pub async fn get_admin(&self, room_id: &str) -> Option<String> {
        let rooms = self.rooms.read().await;
        rooms.get(room_id).map(|e| e.admin_peer_id.clone())
    }

    /// Returns the full registry entry for a room, if it exists.
    pub async fn get_entry(&self, room_id: &str) -> Option<RoomRegistryEntry> {
        let rooms = self.rooms.read().await;
        rooms.get(room_id).cloned()
    }
}

/// Routes a join request to the room admin.
///
/// If the admin is online, forwards the `JoinRequest` directly. If the admin
/// is offline, queues the message for later delivery via store-and-forward.
///
/// # Errors
///
/// Returns [`RegistryError::RoomNotFound`] if the room does not exist, or
/// [`RegistryError::EncodingFailed`] if the message cannot be serialized.
pub async fn route_join_request(
    registry: &RoomRegistry,
    state: &Arc<RelayState>,
    room_id: &str,
    peer_id: &str,
    display_name: &str,
) -> Result<(), RegistryError> {
    let admin_peer_id = registry
        .get_admin(room_id)
        .await
        .ok_or(RegistryError::RoomNotFound)?;

    let join_msg = RoomMessage::JoinRequest {
        room_id: room_id.to_string(),
        peer_id: peer_id.to_string(),
        display_name: display_name.to_string(),
    };

    let room_bytes = room::encode(&join_msg).map_err(RegistryError::EncodingFailed)?;
    let relay_msg = RelayMessage::Room(room_bytes);

    if let Some(sender) = state.get_sender(&admin_peer_id).await {
        if let Ok(bytes) = relay::encode(&relay_msg) {
            let _ = sender.send(Message::Binary(bytes.into()));
        }
    } else {
        // Admin is offline — queue the room message bytes as payload
        // so the admin receives them on reconnect.
        let relay_bytes = relay::encode(&relay_msg).map_err(RegistryError::EncodingFailed)?;
        state
            .store
            .enqueue(&admin_peer_id, peer_id, relay_bytes)
            .await;
    }

    Ok(())
}

/// Routes a `JoinApproved` or `JoinDenied` message back to the target peer.
pub async fn route_room_message(state: &Arc<RelayState>, target_peer_id: &str, msg: &RoomMessage) {
    let room_bytes = match room::encode(msg) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "failed to encode room message for routing");
            return;
        }
    };
    let relay_msg = RelayMessage::Room(room_bytes);
    if let Some(sender) = state.get_sender(target_peer_id).await
        && let Ok(bytes) = relay::encode(&relay_msg)
    {
        let _ = sender.send(Message::Binary(bytes.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_list() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        let rooms = registry.list().await;
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "room-1");
        assert_eq!(rooms[0].name, "General");
        assert_eq!(rooms[0].member_count, 1);
    }

    #[tokio::test]
    async fn unregister_existing_room() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        assert!(registry.unregister("room-1").await);
        assert!(registry.list().await.is_empty());
    }

    #[tokio::test]
    async fn unregister_nonexistent_returns_false() {
        let registry = RoomRegistry::new();
        assert!(!registry.unregister("nonexistent").await);
    }

    #[tokio::test]
    async fn get_admin_existing() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        assert_eq!(
            registry.get_admin("room-1").await,
            Some("alice".to_string())
        );
    }

    #[tokio::test]
    async fn get_admin_nonexistent() {
        let registry = RoomRegistry::new();
        assert_eq!(registry.get_admin("nonexistent").await, None);
    }

    #[tokio::test]
    async fn get_entry_existing() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        let entry = registry.get_entry("room-1").await.unwrap();
        assert_eq!(entry.room_id, "room-1");
        assert_eq!(entry.name, "General");
        assert_eq!(entry.admin_peer_id, "alice");
        assert_eq!(entry.member_count, 1);
    }

    #[tokio::test]
    async fn get_entry_nonexistent() {
        let registry = RoomRegistry::new();
        assert!(registry.get_entry("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn name_conflict_case_insensitive() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        let result = registry.register("room-2", "general", "bob").await;
        assert!(matches!(result, Err(RegistryError::NameConflict)));
    }

    #[tokio::test]
    async fn name_conflict_mixed_case() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "My Room", "alice")
            .await
            .unwrap();

        let result = registry.register("room-2", "MY ROOM", "bob").await;
        assert!(matches!(result, Err(RegistryError::NameConflict)));
    }

    #[tokio::test]
    async fn same_room_id_re_register_allowed() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        // Re-registering the same room_id should succeed (overwrite).
        let result = registry.register("room-1", "General v2", "alice").await;
        assert!(result.is_ok());

        let entry = registry.get_entry("room-1").await.unwrap();
        assert_eq!(entry.name, "General v2");
    }

    #[tokio::test]
    async fn capacity_limit_enforced() {
        let registry = RoomRegistry::new();

        // Fill to capacity.
        for i in 0..MAX_REGISTRY_ROOMS {
            registry
                .register(&format!("room-{i}"), &format!("Room {i}"), "admin")
                .await
                .unwrap();
        }

        // One more should fail.
        let result = registry
            .register("room-overflow", "Overflow", "admin")
            .await;
        assert!(matches!(result, Err(RegistryError::CapacityReached)));
    }

    #[tokio::test]
    async fn capacity_allows_after_unregister() {
        let registry = RoomRegistry::new();

        for i in 0..MAX_REGISTRY_ROOMS {
            registry
                .register(&format!("room-{i}"), &format!("Room {i}"), "admin")
                .await
                .unwrap();
        }

        // Remove one room.
        registry.unregister("room-0").await;

        // Now we should be able to add one more.
        let result = registry.register("room-new", "New Room", "admin").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn multiple_rooms_listed() {
        let registry = RoomRegistry::new();
        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();
        registry.register("room-2", "Random", "bob").await.unwrap();
        registry.register("room-3", "Dev", "carol").await.unwrap();

        let rooms = registry.list().await;
        assert_eq!(rooms.len(), 3);

        let names: Vec<&str> = rooms.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"General"));
        assert!(names.contains(&"Random"));
        assert!(names.contains(&"Dev"));
    }

    #[tokio::test]
    async fn route_join_request_room_not_found() {
        let registry = RoomRegistry::new();
        let state = Arc::new(RelayState::new());

        let result = route_join_request(&registry, &state, "nonexistent", "bob", "Bob").await;
        assert!(matches!(result, Err(RegistryError::RoomNotFound)));
    }

    #[tokio::test]
    async fn route_join_request_admin_online() {
        let registry = RoomRegistry::new();
        let state = Arc::new(RelayState::new());

        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        // Register alice as connected.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state.register("alice", tx).await;

        route_join_request(&registry, &state, "room-1", "bob", "Bob")
            .await
            .unwrap();

        // Alice should have received the forwarded JoinRequest.
        let received = rx.recv().await.unwrap();
        let data = match received {
            Message::Binary(b) => b,
            other => panic!("expected Binary, got {other:?}"),
        };
        let relay_msg = relay::decode(&data).unwrap();
        match relay_msg {
            RelayMessage::Room(room_bytes) => {
                let room_msg = room::decode(&room_bytes).unwrap();
                assert_eq!(
                    room_msg,
                    RoomMessage::JoinRequest {
                        room_id: "room-1".to_string(),
                        peer_id: "bob".to_string(),
                        display_name: "Bob".to_string(),
                    }
                );
            }
            other => panic!("expected Room, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_join_request_admin_offline_queues() {
        let registry = RoomRegistry::new();
        let state = Arc::new(RelayState::new());

        registry
            .register("room-1", "General", "alice")
            .await
            .unwrap();

        // Don't register alice as connected — she's offline.
        route_join_request(&registry, &state, "room-1", "bob", "Bob")
            .await
            .unwrap();

        // The message should be queued for alice.
        assert_eq!(state.store.queue_len("alice").await, 1);
    }
}
