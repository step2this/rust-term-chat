//! Integration tests for UC-008: Share Task List.
//!
//! Tests CRDT merge correctness, `TaskManager` operations,
//! and end-to-end task synchronization between peers.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::similar_names,
    clippy::redundant_clone,
    clippy::cloned_ref_to_slice_refs
)]

use std::collections::HashMap;

use termchat::tasks::{TaskError, TaskManager, merge_lww, merge_task, merge_task_list};
use termchat_proto::task::{
    LwwRegister, MAX_TASK_TITLE_LENGTH, Task, TaskFieldUpdate, TaskId, TaskStatus, TaskSyncMessage,
    decode, encode,
};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Creates a `TaskManager` with the given peer ID.
fn make_manager(peer_id: &str) -> TaskManager {
    TaskManager::new(peer_id.to_string())
}

/// Creates a test task with explicit field values.
fn make_test_task(id: TaskId, room_id: &str, title: &str, ts: u64, author: &str) -> Task {
    Task {
        id,
        room_id: room_id.to_string(),
        title: LwwRegister::new(title.to_string(), ts, author.to_string()),
        status: LwwRegister::new(TaskStatus::Open, ts, author.to_string()),
        assignee: LwwRegister::new(None, ts, author.to_string()),
        created_at: ts,
        created_by: author.to_string(),
    }
}

/// Creates a string LWW register with the given value, timestamp, and author.
fn make_lww_string(value: &str, ts: u64, author: &str) -> LwwRegister<String> {
    LwwRegister::new(value.to_string(), ts, author.to_string())
}

/// Creates a status LWW register with the given status, timestamp, and author.
fn make_lww_status(status: TaskStatus, ts: u64, author: &str) -> LwwRegister<TaskStatus> {
    LwwRegister::new(status, ts, author.to_string())
}

/// Creates an assignee LWW register.
fn make_lww_assignee(assignee: Option<&str>, ts: u64, author: &str) -> LwwRegister<Option<String>> {
    LwwRegister::new(assignee.map(String::from), ts, author.to_string())
}

// ===========================================================================
// Task #17: CRDT merge + TaskManager integration tests
// ===========================================================================

// --- merge_lww tests ---

#[test]
fn merge_lww_later_timestamp_wins() {
    let local = make_lww_string("old", 100, "peer-a");
    let remote = make_lww_string("new", 200, "peer-b");
    let result = merge_lww(&local, &remote);
    assert_eq!(result.value, "new");
    assert_eq!(result.timestamp, 200);
    assert_eq!(result.author, "peer-b");
}

#[test]
fn merge_lww_earlier_timestamp_loses() {
    let local = make_lww_string("current", 200, "peer-a");
    let remote = make_lww_string("stale", 100, "peer-b");
    let result = merge_lww(&local, &remote);
    assert_eq!(result.value, "current");
    assert_eq!(result.timestamp, 200);
    assert_eq!(result.author, "peer-a");
}

#[test]
fn merge_lww_equal_timestamp_tiebreak_by_author() {
    // Higher author (lexicographic) wins when timestamps tie
    let local = make_lww_string("local-val", 100, "peer-a");
    let remote = make_lww_string("remote-val", 100, "peer-z");
    let result = merge_lww(&local, &remote);
    assert_eq!(result.value, "remote-val");
    assert_eq!(result.author, "peer-z");

    // Reverse: lower author loses
    let result2 = merge_lww(&remote, &local);
    assert_eq!(result2.value, "remote-val");
    assert_eq!(result2.author, "peer-z");
}

// --- merge_task tests ---

#[test]
fn merge_task_independent_field_merge() {
    let id = TaskId::new();
    let mut local = make_test_task(id.clone(), "room-1", "original", 100, "peer-a");
    let mut remote = make_test_task(id, "room-1", "original", 100, "peer-a");

    // Remote has newer title (ts=200)
    remote.title = make_lww_string("updated-title", 200, "peer-b");
    // Local has newer status (ts=300)
    local.status = make_lww_status(TaskStatus::InProgress, 300, "peer-a");

    merge_task(&mut local, &remote);

    // Remote title wins (200 > 100)
    assert_eq!(local.title.value, "updated-title");
    // Local status survives (300 > 100)
    assert_eq!(local.status.value, TaskStatus::InProgress);
}

// --- merge_task_list tests ---

#[test]
fn merge_task_list_add_wins_for_new_tasks() {
    let mut local: HashMap<TaskId, Task> = HashMap::new();
    let remote_task = make_test_task(TaskId::new(), "room-1", "new task", 100, "peer-b");
    merge_task_list(&mut local, &[remote_task.clone()]);
    assert_eq!(local.len(), 1);
    assert_eq!(local[&remote_task.id].title.value, "new task");
}

#[test]
fn merge_task_list_merges_existing_tasks() {
    let id = TaskId::new();
    let local_task = make_test_task(id.clone(), "room-1", "original", 100, "peer-a");
    let mut local: HashMap<TaskId, Task> = HashMap::new();
    local.insert(id.clone(), local_task);

    let mut remote_task = make_test_task(id, "room-1", "updated", 200, "peer-b");
    remote_task.status = make_lww_status(TaskStatus::Completed, 200, "peer-b");
    merge_task_list(&mut local, &[remote_task]);

    let result = local.values().next().expect("should have one task");
    assert_eq!(result.title.value, "updated");
    assert_eq!(result.status.value, TaskStatus::Completed);
}

// --- TaskManager::create_task tests ---

#[test]
fn task_manager_create_task_success() {
    let mut mgr = make_manager("peer-a");
    let (task, msg) = mgr
        .create_task("room-1", "Fix login bug")
        .expect("create_task");
    assert_eq!(task.title.value, "Fix login bug");
    assert_eq!(task.status.value, TaskStatus::Open);
    assert_eq!(task.assignee.value, None);
    assert_eq!(task.room_id, "room-1");
    assert_eq!(task.created_by, "peer-a");
    assert!(matches!(msg, TaskSyncMessage::FullState { .. }));
}

#[test]
fn task_manager_create_task_empty_title_error() {
    let mut mgr = make_manager("peer-a");
    let err = mgr.create_task("room-1", "").expect_err("should fail");
    assert_eq!(err, TaskError::TitleEmpty);
}

#[test]
fn task_manager_create_task_title_too_long_error() {
    let mut mgr = make_manager("peer-a");
    let long_title = "x".repeat(MAX_TASK_TITLE_LENGTH + 1);
    let err = mgr
        .create_task("room-1", &long_title)
        .expect_err("should fail");
    assert_eq!(err, TaskError::TitleTooLong);
}

// --- TaskManager::update_status tests ---

#[test]
fn task_manager_update_status_success() {
    let mut mgr = make_manager("peer-a");
    let (task, _) = mgr.create_task("room-1", "My task").expect("create");
    let msg = mgr
        .update_status("room-1", &task.id, TaskStatus::InProgress)
        .expect("update_status");
    assert!(matches!(msg, TaskSyncMessage::FieldUpdate { .. }));
    let tasks = mgr.get_tasks("room-1");
    assert_eq!(tasks[0].status.value, TaskStatus::InProgress);
}

#[test]
fn task_manager_update_status_room_not_found() {
    let mut mgr = make_manager("peer-a");
    let id = TaskId::new();
    let err = mgr
        .update_status("nonexistent", &id, TaskStatus::Completed)
        .expect_err("should fail");
    assert!(matches!(err, TaskError::RoomNotFound(_)));
}

#[test]
fn task_manager_update_status_task_not_found() {
    let mut mgr = make_manager("peer-a");
    mgr.create_task("room-1", "A task").expect("create");
    let bad_id = TaskId::new();
    let err = mgr
        .update_status("room-1", &bad_id, TaskStatus::Completed)
        .expect_err("should fail");
    assert!(matches!(err, TaskError::TaskNotFound(_)));
}

// --- TaskManager::apply_remote with FieldUpdate ---

#[test]
fn task_manager_apply_remote_field_update_newer_wins() {
    let mut mgr = make_manager("peer-a");
    let (task, _) = mgr.create_task("room-1", "My task").expect("create");
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
    // Deleted filter: Completed is not deleted, should appear
    assert_eq!(tasks[0].status.value, TaskStatus::Completed);
}

#[test]
fn task_manager_apply_remote_field_update_stale_rejected() {
    let mut mgr = make_manager("peer-a");
    let (task, _) = mgr.create_task("room-1", "My task").expect("create");
    let msg = TaskSyncMessage::FieldUpdate {
        task_id: task.id.clone(),
        room_id: "room-1".to_string(),
        field: TaskFieldUpdate::Title(LwwRegister::new(
            "stale title".to_string(),
            0, // timestamp 0 is definitely stale
            "peer-b".to_string(),
        )),
    };
    mgr.apply_remote(&msg);
    let tasks = mgr.get_tasks("room-1");
    assert_eq!(tasks[0].title.value, "My task"); // unchanged
}

// --- TaskManager::apply_remote with FullState ---

#[test]
fn task_manager_apply_remote_full_state_adds_new_tasks() {
    let mut mgr = make_manager("peer-a");
    let remote_task = make_test_task(TaskId::new(), "room-1", "Remote task", 100, "peer-b");
    let msg = TaskSyncMessage::FullState {
        room_id: "room-1".to_string(),
        tasks: vec![remote_task],
    };
    mgr.apply_remote(&msg);
    assert_eq!(mgr.get_tasks("room-1").len(), 1);
    assert_eq!(mgr.get_tasks("room-1")[0].title.value, "Remote task");
}

#[test]
fn task_manager_apply_remote_full_state_merges_existing() {
    let mut mgr = make_manager("peer-a");
    let (task, _) = mgr.create_task("room-1", "Local task").expect("create");
    let mut remote_task = make_test_task(
        task.id.clone(),
        "room-1",
        "Updated title",
        u64::MAX,
        "peer-b",
    );
    remote_task.status = make_lww_status(TaskStatus::Open, 0, "peer-b"); // stale status
    let msg = TaskSyncMessage::FullState {
        room_id: "room-1".to_string(),
        tasks: vec![remote_task],
    };
    mgr.apply_remote(&msg);
    let tasks = mgr.get_tasks("room-1");
    assert_eq!(tasks.len(), 1);
    // Title updated (remote is newer with u64::MAX)
    assert_eq!(tasks[0].title.value, "Updated title");
    // Status preserved (local is newer)
    assert_eq!(tasks[0].status.value, TaskStatus::Open);
}

// --- TaskManager::build_full_state round-trip between two managers ---

#[test]
fn task_manager_build_full_state_round_trip() {
    let mut mgr_a = make_manager("peer-a");
    mgr_a.create_task("room-1", "Task from A").expect("create");
    mgr_a
        .create_task("room-1", "Another from A")
        .expect("create");

    let state_msg = mgr_a.build_full_state("room-1").expect("should have state");

    let mut mgr_b = make_manager("peer-b");
    mgr_b.apply_remote(&state_msg);

    assert_eq!(mgr_b.get_tasks("room-1").len(), 2);
}

// --- TaskManager::delete_task soft-delete ---

#[test]
fn task_manager_delete_removes_from_get_tasks_but_not_full_state() {
    let mut mgr = make_manager("peer-a");
    let (task, _) = mgr.create_task("room-1", "Doomed task").expect("create");
    mgr.delete_task("room-1", &task.id).expect("delete");

    // Should not appear in get_tasks
    assert!(mgr.get_tasks("room-1").is_empty());

    // But should still be in build_full_state for sync convergence
    let state = mgr.build_full_state("room-1").expect("should have state");
    if let TaskSyncMessage::FullState { tasks, .. } = state {
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status.value, TaskStatus::Deleted);
    } else {
        panic!("expected FullState");
    }
}

// --- Two-manager convergence ---

#[test]
fn two_manager_convergence() {
    // Manager A creates task X
    let mut mgr_a = make_manager("peer-a");
    let (task_a, _) = mgr_a.create_task("room-1", "Task from A").expect("create");

    // Manager B creates task Y
    let mut mgr_b = make_manager("peer-b");
    let (task_b, _) = mgr_b.create_task("room-1", "Task from B").expect("create");

    // Exchange full state: A -> B
    let state_a = mgr_a.build_full_state("room-1").expect("state A");
    mgr_b.apply_remote(&state_a);

    // Exchange full state: B -> A
    let state_b = mgr_b.build_full_state("room-1").expect("state B");
    mgr_a.apply_remote(&state_b);

    // Both should now have both tasks
    let tasks_a = mgr_a.get_tasks("room-1");
    let tasks_b = mgr_b.get_tasks("room-1");
    assert_eq!(tasks_a.len(), 2);
    assert_eq!(tasks_b.len(), 2);

    // Both should have the same task IDs
    let ids_a: Vec<&TaskId> = tasks_a.iter().map(|t| &t.id).collect();
    let ids_b: Vec<&TaskId> = tasks_b.iter().map(|t| &t.id).collect();
    assert!(ids_a.contains(&&task_a.id));
    assert!(ids_a.contains(&&task_b.id));
    assert!(ids_b.contains(&&task_a.id));
    assert!(ids_b.contains(&&task_b.id));
}

// ===========================================================================
// Task #18: End-to-end task sync + agent task management tests
// ===========================================================================

// --- Proto round-trip ---

#[test]
fn task_sync_message_encode_decode_round_trip() {
    let task = make_test_task(TaskId::new(), "room-1", "Test task", 1000, "peer-a");
    let msg = TaskSyncMessage::FullState {
        room_id: "room-1".to_string(),
        tasks: vec![task],
    };
    let bytes = encode(&msg).expect("encode");
    let decoded = decode(&bytes).expect("decode");
    assert_eq!(msg, decoded);
}

#[test]
fn task_sync_message_field_update_encode_decode_round_trip() {
    let msg = TaskSyncMessage::FieldUpdate {
        task_id: TaskId::new(),
        room_id: "room-1".to_string(),
        field: TaskFieldUpdate::Status(make_lww_status(TaskStatus::InProgress, 2000, "peer-b")),
    };
    let bytes = encode(&msg).expect("encode");
    let decoded = decode(&bytes).expect("decode");
    assert_eq!(msg, decoded);
}

#[test]
fn task_sync_message_request_full_state_encode_decode() {
    let msg = TaskSyncMessage::RequestFullState {
        room_id: "room-1".to_string(),
    };
    let bytes = encode(&msg).expect("encode");
    let decoded = decode(&bytes).expect("decode");
    assert_eq!(msg, decoded);
}

// --- Three-peer convergence ---

#[test]
fn three_peer_convergence() {
    let mut mgr_a = make_manager("peer-a");
    let mut mgr_b = make_manager("peer-b");
    let mut mgr_c = make_manager("peer-c");

    // A creates a task
    let (task, _) = mgr_a.create_task("room-1", "Shared task").expect("create");

    // Sync A -> B via FullState
    let state_a = mgr_a.build_full_state("room-1").expect("state A");
    mgr_b.apply_remote(&state_a);

    // B updates the status
    let status_msg = mgr_b
        .update_status("room-1", &task.id, TaskStatus::InProgress)
        .expect("update");

    // Sync B's update -> A and C
    mgr_a.apply_remote(&status_msg);
    // C first gets the full state from A (which now has B's update)
    let state_a2 = mgr_a.build_full_state("room-1").expect("state A2");
    mgr_c.apply_remote(&state_a2);

    // All three should converge: status = InProgress
    assert_eq!(
        mgr_a.get_tasks("room-1")[0].status.value,
        TaskStatus::InProgress
    );
    assert_eq!(
        mgr_b.get_tasks("room-1")[0].status.value,
        TaskStatus::InProgress
    );
    assert_eq!(
        mgr_c.get_tasks("room-1")[0].status.value,
        TaskStatus::InProgress
    );

    // All three should have exactly 1 task
    assert_eq!(mgr_a.get_tasks("room-1").len(), 1);
    assert_eq!(mgr_b.get_tasks("room-1").len(), 1);
    assert_eq!(mgr_c.get_tasks("room-1").len(), 1);
}

// --- Concurrent updates to different fields both survive ---

#[test]
fn concurrent_updates_to_different_fields_both_survive() {
    let mut mgr_a = make_manager("peer-a");
    let mut mgr_b = make_manager("peer-b");

    // A creates a task, sync to B
    let (task, _) = mgr_a.create_task("room-1", "Shared task").expect("create");
    let state = mgr_a.build_full_state("room-1").expect("state");
    mgr_b.apply_remote(&state);

    // Small sleep to ensure update timestamps are strictly newer than creation
    std::thread::sleep(std::time::Duration::from_millis(2));

    // A updates assignee, B updates status (concurrently — no sync between them)
    let assign_msg = mgr_a
        .update_assignee("room-1", &task.id, Some("alice".to_string()))
        .expect("assign");
    let status_msg = mgr_b
        .update_status("room-1", &task.id, TaskStatus::Completed)
        .expect("status");

    // Now sync: A gets B's status update, B gets A's assignee update
    mgr_a.apply_remote(&status_msg);
    mgr_b.apply_remote(&assign_msg);

    // Both fields should survive on both managers
    let tasks_a = mgr_a.get_tasks("room-1");
    assert_eq!(tasks_a[0].assignee.value, Some("alice".to_string()));
    assert_eq!(tasks_a[0].status.value, TaskStatus::Completed);

    let tasks_b = mgr_b.get_tasks("room-1");
    assert_eq!(tasks_b[0].assignee.value, Some("alice".to_string()));
    assert_eq!(tasks_b[0].status.value, TaskStatus::Completed);
}

// --- Stale FieldUpdate rejected after FullState with newer timestamp ---

#[test]
fn stale_field_update_rejected_after_full_state_with_newer_timestamp() {
    let mut mgr = make_manager("peer-a");
    let (task, _) = mgr.create_task("room-1", "My task").expect("create");

    // Apply a FullState with a very high timestamp title
    let updated_task = Task {
        id: task.id.clone(),
        room_id: "room-1".to_string(),
        title: LwwRegister::new(
            "FullState title".to_string(),
            u64::MAX - 1,
            "peer-b".to_string(),
        ),
        status: make_lww_status(TaskStatus::Open, u64::MAX - 1, "peer-b"),
        assignee: make_lww_assignee(None, u64::MAX - 1, "peer-b"),
        created_at: task.created_at,
        created_by: "peer-a".to_string(),
    };
    let full_state = TaskSyncMessage::FullState {
        room_id: "room-1".to_string(),
        tasks: vec![updated_task],
    };
    mgr.apply_remote(&full_state);

    // Now a stale FieldUpdate arrives with timestamp 1 — should be rejected
    let stale_update = TaskSyncMessage::FieldUpdate {
        task_id: task.id.clone(),
        room_id: "room-1".to_string(),
        field: TaskFieldUpdate::Title(LwwRegister::new(
            "stale title".to_string(),
            1,
            "peer-c".to_string(),
        )),
    };
    mgr.apply_remote(&stale_update);

    let tasks = mgr.get_tasks("room-1");
    assert_eq!(tasks[0].title.value, "FullState title"); // stale was rejected
}
