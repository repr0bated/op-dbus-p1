//! Discovery Sources
//!
//! Implementations of ToolDiscoverySource for various backends:
//! - D-Bus runtime introspection
//! - Plugin registry scanning
//! - Agent registry scanning

mod dbus;
mod plugin;
mod agent;

pub use dbus::DbusDiscoverySource;
pub use plugin::PluginDiscoverySource;
pub use agent::AgentDiscoverySource;
