//! Shared task coordination for `TermChat` rooms.
//!
//! Provides room-scoped task lists with CRDT-based synchronization
//! using Last-Write-Wins (LWW) registers per field. Task changes
//! are broadcast to all room members as encrypted `TaskSync` messages.

pub mod manager;
pub mod merge;

pub use manager::TaskManager;
pub use merge::{apply_field_update, merge_lww, merge_task, merge_task_list};

use thiserror::Error;

/// Errors that can occur during task operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TaskError {
    /// Task title cannot be empty.
    #[error("task title cannot be empty")]
    TitleEmpty,
    /// Task title exceeds the maximum length.
    #[error("task title too long (max 256 characters)")]
    TitleTooLong,
    /// Task with the given ID was not found.
    #[error("task not found: {0}")]
    TaskNotFound(String),
    /// Room with the given ID was not found.
    #[error("room not found: {0}")]
    RoomNotFound(String),
    /// Invalid assignee peer ID.
    #[error("invalid assignee: {0}")]
    InvalidAssignee(String),
}
