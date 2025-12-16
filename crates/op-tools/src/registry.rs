//! Tool registry for managing tool registration and discovery

use op_core::{Tool, ToolDefinition, ToolRegistry, ToolRequest, ToolResult, SecurityLevel};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// In-memory tool registry implementation
pub struct ToolRegistryImpl {
    tools: HashMap<String, Arc<dyn Tool>>,
    tools_by_category: HashMap<String, Vec<String>>,
}

impl ToolRegistryImpl {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            tools_by_category: HashMap::new(),
        }
    }

    /// Register a tool
    pub async fn register_tool(&mut self, tool: Box<dyn Tool>) -> anyhow::Result<()> {
        let definition = tool.definition();
        let name = definition.name.clone();
        
        // Check if tool already exists
        if self.tools.contains_key(&name) {
            return Err(anyhow::anyhow!("Tool '{}' already registered", name));
        }

        // Register the tool
        self.tools.insert(name.clone(), Arc::from(tool));
        
        // Add to category index
        self.tools_by_category
            .entry(definition.category.clone())
            .or_insert_with(Vec::new)
            .push(name);
        
        info!("Registered tool: {} (category: {})", name, definition.category);
        Ok(())
    }

    /// Unregister a tool by name
    pub async fn unregister_tool(&mut self, name: &str) -> anyhow::Result<()> {
        if let Some(tool) = self.tools.remove(name) {
            // Remove from category index
            if let Some(tool_def) = tool.definition_opt() {
                if let Some(category_tools) = self.tools_by_category.get_mut(&tool_def.category) {
                    category_tools.retain(|tool_name| tool_name != name);
                    if category_tools.is_empty() {
                        self.tools_by_category.remove(&tool_def.category);
                    }
                }
            }
            
            info!("Unregistered tool: {}", name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Tool '{}' not found", name))
        }
    }

    /// Get a tool by name
    pub async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tools
    pub async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.values()
            .map(|tool| tool.definition())
            .collect()
    }

    /// Get tools by category
    pub async fn get_tools_by_category(&self, category: &str) -> Vec<ToolDefinition> {
        if let Some(tool_names) = self.tools_by_category.get(category) {
            tool_names.iter()
                .filter_map(|name| self.tools.get(name))
                .map(|tool| tool.definition())
                .collect()
        } else {
            vec![]
        }
    }

    /// Get tool statistics
    pub async fn get_stats(&self) -> RegistryStats {
        let total_tools = self.tools.len();
        let categories = self.tools_by_category.len();
        let tools_by_security: HashMap<String, usize> = self.tools.values()
            .map(|tool| tool.definition())
            .fold(HashMap::new(), |mut acc, def| {
                *acc.entry(format!("{:?}", def.security_level)).or_insert(0) += 1;
                acc
            });

        RegistryStats {
            total_tools,
            categories,
            tools_by_security,
        }
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_tools: usize,
    pub categories: usize,
    pub tools_by_security: HashMap<String, usize>,
}

/// Tool wrapper that provides optional access to definition
pub struct ToolWrapper {
    tool: Arc<dyn Tool>,
}

impl ToolWrapper {
    /// Create a new tool wrapper
    pub fn new(tool: Arc<dyn Tool>) -> Self {
        Self { tool }
    }

    /// Get the tool definition
    pub fn definition(&self) -> ToolDefinition {
        self.tool.definition()
    }

    /// Get the tool definition if available
    pub fn definition_opt(&self) -> Option<ToolDefinition> {
        Some(self.definition())
    }
}

// Implement the ToolRegistry trait for ToolRegistryImpl
#[async_trait::async_trait]
impl ToolRegistry for ToolRegistryImpl {
    async fn register_tool(&self, tool: Box<dyn Tool>) -> anyhow::Result<()> {
        let mut registry = self.tools_write().await;
        registry.register_tool(tool).await
    }

    async fn unregister_tool(&self, name: &str) -> anyhow::Result<()> {
        let mut registry = self.tools_write().await;
        registry.unregister_tool(name).await
    }

    async fn get_tool(&self, name: &str) -> Option<Box<dyn Tool>> {
        let registry = self.tools_read().await;
        registry.get_tool(name).map(|arc_tool| {
            // Clone the Arc to get a new Box
            let tool_clone: Arc<dyn Tool> = Arc::clone(&arc_tool);
            Box::new(ToolWrapper::new(tool_clone)) as Box<dyn Tool>
        })
    }

    async fn list_tools(&self) -> Vec<ToolDefinition> {
        let registry = self.tools_read().await;
        registry.list_tools().await
    }

    async fn get_tools_by_category(&self, category: &str) -> Vec<ToolDefinition> {
        let registry = self.tools_read().await;
        registry.get_tools_by_category(category).await
    }
}

impl ToolRegistryImpl {
    async fn tools_read(&self) -> tokio::sync::RwLockReadGuard<'_, Self> {
        // This is a workaround since we can't use RwLock directly on self
        // In a real implementation, we'd use Arc<RwLock<ToolRegistryImpl>>
        unimplemented!("This method should be called on an Arc<RwLock<ToolRegistryImpl>>")
    }

    async fn tools_write(&self) -> tokio::sync::RwLockWriteGuard<'_, Self> {
        // This is a workaround since we can't use RwLock directly on self
        // In a real implementation, we'd use Arc<RwLock<ToolRegistryImpl>>
        unimplemented!("This method should be called on an Arc<RwLock<ToolRegistryImpl>>")
    }
}