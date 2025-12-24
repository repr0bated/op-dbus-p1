//! OP Dynamic Loader - Intelligent Tool Loading Enhancement
//!
//! Complements existing MCP tool loading by adding:
//! - LRU caching for frequently used tools
//! - Execution-aware loading decisions
//! - Integration with execution tracking
//! - Memory-efficient tool management

pub mod error;
pub mod loading_strategy;
pub mod dynamic_registry;
pub mod execution_aware_loader;

pub use error::DynamicLoaderError;
pub use loading_strategy::{LoadingStrategy, SmartLoadingStrategy};
pub use dynamic_registry::DynamicToolRegistry;
pub use execution_aware_loader::ExecutionAwareLoader;