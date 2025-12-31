//! op-state: State Management System
//!
//! Provides:
//! - StatePlugin trait for pluggable state management
//! - State manager for coordinating plugins
//! - Crypto utilities for state hashing/signing
//! - Schema validation
//! - Plugin tree for hierarchical state
//! - Auto-plugin generation

pub mod authority;
// pub mod auto_plugin;
pub mod crypto;
pub mod dbus_plugin_base;
pub mod dbus_server;
pub mod manager;
pub mod plugin;
pub mod plugin_workflow;
pub mod plugtree;
pub mod schema_validator;

pub use manager::{StateManager, FootprintSender};
pub use plugin::{
    ApplyResult, Checkpoint, DiffMetadata, PluginCapabilities, StateAction, StateDiff, StatePlugin,
};
pub use plugtree::PlugTree;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::manager::StateManager;
    pub use super::plugin::{
        ApplyResult, Checkpoint, DiffMetadata, PluginCapabilities, StateAction, StateDiff,
        StatePlugin,
    };
    pub use super::plugtree::PlugTree;
}
