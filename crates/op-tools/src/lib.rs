//! op-tools: Tool registry and execution engine
//!
//! This crate provides the core tool management system including
//! registry, discovery, execution, and middleware support.

pub mod registry;
pub mod executor;
pub mod middleware;
pub mod builtin;
pub mod discovery;

// Re-export main types
pub use registry::ToolRegistry;
pub use registry::ToolRegistryImpl;
pub use executor::ToolExecutor;
pub use executor::ToolExecutorImpl;
pub use middleware::{ToolMiddleware, LoggingMiddleware, TimingMiddleware};
pub use builtin::register_builtin_tools;
pub use discovery::ToolDiscovery;

// Re-export core types for convenience
pub use op_core::prelude::{Tool, ToolDefinition, ToolRequest, ToolResult, SecurityLevel};

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Main tool system that combines registry, executor, and discovery
pub struct ToolSystem {
    registry: Arc<RwLock<dyn op_core::prelude::ToolRegistry>>,
    executor: Arc<dyn ToolExecutor>,
    discovery: ToolDiscovery,
}

impl ToolSystem {
    /// Create a new tool system
    pub fn new(
        registry: Arc<RwLock<dyn op_core::prelude::ToolRegistry>>,
        executor: Arc<dyn ToolExecutor>,
        discovery: ToolDiscovery,
    ) -> Self {
        Self {
            registry,
            executor,
            discovery,
        }
    }

    /// Initialize the tool system with built-in tools
    pub async fn initialize_with_builtins(&self) -> anyhow::Result<()> {
        info!("Initializing tool system with built-in tools");
        
        // Register built-in tools
        let builtin_tools = register_builtin_tools();
        let mut registry = self.registry.write().await;
        
        for tool in builtin_tools {
            registry.register_tool(Box::new(tool)).await?;
        }
        
        info!("Registered {} built-in tools", builtin_tools.len());
        
        // Discover additional tools
        let discovered_tools = self.discovery.discover_tools().await?;
        for tool in discovered_tools {
            registry.register_tool(tool).await?;
        }
        
        info!("Discovered and registered {} additional tools", discovered_tools.len());
        
        Ok(())
    }

    /// Get the tool registry
    pub fn registry(&self) -> &Arc<RwLock<dyn op_core::prelude::ToolRegistry>> {
        &self.registry
    }

    /// Get the tool executor
    pub fn executor(&self) -> &Arc<dyn ToolExecutor> {
        &self.executor
    }
}

/// Builder for configuring tool system
pub struct ToolSystemBuilder {
    registry: Option<Arc<RwLock<dyn op_core::prelude::ToolRegistry>>>,
    executor: Option<Arc<dyn ToolExecutor>>,
    middleware: Vec<Arc<dyn ToolMiddleware>>,
    enable_builtin_tools: bool,
    enable_discovery: bool,
}

impl ToolSystemBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            registry: None,
            executor: None,
            middleware: vec![],
            enable_builtin_tools: true,
            enable_discovery: true,
        }
    }

    /// Set the tool registry
    pub fn registry(mut self, registry: Arc<RwLock<dyn op_core::prelude::ToolRegistry>>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Set the tool executor
    pub fn executor(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Add middleware
    pub fn middleware(mut self, middleware: Arc<dyn ToolMiddleware>) -> Self {
        self.middleware.push(middleware);
        self
    }

    /// Enable or disable built-in tools
    pub fn builtin_tools(mut self, enable: bool) -> Self {
        self.enable_builtin_tools = enable;
        self
    }

    /// Enable or disable tool discovery
    pub fn tool_discovery(mut self, enable: bool) -> Self {
        self.enable_discovery = enable;
        self
    }

    /// Build the tool system
    pub async fn build(self) -> anyhow::Result<ToolSystem> {
        let registry = self.registry.unwrap_or_else(|| {
            Arc::new(RwLock::new(ToolRegistryImpl::new()))
        });
        
        let executor = self.executor.unwrap_or_else(|| {
            Arc::new(ToolExecutorImpl::new(self.middleware))
        });

        let discovery = if self.enable_discovery {
            ToolDiscovery::new()
        } else {
            ToolDiscovery::disabled()
        };

        let system = ToolSystem::new(registry, executor, discovery);
        
        if self.enable_builtin_tools {
            system.initialize_with_builtins().await?;
        }
        
        Ok(system)
    }
}

impl Default for ToolSystemBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{
        ToolSystem, ToolSystemBuilder, ToolRegistry, ToolExecutor,
        register_builtin_tools, ToolDiscovery
    };
    pub use op_core::prelude::{Tool, ToolDefinition, ToolRequest, ToolResult, SecurityLevel};
}