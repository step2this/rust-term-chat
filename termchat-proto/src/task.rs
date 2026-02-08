//! Task synchronization protocol types for `TermChat`.
//!
//! Defines the CRDT-based task model using Last-Write-Wins (LWW) registers
//! per field, the sync protocol messages, and postcard encode/decode functions.
//! Task sync messages are carried as opaque bytes in [`Envelope::TaskSync`].

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum allowed task title length in characters.
pub const MAX_TASK_TITLE_LENGTH: usize = 256;

/// Unique identifier for a task, based on UUID v7 for time-ordering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(Uuid);

impl TaskId {
    /// Creates a new time-ordered task identifier (UUID v7).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates a `TaskId` from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID value.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A Last-Write-Wins register for CRDT-based conflict resolution.
///
/// The merge rule:
/// 1. Higher timestamp wins
/// 2. Equal timestamps: higher `author` (lexicographic) wins
/// 3. This guarantees commutativity, associativity, and idempotency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LwwRegister<T> {
    /// The current value of the register.
    pub value: T,
    /// Milliseconds since epoch when this value was written.
    pub timestamp: u64,
    /// `PeerId` of the peer that wrote this value.
    pub author: String,
}

impl<T> LwwRegister<T> {
    /// Creates a new LWW register with the given value, timestamp, and author.
    pub const fn new(value: T, timestamp: u64, author: String) -> Self {
        Self {
            value,
            timestamp,
            author,
        }
    }
}

/// Status of a task in the shared task list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is open and not started.
    Open,
    /// Task is actively being worked on.
    InProgress,
    /// Task has been completed.
    Completed,
    /// Task has been soft-deleted.
    Deleted,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Deleted => write!(f, "deleted"),
        }
    }
}

/// A shared task with CRDT fields for conflict-free synchronization.
///
/// Each mutable field (title, status, assignee) is wrapped in an
/// [`LwwRegister`] so that concurrent edits to different fields
/// both survive, and concurrent edits to the same field resolve
/// deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier (UUID v7, time-ordered).
    pub id: TaskId,
    /// Room this task belongs to.
    pub room_id: String,
    /// Task title (LWW — concurrent title edits resolved by timestamp).
    pub title: LwwRegister<String>,
    /// Task status (LWW — concurrent status changes resolved by timestamp).
    pub status: LwwRegister<TaskStatus>,
    /// Optional assignee `PeerId` (LWW — concurrent assignment changes resolved by timestamp).
    pub assignee: LwwRegister<Option<String>>,
    /// When this task was originally created (milliseconds since epoch).
    pub created_at: u64,
    /// `PeerId` of the peer who created this task.
    pub created_by: String,
}

/// An update to a single field of a task.
///
/// Used in incremental sync messages so that only changed fields
/// are transmitted, reducing bandwidth and enabling fine-grained
/// LWW merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskFieldUpdate {
    /// Update the task title.
    Title(LwwRegister<String>),
    /// Update the task status.
    Status(LwwRegister<TaskStatus>),
    /// Update the task assignee.
    Assignee(LwwRegister<Option<String>>),
}

/// Sync protocol messages for task coordination between peers.
///
/// These messages are postcard-encoded and carried as opaque bytes
/// in [`Envelope::TaskSync`]. The three message types support both
/// incremental updates and full-state catch-up synchronization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskSyncMessage {
    /// Incremental update to a single task field.
    FieldUpdate {
        /// Which task is being updated.
        task_id: TaskId,
        /// Which room this task belongs to.
        room_id: String,
        /// The field update with LWW metadata.
        field: TaskFieldUpdate,
    },
    /// Full state snapshot for catch-up synchronization.
    ///
    /// Sent when a new peer joins a room, or for periodic reconciliation.
    /// Also used for task creation (single-task `FullState` with add-wins semantics).
    FullState {
        /// Which room these tasks belong to.
        room_id: String,
        /// All tasks in the room (receivers merge via LWW).
        tasks: Vec<Task>,
    },
    /// Request a full state snapshot from a room member.
    ///
    /// Sent by a newly-joined peer. Any member can respond (not just admin).
    RequestFullState {
        /// Which room to request state for.
        room_id: String,
    },
}

/// Encodes a [`TaskSyncMessage`] into bytes using postcard.
///
/// # Errors
///
/// Returns an error string if serialization fails.
pub fn encode(msg: &TaskSyncMessage) -> Result<Vec<u8>, String> {
    postcard::to_allocvec(msg).map_err(|e| format!("task encode error: {e}"))
}

/// Decodes a [`TaskSyncMessage`] from bytes using postcard.
///
/// # Errors
///
/// Returns an error string if deserialization fails.
pub fn decode(bytes: &[u8]) -> Result<TaskSyncMessage, String> {
    postcard::from_bytes(bytes).map_err(|e| format!("task decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_display_is_uuid() {
        let id = TaskId::new();
        let display = id.to_string();
        assert_eq!(display.len(), 36);
        assert!(display.contains('-'));
    }

    #[test]
    fn task_id_from_uuid_round_trip() {
        let uuid = Uuid::now_v7();
        let id = TaskId::from_uuid(uuid);
        assert_eq!(*id.as_uuid(), uuid);
    }

    #[test]
    fn lww_register_new() {
        let reg = LwwRegister::new("hello".to_string(), 1000, "peer-a".to_string());
        assert_eq!(reg.value, "hello");
        assert_eq!(reg.timestamp, 1000);
        assert_eq!(reg.author, "peer-a");
    }

    #[test]
    fn task_status_display() {
        assert_eq!(TaskStatus::Open.to_string(), "open");
        assert_eq!(TaskStatus::InProgress.to_string(), "in_progress");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
        assert_eq!(TaskStatus::Deleted.to_string(), "deleted");
    }

    fn make_test_task() -> Task {
        Task {
            id: TaskId::new(),
            room_id: "room-1".to_string(),
            title: LwwRegister::new("Fix the login bug".to_string(), 1000, "peer-a".to_string()),
            status: LwwRegister::new(TaskStatus::Open, 1000, "peer-a".to_string()),
            assignee: LwwRegister::new(None, 1000, "peer-a".to_string()),
            created_at: 1000,
            created_by: "peer-a".to_string(),
        }
    }

    #[test]
    fn round_trip_task() {
        let task = make_test_task();
        let bytes = postcard::to_allocvec(&task).expect("serialize");
        let decoded: Task = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(task, decoded);
    }

    #[test]
    fn round_trip_task_with_assignee() {
        let mut task = make_test_task();
        task.assignee = LwwRegister::new(Some("peer-b".to_string()), 2000, "peer-a".to_string());
        let bytes = postcard::to_allocvec(&task).expect("serialize");
        let decoded: Task = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(task, decoded);
    }

    #[test]
    fn round_trip_field_update_title() {
        let update = TaskFieldUpdate::Title(LwwRegister::new(
            "New title".to_string(),
            2000,
            "peer-b".to_string(),
        ));
        let bytes = postcard::to_allocvec(&update).expect("serialize");
        let decoded: TaskFieldUpdate = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(update, decoded);
    }

    #[test]
    fn round_trip_field_update_status() {
        let update = TaskFieldUpdate::Status(LwwRegister::new(
            TaskStatus::Completed,
            3000,
            "peer-c".to_string(),
        ));
        let bytes = postcard::to_allocvec(&update).expect("serialize");
        let decoded: TaskFieldUpdate = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(update, decoded);
    }

    #[test]
    fn round_trip_field_update_assignee() {
        let update = TaskFieldUpdate::Assignee(LwwRegister::new(
            Some("peer-d".to_string()),
            4000,
            "peer-e".to_string(),
        ));
        let bytes = postcard::to_allocvec(&update).expect("serialize");
        let decoded: TaskFieldUpdate = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(update, decoded);
    }

    #[test]
    fn round_trip_field_update_assignee_none() {
        let update = TaskFieldUpdate::Assignee(LwwRegister::new(None, 5000, "peer-f".to_string()));
        let bytes = postcard::to_allocvec(&update).expect("serialize");
        let decoded: TaskFieldUpdate = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(update, decoded);
    }

    #[test]
    fn round_trip_sync_field_update() {
        let msg = TaskSyncMessage::FieldUpdate {
            task_id: TaskId::new(),
            room_id: "room-1".to_string(),
            field: TaskFieldUpdate::Status(LwwRegister::new(
                TaskStatus::InProgress,
                2000,
                "peer-b".to_string(),
            )),
        };
        let bytes = encode(&msg).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_sync_full_state_empty() {
        let msg = TaskSyncMessage::FullState {
            room_id: "room-1".to_string(),
            tasks: vec![],
        };
        let bytes = encode(&msg).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_sync_full_state_with_tasks() {
        let msg = TaskSyncMessage::FullState {
            room_id: "room-1".to_string(),
            tasks: vec![make_test_task(), make_test_task()],
        };
        let bytes = encode(&msg).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_sync_request_full_state() {
        let msg = TaskSyncMessage::RequestFullState {
            room_id: "room-1".to_string(),
        };
        let bytes = encode(&msg).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(msg, decoded);
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
    fn round_trip_task_unicode_title() {
        let mut task = make_test_task();
        task.title = LwwRegister::new("バグ修正 🐛".to_string(), 1000, "peer-a".to_string());
        let bytes = postcard::to_allocvec(&task).expect("serialize");
        let decoded: Task = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(task, decoded);
    }

    #[test]
    fn round_trip_all_task_statuses() {
        for status in &[
            TaskStatus::Open,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Deleted,
        ] {
            let bytes = postcard::to_allocvec(status).expect("serialize");
            let decoded: TaskStatus = postcard::from_bytes(&bytes).expect("deserialize");
            assert_eq!(*status, decoded);
        }
    }

    #[test]
    fn lww_register_generic_with_option() {
        let reg: LwwRegister<Option<String>> =
            LwwRegister::new(Some("alice".to_string()), 1000, "peer-a".to_string());
        let bytes = postcard::to_allocvec(&reg).expect("serialize");
        let decoded: LwwRegister<Option<String>> =
            postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(reg, decoded);
    }
}
