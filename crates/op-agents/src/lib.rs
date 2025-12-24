//! op-agents: Agent Registry and Management
//!
//! Provides agent lifecycle management and HTTP router.

pub mod agent_registry;
pub mod agent_catalog;
pub mod agents;
pub mod security;
pub mod dbus_service;
pub mod router;

// Re-export main types
pub use agent_registry::{AgentRegistry, AgentStatus};
pub use agent_catalog::{AgentDescriptor, builtin_agent_descriptors};
pub use router::{create_router, AgentsServiceRouter, AgentsState};

/// List available agent types
pub fn list_agent_types() -> Vec<String> {
    builtin_agent_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.agent_type)
        .collect()
}
