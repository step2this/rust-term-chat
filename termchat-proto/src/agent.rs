//! Agent-related protocol types for `TermChat`.
//!
//! Defines the [`AgentInfo`] struct and [`AgentCapability`] enum used
//! to describe an AI agent participant's identity and capabilities
//! within the room protocol.

use serde::{Deserialize, Serialize};

/// Describes an AI agent participant in a room.
///
/// Carried in the bridge handshake and stored alongside room membership
/// information to identify agent participants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Unique identifier for the agent (e.g., "claude-code-1").
    pub agent_id: String,
    /// Human-readable display name (e.g., "Claude").
    pub display_name: String,
    /// Capabilities this agent advertises.
    pub capabilities: Vec<AgentCapability>,
}

/// Capabilities an agent can advertise during handshake.
///
/// Used by the bridge to determine what features the agent supports.
/// Currently only `Chat` is implemented; additional capabilities are
/// reserved for future use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentCapability {
    /// The agent can send and receive chat messages.
    Chat,
    /// The agent can manage shared task lists (future).
    TaskManagement,
    /// The agent can review code and provide feedback (future).
    CodeReview,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_info_round_trip_postcard() {
        let info = AgentInfo {
            agent_id: "claude-code-1".to_string(),
            display_name: "Claude".to_string(),
            capabilities: vec![AgentCapability::Chat],
        };
        let bytes = postcard::to_allocvec(&info).expect("serialize");
        let decoded: AgentInfo = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(info, decoded);
    }

    #[test]
    fn agent_info_multiple_capabilities_round_trip() {
        let info = AgentInfo {
            agent_id: "multi-agent".to_string(),
            display_name: "Multi Agent".to_string(),
            capabilities: vec![
                AgentCapability::Chat,
                AgentCapability::TaskManagement,
                AgentCapability::CodeReview,
            ],
        };
        let bytes = postcard::to_allocvec(&info).expect("serialize");
        let decoded: AgentInfo = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(info, decoded);
    }

    #[test]
    fn agent_info_empty_capabilities_round_trip() {
        let info = AgentInfo {
            agent_id: "basic".to_string(),
            display_name: "Basic Agent".to_string(),
            capabilities: vec![],
        };
        let bytes = postcard::to_allocvec(&info).expect("serialize");
        let decoded: AgentInfo = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(info, decoded);
    }

    #[test]
    fn agent_capability_round_trip_postcard() {
        for cap in &[
            AgentCapability::Chat,
            AgentCapability::TaskManagement,
            AgentCapability::CodeReview,
        ] {
            let bytes = postcard::to_allocvec(cap).expect("serialize");
            let decoded: AgentCapability = postcard::from_bytes(&bytes).expect("deserialize");
            assert_eq!(*cap, decoded);
        }
    }

    #[test]
    fn agent_info_unicode_display_name() {
        let info = AgentInfo {
            agent_id: "unicode-agent".to_string(),
            display_name: "エージェント 🤖".to_string(),
            capabilities: vec![AgentCapability::Chat],
        };
        let bytes = postcard::to_allocvec(&info).expect("serialize");
        let decoded: AgentInfo = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(info, decoded);
    }
}
