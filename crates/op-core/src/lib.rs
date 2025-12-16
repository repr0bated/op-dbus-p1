//! op-core: Foundation types and traits for op-dbus-v2
//!
//! This crate provides the core abstractions and types used throughout the
//! op-dbus-v2 system. It defines the fundamental contracts that other crates
//! build upon.

pub mod error;
pub mod types;
pub mod traits;

// Re-export main types for convenient imports
pub use error::{CoreError, Result};
pub use types::*;

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{CoreError, Result, ToolDefinition, ToolRequest, ToolResult};
    pub use super::traits::{Tool, ToolRegistry, DbusIntrospector};
    pub use super::types::*;
}