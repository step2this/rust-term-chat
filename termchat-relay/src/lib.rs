//! `TermChat` Relay Server library.
//!
//! Exposes the relay server for use in tests and embedding.
//! The relay server accepts WebSocket connections, registers peers,
//! and routes encrypted payloads between them.

pub mod relay;
pub mod rooms;
pub mod store;
