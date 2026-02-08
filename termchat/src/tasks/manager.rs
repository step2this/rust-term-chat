//! Task manager for room-scoped task CRUD and sync message generation.
//!
//! `TaskManager` provides the application-layer interface for creating,
//! updating, deleting, and synchronizing tasks within rooms.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use termchat_proto::task::{
    LwwRegister, MAX_TASK_TITLE_LENGTH, Task, TaskFieldUpdate, TaskId, TaskStatus, TaskSyncMessage,
};

use super::TaskError;
use super::merge::{apply_field_update, merge_task_list};

/// Manages room-scoped task lists with CRDT-based synchronization.
///
/// Each room has its own independent task map. All mutations generate
/// [`TaskSyncMessage`] values that should be broadcast to room members.
pub struct TaskManager {
    /// Room ID -> (Task ID -> Task) mapping.
    tasks: HashMap<String, HashMap<TaskId, Task>>,
    /// The local peer's identifier, used as author in LWW registers.
    local_peer_id: String,
}

impl TaskManager {
    /// Creates a new `TaskManager` for the given local peer.
    #[must_use]
    pub fn new(local_peer_id: String) -> Self {
        Self {
            tasks: HashMap::new(),
            local_peer_id,
        }
    }

    /// Returns the current timestamp in milliseconds since epoch.
    fn now_ms() -> u64 {
        u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        )
        .unwrap_or(u64::MAX)
    }

    /// Creates a new task in the given room.
    ///
    /// # Errors
    ///
    /// Returns [`TaskError::TitleEmpty`] if the title is empty, or
    /// [`TaskError::TitleTooLong`] if it exceeds 256 characters.
    pub fn create_task(
        &mut self,
        room_id: &str,
        title: &str,
    ) -> Result<(Task, TaskSyncMessage), TaskError> {
        if title.is_empty() {
            return Err(TaskError::TitleEmpty);
        }
        if title.chars().count() > MAX_TASK_TITLE_LENGTH {
            return Err(TaskError::TitleTooLong);
        }

        let now = Self::now_ms();
        let task = Task {
            id: TaskId::new(),
            room_id: room_id.to_string(),
            title: LwwRegister::new(title.to_string(), now, self.local_peer_id.clone()),
            status: LwwRegister::new(TaskStatus::Open, now, self.local_peer_id.clone()),
            assignee: LwwRegister::new(None, now, self.local_peer_id.clone()),
            created_at: now,
            created_by: self.local_peer_id.clone(),
        };

        let room_tasks = self.tasks.entry(room_id.to_string()).or_default();
        room_tasks.insert(task.id.clone(), task.clone());

        let msg = TaskSyncMessage::FullState {
            room_id: room_id.to_string(),
            tasks: vec![task.clone()],
        };

        Ok((task, msg))
    }

    /// Updates the status of a task.
    ///
    /// # Errors
    ///
    /// Returns [`TaskError::RoomNotFound`] or [`TaskError::TaskNotFound`]
    /// if the room or task does not exist.
    pub fn update_status(
        &mut self,
        room_id: &str,
        task_id: &TaskId,
        new_status: TaskStatus,
    ) -> Result<TaskSyncMessage, TaskError> {
        let now = Self::now_ms();
        let peer_id = self.local_peer_id.clone();
        let task = self.get_task_mut(room_id, task_id)?;
        task.status = LwwRegister::new(new_status, now, peer_id.clone());

        Ok(TaskSyncMessage::FieldUpdate {
            task_id: task_id.clone(),
            room_id: room_id.to_string(),
            field: TaskFieldUpdate::Status(LwwRegister::new(new_status, now, peer_id)),
        })
    }

    /// Updates the assignee of a task.
    ///
    /// # Errors
    ///
    /// Returns [`TaskError::RoomNotFound`] or [`TaskError::TaskNotFound`]
    /// if the room or task does not exist.
    pub fn update_assignee(
        &mut self,
        room_id: &str,
        task_id: &TaskId,
        assignee: Option<String>,
    ) -> Result<TaskSyncMessage, TaskError> {
        let now = Self::now_ms();
        let peer_id = self.local_peer_id.clone();
        let task = self.get_task_mut(room_id, task_id)?;
        task.assignee = LwwRegister::new(assignee.clone(), now, peer_id.clone());

        Ok(TaskSyncMessage::FieldUpdate {
            task_id: task_id.clone(),
            room_id: room_id.to_string(),
            field: TaskFieldUpdate::Assignee(LwwRegister::new(assignee, now, peer_id)),
        })
    }

    /// Soft-deletes a task by setting its status to [`TaskStatus::Deleted`].
    ///
    /// # Errors
    ///
    /// Returns [`TaskError::RoomNotFound`] or [`TaskError::TaskNotFound`]
    /// if the room or task does not exist.
    pub fn delete_task(
        &mut self,
        room_id: &str,
        task_id: &TaskId,
    ) -> Result<TaskSyncMessage, TaskError> {
        self.update_status(room_id, task_id, TaskStatus::Deleted)
    }

    /// Applies a remote sync message using CRDT merge logic.
    ///
    /// For [`TaskSyncMessage::FieldUpdate`]: applies the field update
    /// to the local task, or creates a stub task if the ID is unknown
    /// (add-wins semantics).
    ///
    /// For [`TaskSyncMessage::FullState`]: merges all remote tasks into
    /// the local state using [`merge_task_list`].
    ///
    /// For [`TaskSyncMessage::RequestFullState`]: no-op (caller should
    /// use [`build_full_state`](Self::build_full_state) to respond).
    pub fn apply_remote(&mut self, msg: &TaskSyncMessage) {
        match msg {
            TaskSyncMessage::FieldUpdate {
                task_id,
                room_id,
                field,
            } => {
                let room_tasks = self.tasks.entry(room_id.clone()).or_default();
                if let Some(task) = room_tasks.get_mut(task_id) {
                    apply_field_update(task, field);
                } else {
                    // Add-wins: create a stub task from the field update
                    let (timestamp, author) = match field {
                        TaskFieldUpdate::Title(reg) => (reg.timestamp, reg.author.clone()),
                        TaskFieldUpdate::Status(reg) => (reg.timestamp, reg.author.clone()),
                        TaskFieldUpdate::Assignee(reg) => (reg.timestamp, reg.author.clone()),
                    };
                    let mut task = Task {
                        id: task_id.clone(),
                        room_id: room_id.clone(),
                        title: LwwRegister::new(String::new(), 0, String::new()),
                        status: LwwRegister::new(TaskStatus::Open, 0, String::new()),
                        assignee: LwwRegister::new(None, 0, String::new()),
                        created_at: timestamp,
                        created_by: author,
                    };
                    apply_field_update(&mut task, field);
                    room_tasks.insert(task_id.clone(), task);
                }
            }
            TaskSyncMessage::FullState { room_id, tasks } => {
                let room_tasks = self.tasks.entry(room_id.clone()).or_default();
                merge_task_list(room_tasks, tasks);
            }
            TaskSyncMessage::RequestFullState { .. } => {
                // No-op: caller should use build_full_state() to respond
            }
        }
    }

    /// Returns all non-deleted tasks in a room, sorted by creation time.
    ///
    /// Returns an empty vec if the room has no tasks.
    #[must_use]
    pub fn get_tasks(&self, room_id: &str) -> Vec<&Task> {
        let Some(room_tasks) = self.tasks.get(room_id) else {
            return Vec::new();
        };
        let mut tasks: Vec<&Task> = room_tasks
            .values()
            .filter(|t| t.status.value != TaskStatus::Deleted)
            .collect();
        tasks.sort_by_key(|t| t.created_at);
        tasks
    }

    /// Builds a full state snapshot for a room, suitable for sending
    /// to a newly-joined peer.
    ///
    /// Returns `None` if the room has no tasks.
    #[must_use]
    pub fn build_full_state(&self, room_id: &str) -> Option<TaskSyncMessage> {
        let room_tasks = self.tasks.get(room_id)?;
        if room_tasks.is_empty() {
            return None;
        }
        Some(TaskSyncMessage::FullState {
            room_id: room_id.to_string(),
            tasks: room_tasks.values().cloned().collect(),
        })
    }

    /// Returns a mutable reference to a task, or an error if not found.
    fn get_task_mut(&mut self, room_id: &str, task_id: &TaskId) -> Result<&mut Task, TaskError> {
        let room_tasks = self
            .tasks
            .get_mut(room_id)
            .ok_or_else(|| TaskError::RoomNotFound(room_id.to_string()))?;
        room_tasks
            .get_mut(task_id)
            .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> TaskManager {
        TaskManager::new("local-peer".to_string())
    }

    // --- create_task tests ---

    #[test]
    fn create_task_success() {
        let mut mgr = make_manager();
        let (task, msg) = mgr.create_task("room-1", "Fix login bug").unwrap();
        assert_eq!(task.title.value, "Fix login bug");
        assert_eq!(task.status.value, TaskStatus::Open);
        assert_eq!(task.assignee.value, None);
        assert_eq!(task.room_id, "room-1");
        assert_eq!(task.created_by, "local-peer");
        assert!(matches!(msg, TaskSyncMessage::FullState { .. }));
    }

    #[test]
    fn create_task_empty_title_error() {
        let mut mgr = make_manager();
        let err = mgr.create_task("room-1", "").unwrap_err();
        assert_eq!(err, TaskError::TitleEmpty);
    }

    #[test]
    fn create_task_title_too_long_error() {
        let mut mgr = make_manager();
        let long_title = "x".repeat(257);
        let err = mgr.create_task("room-1", &long_title).unwrap_err();
        assert_eq!(err, TaskError::TitleTooLong);
    }

    #[test]
    fn create_task_max_length_title_ok() {
        let mut mgr = make_manager();
        let title = "x".repeat(256);
        let result = mgr.create_task("room-1", &title);
        assert!(result.is_ok());
    }

    #[test]
    fn create_task_appears_in_get_tasks() {
        let mut mgr = make_manager();
        mgr.create_task("room-1", "Task A").unwrap();
        mgr.create_task("room-1", "Task B").unwrap();
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks.len(), 2);
    }

    // --- update_status tests ---

    #[test]
    fn update_status_success() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "My task").unwrap();
        let msg = mgr
            .update_status("room-1", &task.id, TaskStatus::InProgress)
            .unwrap();
        assert!(matches!(msg, TaskSyncMessage::FieldUpdate { .. }));
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks[0].status.value, TaskStatus::InProgress);
    }

    #[test]
    fn update_status_room_not_found() {
        let mut mgr = make_manager();
        let id = TaskId::new();
        let err = mgr
            .update_status("nonexistent", &id, TaskStatus::Completed)
            .unwrap_err();
        assert!(matches!(err, TaskError::RoomNotFound(_)));
    }

    #[test]
    fn update_status_task_not_found() {
        let mut mgr = make_manager();
        mgr.create_task("room-1", "A task").unwrap();
        let bad_id = TaskId::new();
        let err = mgr
            .update_status("room-1", &bad_id, TaskStatus::Completed)
            .unwrap_err();
        assert!(matches!(err, TaskError::TaskNotFound(_)));
    }

    // --- update_assignee tests ---

    #[test]
    fn update_assignee_success() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "My task").unwrap();
        let msg = mgr
            .update_assignee("room-1", &task.id, Some("alice".to_string()))
            .unwrap();
        assert!(matches!(msg, TaskSyncMessage::FieldUpdate { .. }));
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks[0].assignee.value, Some("alice".to_string()));
    }

    #[test]
    fn update_assignee_clear() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "My task").unwrap();
        mgr.update_assignee("room-1", &task.id, Some("alice".to_string()))
            .unwrap();
        mgr.update_assignee("room-1", &task.id, None).unwrap();
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks[0].assignee.value, None);
    }

    // --- delete_task tests ---

    #[test]
    fn delete_task_removes_from_get_tasks() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "Doomed task").unwrap();
        mgr.delete_task("room-1", &task.id).unwrap();
        let tasks = mgr.get_tasks("room-1");
        assert!(tasks.is_empty());
    }

    #[test]
    fn delete_task_not_found() {
        let mut mgr = make_manager();
        let id = TaskId::new();
        let err = mgr.delete_task("room-1", &id).unwrap_err();
        assert!(matches!(err, TaskError::RoomNotFound(_)));
    }

    // --- get_tasks tests ---

    #[test]
    fn get_tasks_empty_room() {
        let mgr = make_manager();
        let tasks = mgr.get_tasks("nonexistent");
        assert!(tasks.is_empty());
    }

    #[test]
    fn get_tasks_sorted_by_created_at() {
        let mut mgr = make_manager();
        // Tasks get increasing timestamps from SystemTime::now()
        mgr.create_task("room-1", "First").unwrap();
        mgr.create_task("room-1", "Second").unwrap();
        mgr.create_task("room-1", "Third").unwrap();
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks.len(), 3);
        assert!(tasks[0].created_at <= tasks[1].created_at);
        assert!(tasks[1].created_at <= tasks[2].created_at);
    }

    // --- build_full_state tests ---

    #[test]
    fn build_full_state_none_for_unknown_room() {
        let mgr = make_manager();
        assert!(mgr.build_full_state("nonexistent").is_none());
    }

    #[test]
    fn build_full_state_returns_all_tasks() {
        let mut mgr = make_manager();
        mgr.create_task("room-1", "Task A").unwrap();
        mgr.create_task("room-1", "Task B").unwrap();
        let msg = mgr.build_full_state("room-1").unwrap();
        if let TaskSyncMessage::FullState { tasks, room_id } = msg {
            assert_eq!(room_id, "room-1");
            assert_eq!(tasks.len(), 2);
        } else {
            panic!("expected FullState");
        }
    }

    // --- apply_remote tests ---

    #[test]
    fn apply_remote_full_state_adds_tasks() {
        let mut mgr = make_manager();
        let task = Task {
            id: TaskId::new(),
            room_id: "room-1".to_string(),
            title: LwwRegister::new("Remote task".to_string(), 100, "peer-b".to_string()),
            status: LwwRegister::new(TaskStatus::Open, 100, "peer-b".to_string()),
            assignee: LwwRegister::new(None, 100, "peer-b".to_string()),
            created_at: 100,
            created_by: "peer-b".to_string(),
        };
        let msg = TaskSyncMessage::FullState {
            room_id: "room-1".to_string(),
            tasks: vec![task],
        };
        mgr.apply_remote(&msg);
        assert_eq!(mgr.get_tasks("room-1").len(), 1);
    }

    #[test]
    fn apply_remote_field_update_existing_task() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "My task").unwrap();
        let msg = TaskSyncMessage::FieldUpdate {
            task_id: task.id.clone(),
            room_id: "room-1".to_string(),
            field: TaskFieldUpdate::Status(LwwRegister::new(
                TaskStatus::Completed,
                u64::MAX,
                "peer-b".to_string(),
            )),
        };
        mgr.apply_remote(&msg);
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks[0].status.value, TaskStatus::Completed);
    }

    #[test]
    fn apply_remote_field_update_unknown_task_add_wins() {
        let mut mgr = make_manager();
        let task_id = TaskId::new();
        let msg = TaskSyncMessage::FieldUpdate {
            task_id: task_id.clone(),
            room_id: "room-1".to_string(),
            field: TaskFieldUpdate::Title(LwwRegister::new(
                "Ghost task".to_string(),
                100,
                "peer-b".to_string(),
            )),
        };
        mgr.apply_remote(&msg);
        // Task should exist even though we never created it locally
        let room_tasks = mgr.tasks.get("room-1").unwrap();
        assert!(room_tasks.contains_key(&task_id));
    }

    #[test]
    fn apply_remote_request_full_state_is_noop() {
        let mut mgr = make_manager();
        mgr.create_task("room-1", "A task").unwrap();
        let msg = TaskSyncMessage::RequestFullState {
            room_id: "room-1".to_string(),
        };
        mgr.apply_remote(&msg);
        // Should not change anything
        assert_eq!(mgr.get_tasks("room-1").len(), 1);
    }

    // --- Extension tests (Task #5) ---

    #[test]
    fn apply_remote_stale_field_update_rejected_silently() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "My task").unwrap();
        // Create a field update with timestamp 0 (definitely stale)
        let msg = TaskSyncMessage::FieldUpdate {
            task_id: task.id.clone(),
            room_id: "room-1".to_string(),
            field: TaskFieldUpdate::Title(LwwRegister::new(
                "stale title".to_string(),
                0,
                "peer-b".to_string(),
            )),
        };
        mgr.apply_remote(&msg);
        // Title should not have changed
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks[0].title.value, "My task");
    }

    #[test]
    fn apply_remote_unknown_task_status_update_creates_stub() {
        let mut mgr = make_manager();
        let task_id = TaskId::new();
        let msg = TaskSyncMessage::FieldUpdate {
            task_id: task_id.clone(),
            room_id: "room-1".to_string(),
            field: TaskFieldUpdate::Status(LwwRegister::new(
                TaskStatus::Completed,
                100,
                "peer-b".to_string(),
            )),
        };
        mgr.apply_remote(&msg);
        let room_tasks = mgr.tasks.get("room-1").unwrap();
        let stub = room_tasks.get(&task_id).unwrap();
        assert_eq!(stub.status.value, TaskStatus::Completed);
        assert_eq!(stub.created_by, "peer-b");
    }

    #[test]
    fn apply_remote_unknown_task_assignee_update_creates_stub() {
        let mut mgr = make_manager();
        let task_id = TaskId::new();
        let msg = TaskSyncMessage::FieldUpdate {
            task_id: task_id.clone(),
            room_id: "room-1".to_string(),
            field: TaskFieldUpdate::Assignee(LwwRegister::new(
                Some("alice".to_string()),
                100,
                "peer-b".to_string(),
            )),
        };
        mgr.apply_remote(&msg);
        let room_tasks = mgr.tasks.get("room-1").unwrap();
        let stub = room_tasks.get(&task_id).unwrap();
        assert_eq!(stub.assignee.value, Some("alice".to_string()));
    }

    #[test]
    fn create_task_whitespace_only_is_not_empty() {
        let mut mgr = make_manager();
        // Whitespace-only string is technically non-empty
        let result = mgr.create_task("room-1", "   ");
        assert!(result.is_ok());
    }

    #[test]
    fn create_task_unicode_title_length_counts_chars() {
        let mut mgr = make_manager();
        // 256 Unicode chars (each char is multi-byte)
        let title: String = std::iter::repeat('ñ').take(256).collect();
        assert!(mgr.create_task("room-1", &title).is_ok());

        let title_too_long: String = std::iter::repeat('ñ').take(257).collect();
        assert_eq!(
            mgr.create_task("room-1", &title_too_long).unwrap_err(),
            TaskError::TitleTooLong
        );
    }

    #[test]
    fn delete_task_still_in_full_state() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "To delete").unwrap();
        mgr.delete_task("room-1", &task.id).unwrap();
        // Deleted tasks should NOT appear in get_tasks
        assert!(mgr.get_tasks("room-1").is_empty());
        // But they SHOULD still be in build_full_state (for sync convergence)
        let state = mgr.build_full_state("room-1").unwrap();
        if let TaskSyncMessage::FullState { tasks, .. } = state {
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks[0].status.value, TaskStatus::Deleted);
        } else {
            panic!("expected FullState");
        }
    }

    #[test]
    fn apply_remote_full_state_merges_with_existing() {
        let mut mgr = make_manager();
        let (local_task, _) = mgr.create_task("room-1", "Local task").unwrap();
        // Remote has both an update to the local task and a new task
        let updated_local = Task {
            id: local_task.id.clone(),
            room_id: "room-1".to_string(),
            title: LwwRegister::new(
                "Updated by remote".to_string(),
                u64::MAX,
                "peer-b".to_string(),
            ),
            status: LwwRegister::new(TaskStatus::Open, 0, "peer-a".to_string()),
            assignee: LwwRegister::new(None, 0, "peer-a".to_string()),
            created_at: local_task.created_at,
            created_by: "local-peer".to_string(),
        };
        let new_remote = Task {
            id: TaskId::new(),
            room_id: "room-1".to_string(),
            title: LwwRegister::new("Remote-only task".to_string(), 100, "peer-b".to_string()),
            status: LwwRegister::new(TaskStatus::Open, 100, "peer-b".to_string()),
            assignee: LwwRegister::new(None, 100, "peer-b".to_string()),
            created_at: 100,
            created_by: "peer-b".to_string(),
        };
        let msg = TaskSyncMessage::FullState {
            room_id: "room-1".to_string(),
            tasks: vec![updated_local, new_remote],
        };
        mgr.apply_remote(&msg);
        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks.len(), 2);
        // Find the updated local task
        let found = tasks.iter().find(|t| t.id == local_task.id).unwrap();
        assert_eq!(found.title.value, "Updated by remote");
    }

    #[test]
    fn multiple_rooms_are_independent() {
        let mut mgr = make_manager();
        mgr.create_task("room-a", "Task in A").unwrap();
        mgr.create_task("room-b", "Task in B").unwrap();
        assert_eq!(mgr.get_tasks("room-a").len(), 1);
        assert_eq!(mgr.get_tasks("room-b").len(), 1);
        assert_eq!(mgr.get_tasks("room-a")[0].title.value, "Task in A");
        assert_eq!(mgr.get_tasks("room-b")[0].title.value, "Task in B");
    }

    // --- FullState sync tests (Task #6) ---

    #[test]
    fn full_state_round_trip_between_managers() {
        let mut mgr_a = TaskManager::new("peer-a".to_string());
        mgr_a.create_task("room-1", "Task from A").unwrap();
        mgr_a.create_task("room-1", "Another from A").unwrap();

        let state_msg = mgr_a.build_full_state("room-1").unwrap();

        let mut mgr_b = TaskManager::new("peer-b".to_string());
        mgr_b.apply_remote(&state_msg);

        assert_eq!(mgr_b.get_tasks("room-1").len(), 2);
    }

    #[test]
    fn full_state_merge_preserves_newer_local() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "Local task").unwrap();
        mgr.update_status("room-1", &task.id, TaskStatus::InProgress)
            .unwrap();

        let remote_task = Task {
            id: task.id.clone(),
            room_id: "room-1".to_string(),
            title: LwwRegister::new("Local task".to_string(), 0, "peer-b".to_string()),
            status: LwwRegister::new(TaskStatus::Open, 0, "peer-b".to_string()),
            assignee: LwwRegister::new(None, 0, "peer-b".to_string()),
            created_at: 0,
            created_by: "peer-b".to_string(),
        };
        let msg = TaskSyncMessage::FullState {
            room_id: "room-1".to_string(),
            tasks: vec![remote_task],
        };
        mgr.apply_remote(&msg);

        let tasks = mgr.get_tasks("room-1");
        assert_eq!(tasks[0].status.value, TaskStatus::InProgress);
    }

    #[test]
    fn full_state_idempotent_apply() {
        let mut mgr = make_manager();
        let remote_task = Task {
            id: TaskId::new(),
            room_id: "room-1".to_string(),
            title: LwwRegister::new("Remote".to_string(), 100, "peer-b".to_string()),
            status: LwwRegister::new(TaskStatus::Open, 100, "peer-b".to_string()),
            assignee: LwwRegister::new(None, 100, "peer-b".to_string()),
            created_at: 100,
            created_by: "peer-b".to_string(),
        };
        let msg = TaskSyncMessage::FullState {
            room_id: "room-1".to_string(),
            tasks: vec![remote_task],
        };
        mgr.apply_remote(&msg);
        mgr.apply_remote(&msg);
        assert_eq!(mgr.get_tasks("room-1").len(), 1);
    }

    #[test]
    fn request_full_state_response_flow() {
        let mut mgr = make_manager();
        mgr.create_task("room-1", "Task A").unwrap();
        mgr.create_task("room-1", "Task B").unwrap();

        let request = TaskSyncMessage::RequestFullState {
            room_id: "room-1".to_string(),
        };
        mgr.apply_remote(&request);

        let response = mgr.build_full_state("room-1").unwrap();
        if let TaskSyncMessage::FullState { tasks, room_id } = response {
            assert_eq!(room_id, "room-1");
            assert_eq!(tasks.len(), 2);
        } else {
            panic!("expected FullState response");
        }
    }

    #[test]
    fn build_full_state_empty_room_after_all_deleted() {
        let mut mgr = make_manager();
        let (task, _) = mgr.create_task("room-1", "Will delete").unwrap();
        mgr.delete_task("room-1", &task.id).unwrap();
        let state = mgr.build_full_state("room-1");
        assert!(state.is_some());
    }
}
