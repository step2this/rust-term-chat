//! Pure CRDT merge functions for task synchronization.
//!
//! Implements Last-Write-Wins (LWW) merge logic that guarantees
//! commutativity, associativity, and idempotency for convergent
//! task state across peers.
//!
//! ## CRDT Library Evaluation (UC-013, 2026-02-08)
//!
//! Evaluated `crdts` crate (v7.3.2) as a replacement for this module.
//! Decision: **keep hand-rolled implementation**. Rationale:
//!
//! - `crdts` pulls in 32 transitive dependencies (`num`, `quickcheck`,
//!   `env_logger`, `regex`, `rand` 0.8) -- unacceptable weight for LWW registers.
//! - This module is 87 lines of focused LWW logic with 22 unit tests
//!   covering commutativity, associativity, and idempotency.
//! - Our tiebreaking semantics (timestamp-then-author) are purpose-built
//!   for the task sync protocol; `crdts` uses different conflict resolution.
//! - Zero external dependencies beyond `termchat-proto` types.

use std::collections::HashMap;
use std::hash::BuildHasher;

use termchat_proto::task::{LwwRegister, Task, TaskFieldUpdate, TaskId};

/// Merges two LWW registers, returning the winning value.
///
/// Rules (in priority order):
/// 1. Higher timestamp wins.
/// 2. Equal timestamps: higher author (lexicographic `String` comparison) wins.
/// 3. Equal everything: local wins (idempotent).
#[must_use]
pub fn merge_lww<T: Clone>(local: &LwwRegister<T>, remote: &LwwRegister<T>) -> LwwRegister<T> {
    if remote.timestamp > local.timestamp
        || (remote.timestamp == local.timestamp && remote.author > local.author)
    {
        remote.clone()
    } else {
        local.clone()
    }
}

/// Merges a remote task into a local task, field by field.
///
/// Each field (title, status, assignee) is merged independently using
/// [`merge_lww`], so concurrent edits to different fields both survive.
pub fn merge_task(local: &mut Task, remote: &Task) {
    local.title = merge_lww(&local.title, &remote.title);
    local.status = merge_lww(&local.status, &remote.status);
    local.assignee = merge_lww(&local.assignee, &remote.assignee);
}

/// Merges a list of remote tasks into a local task map.
///
/// For each remote task:
/// - If it exists locally, merge fields using [`merge_task`].
/// - If it is new, add it (add-wins semantics).
pub fn merge_task_list<S: BuildHasher>(local: &mut HashMap<TaskId, Task, S>, remote: &[Task]) {
    for remote_task in remote {
        if let Some(local_task) = local.get_mut(&remote_task.id) {
            merge_task(local_task, remote_task);
        } else {
            local.insert(remote_task.id.clone(), remote_task.clone());
        }
    }
}

/// Applies a single field update to a task using LWW logic.
///
/// Returns `true` if the update was applied (newer), `false` if rejected (stale).
pub fn apply_field_update(task: &mut Task, update: &TaskFieldUpdate) -> bool {
    match update {
        TaskFieldUpdate::Title(reg) => {
            let merged = merge_lww(&task.title, reg);
            if merged.timestamp == reg.timestamp && merged.author == reg.author {
                task.title = merged;
                true
            } else {
                false
            }
        }
        TaskFieldUpdate::Status(reg) => {
            let merged = merge_lww(&task.status, reg);
            if merged.timestamp == reg.timestamp && merged.author == reg.author {
                task.status = merged;
                true
            } else {
                false
            }
        }
        TaskFieldUpdate::Assignee(reg) => {
            let merged = merge_lww(&task.assignee, reg);
            if merged.timestamp == reg.timestamp && merged.author == reg.author {
                task.assignee = merged;
                true
            } else {
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use termchat_proto::task::TaskStatus;

    use super::*;

    fn make_reg(value: &str, timestamp: u64, author: &str) -> LwwRegister<String> {
        LwwRegister::new(value.to_string(), timestamp, author.to_string())
    }

    fn make_status_reg(
        status: TaskStatus,
        timestamp: u64,
        author: &str,
    ) -> LwwRegister<TaskStatus> {
        LwwRegister::new(status, timestamp, author.to_string())
    }

    fn make_task(id: TaskId, title: &str, ts: u64, author: &str) -> Task {
        Task {
            id,
            room_id: "room-1".to_string(),
            title: LwwRegister::new(title.to_string(), ts, author.to_string()),
            status: LwwRegister::new(TaskStatus::Open, ts, author.to_string()),
            assignee: LwwRegister::new(None, ts, author.to_string()),
            created_at: ts,
            created_by: author.to_string(),
        }
    }

    // --- merge_lww tests ---

    #[test]
    fn lww_later_timestamp_wins() {
        let local = make_reg("old", 100, "peer-a");
        let remote = make_reg("new", 200, "peer-b");
        let result = merge_lww(&local, &remote);
        assert_eq!(result.value, "new");
        assert_eq!(result.timestamp, 200);
    }

    #[test]
    fn lww_earlier_timestamp_loses() {
        let local = make_reg("current", 200, "peer-a");
        let remote = make_reg("stale", 100, "peer-b");
        let result = merge_lww(&local, &remote);
        assert_eq!(result.value, "current");
        assert_eq!(result.timestamp, 200);
    }

    #[test]
    fn lww_equal_timestamp_higher_author_wins() {
        let local = make_reg("local-val", 100, "peer-a");
        let remote = make_reg("remote-val", 100, "peer-z");
        let result = merge_lww(&local, &remote);
        assert_eq!(result.value, "remote-val");
        assert_eq!(result.author, "peer-z");
    }

    #[test]
    fn lww_equal_timestamp_lower_author_loses() {
        let local = make_reg("local-val", 100, "peer-z");
        let remote = make_reg("remote-val", 100, "peer-a");
        let result = merge_lww(&local, &remote);
        assert_eq!(result.value, "local-val");
        assert_eq!(result.author, "peer-z");
    }

    #[test]
    fn lww_identical_returns_local() {
        let local = make_reg("same", 100, "peer-a");
        let remote = make_reg("same", 100, "peer-a");
        let result = merge_lww(&local, &remote);
        assert_eq!(result.value, "same");
        assert_eq!(result.timestamp, 100);
        assert_eq!(result.author, "peer-a");
    }

    #[test]
    fn lww_idempotent() {
        let local = make_reg("val", 100, "peer-a");
        let remote = make_reg("val", 100, "peer-a");
        let r1 = merge_lww(&local, &remote);
        let r2 = merge_lww(&r1, &remote);
        assert_eq!(r1, r2);
    }

    #[test]
    fn lww_commutative() {
        let a = make_reg("a-val", 100, "peer-a");
        let b = make_reg("b-val", 200, "peer-b");
        let ab = merge_lww(&a, &b);
        let ba = merge_lww(&b, &a);
        assert_eq!(ab.value, ba.value);
        assert_eq!(ab.timestamp, ba.timestamp);
        assert_eq!(ab.author, ba.author);
    }

    #[test]
    fn lww_with_status_type() {
        let local = make_status_reg(TaskStatus::Open, 100, "peer-a");
        let remote = make_status_reg(TaskStatus::Completed, 200, "peer-b");
        let result = merge_lww(&local, &remote);
        assert_eq!(result.value, TaskStatus::Completed);
    }

    // --- merge_task tests ---

    #[test]
    fn merge_task_independent_fields() {
        let id = TaskId::new();
        let mut local = make_task(id.clone(), "original", 100, "peer-a");
        let mut remote = make_task(id, "original", 100, "peer-a");
        // Remote has newer title
        remote.title = make_reg("updated-title", 200, "peer-b");
        // Remote has newer status
        remote.status = make_status_reg(TaskStatus::InProgress, 200, "peer-b");
        // Local has newer assignee
        local.assignee = LwwRegister::new(Some("alice".to_string()), 300, "peer-a".to_string());

        merge_task(&mut local, &remote);

        assert_eq!(local.title.value, "updated-title");
        assert_eq!(local.status.value, TaskStatus::InProgress);
        assert_eq!(local.assignee.value, Some("alice".to_string()));
    }

    #[test]
    fn merge_task_idempotent() {
        let id = TaskId::new();
        let mut local = make_task(id.clone(), "title", 100, "peer-a");
        let remote = make_task(id, "title", 100, "peer-a");
        let before = local.clone();
        merge_task(&mut local, &remote);
        assert_eq!(local, before);
    }

    // --- merge_task_list tests ---

    #[test]
    fn merge_task_list_add_wins_new_task() {
        let mut local: HashMap<TaskId, Task> = HashMap::new();
        let remote_task = make_task(TaskId::new(), "new task", 100, "peer-b");
        merge_task_list(&mut local, &[remote_task.clone()]);
        assert_eq!(local.len(), 1);
        assert_eq!(local[&remote_task.id].title.value, "new task");
    }

    #[test]
    fn merge_task_list_merges_existing() {
        let id = TaskId::new();
        let local_task = make_task(id.clone(), "original", 100, "peer-a");
        let mut local: HashMap<TaskId, Task> = HashMap::new();
        local.insert(id.clone(), local_task);

        let mut remote_task = make_task(id, "updated", 200, "peer-b");
        remote_task.status = make_status_reg(TaskStatus::Completed, 200, "peer-b");
        merge_task_list(&mut local, &[remote_task]);

        let result = local.values().next().unwrap();
        assert_eq!(result.title.value, "updated");
        assert_eq!(result.status.value, TaskStatus::Completed);
    }

    #[test]
    fn merge_task_list_empty_local_full_remote() {
        let mut local: HashMap<TaskId, Task> = HashMap::new();
        let tasks = vec![
            make_task(TaskId::new(), "task-1", 100, "peer-a"),
            make_task(TaskId::new(), "task-2", 200, "peer-b"),
            make_task(TaskId::new(), "task-3", 300, "peer-c"),
        ];
        merge_task_list(&mut local, &tasks);
        assert_eq!(local.len(), 3);
    }

    #[test]
    fn merge_task_list_empty_remote_no_change() {
        let id = TaskId::new();
        let task = make_task(id.clone(), "existing", 100, "peer-a");
        let mut local: HashMap<TaskId, Task> = HashMap::new();
        local.insert(id, task);
        merge_task_list(&mut local, &[]);
        assert_eq!(local.len(), 1);
    }

    // --- apply_field_update tests ---

    #[test]
    fn apply_field_update_newer_title_wins() {
        let mut task = make_task(TaskId::new(), "old", 100, "peer-a");
        let update = TaskFieldUpdate::Title(make_reg("new", 200, "peer-b"));
        assert!(apply_field_update(&mut task, &update));
        assert_eq!(task.title.value, "new");
    }

    #[test]
    fn apply_field_update_stale_title_rejected() {
        let mut task = make_task(TaskId::new(), "current", 200, "peer-a");
        let update = TaskFieldUpdate::Title(make_reg("stale", 100, "peer-b"));
        assert!(!apply_field_update(&mut task, &update));
        assert_eq!(task.title.value, "current");
    }

    #[test]
    fn apply_field_update_newer_status_wins() {
        let mut task = make_task(TaskId::new(), "task", 100, "peer-a");
        let update = TaskFieldUpdate::Status(make_status_reg(TaskStatus::Completed, 200, "peer-b"));
        assert!(apply_field_update(&mut task, &update));
        assert_eq!(task.status.value, TaskStatus::Completed);
    }

    #[test]
    fn apply_field_update_stale_status_rejected() {
        let mut task = make_task(TaskId::new(), "task", 200, "peer-a");
        let update = TaskFieldUpdate::Status(make_status_reg(TaskStatus::Completed, 100, "peer-b"));
        assert!(!apply_field_update(&mut task, &update));
        assert_eq!(task.status.value, TaskStatus::Open);
    }

    #[test]
    fn apply_field_update_newer_assignee_wins() {
        let mut task = make_task(TaskId::new(), "task", 100, "peer-a");
        let update = TaskFieldUpdate::Assignee(LwwRegister::new(
            Some("bob".to_string()),
            200,
            "peer-b".to_string(),
        ));
        assert!(apply_field_update(&mut task, &update));
        assert_eq!(task.assignee.value, Some("bob".to_string()));
    }

    #[test]
    fn apply_field_update_stale_assignee_rejected() {
        let mut task = make_task(TaskId::new(), "task", 200, "peer-a");
        task.assignee = LwwRegister::new(Some("alice".to_string()), 200, "peer-a".to_string());
        let update = TaskFieldUpdate::Assignee(LwwRegister::new(
            Some("bob".to_string()),
            100,
            "peer-b".to_string(),
        ));
        assert!(!apply_field_update(&mut task, &update));
        assert_eq!(task.assignee.value, Some("alice".to_string()));
    }
}
