//! Tool Registry with LRU Caching and Lazy Loading Support
//!
//! Provides a registry for tools with:
//! - LRU eviction for memory management
//! - Lazy loading via ToolFactory trait
//! - Usage tracking and statistics

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::tool::BoxedTool;

/// Tool definition metadata (without the actual tool implementation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub category: String,
    pub tags: Vec<String>,
    #[serde(default = "default_namespace")]
    pub namespace: String,
}

fn default_namespace() -> String {
    "system".to_string()
}

/// Factory trait for lazy tool creation
#[async_trait]
pub trait ToolFactory: Send + Sync {
    /// Get the tool name this factory creates
    fn tool_name(&self) -> &str;

    /// Get the tool definition (metadata only, no loading)
    fn definition(&self) -> ToolDefinition;

    /// Create the actual tool instance (may be expensive)
    async fn create(&self) -> Result<BoxedTool>;

    /// Estimated memory cost of the tool
    fn memory_cost(&self) -> usize {
        1
    }
}

/// A registered tool with usage tracking
struct RegisteredTool {
    tool: BoxedTool,
    definition: ToolDefinition,
    last_used: RwLock<Instant>,
    use_count: AtomicU64,
    // loaded_at: Instant,
}

impl RegisteredTool {
    fn new(tool: BoxedTool, definition: ToolDefinition) -> Self {
        Self {
            tool,
            definition,
            last_used: RwLock::new(Instant::now()),
            use_count: AtomicU64::new(0),
            // loaded_at: Instant::now(),
        }
    }

    async fn touch(&self) {
        *self.last_used.write().await = Instant::now();
        self.use_count.fetch_add(1, Ordering::Relaxed);
    }

    async fn idle_time(&self) -> Duration {
        self.last_used.read().await.elapsed()
    }
}

/// LRU configuration
#[derive(Debug, Clone)]
pub struct LruConfig {
    /// Maximum number of loaded tools
    pub max_loaded_tools: usize,
    /// Minimum idle time before a tool can be evicted
    pub min_idle_time: Duration,
    /// Number of uses that makes a tool "hot" (won't be evicted)
    pub hot_threshold: u64,
    /// How often to check for eviction (in terms of operations)
    pub eviction_check_interval: usize,
}

impl Default for LruConfig {
    fn default() -> Self {
        Self {
            max_loaded_tools: 500,
            min_idle_time: Duration::from_secs(300),
            hot_threshold: 10,
            eviction_check_interval: 10,
        }
    }
}

/// Statistics about the registry
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_registered: usize,
    pub currently_loaded: usize,
    pub total_calls: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
}

/// Tool Registry with LRU caching
pub struct ToolRegistry {
    /// Currently loaded tools
    tools: RwLock<HashMap<Arc<str>, Arc<RegisteredTool>>>,
    /// Tool factories for lazy loading
    factories: RwLock<HashMap<Arc<str>, Arc<dyn ToolFactory>>>,
    /// Tool definitions (always available, even if tool not loaded)
    definitions: RwLock<HashMap<Arc<str>, ToolDefinition>>,
    /// LRU configuration
    config: LruConfig,
    /// Statistics
    stats: RwLock<RegistryStats>,
    /// Operation counter for eviction checks
    op_counter: AtomicU64,
}

impl ToolRegistry {
    /// Create a new registry with default config
    pub fn new() -> Self {
        Self::with_config(LruConfig::default())
    }

    /// Create a new registry with custom config
    pub fn with_config(config: LruConfig) -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            factories: RwLock::new(HashMap::new()),
            definitions: RwLock::new(HashMap::new()),
            config,
            stats: RwLock::new(RegistryStats::default()),
            op_counter: AtomicU64::new(0),
        }
    }

    /// Register a tool with its definition
    pub async fn register(
        &self,
        name: Arc<str>,
        tool: BoxedTool,
        definition: ToolDefinition,
    ) -> Result<()> {
        let registered = Arc::new(RegisteredTool::new(tool, definition.clone()));

        {
            let mut tools = self.tools.write().await;
            let mut definitions = self.definitions.write().await;

            tools.insert(name.clone(), registered);
            definitions.insert(name.clone(), definition);
        }

        {
            let mut stats = self.stats.write().await;
            stats.total_registered += 1;
            stats.currently_loaded += 1;
        }

        debug!("Registered tool: {}", name);
        Ok(())
    }

    /// Helper to register a tool instance directly
    pub async fn register_tool(&self, tool: BoxedTool) -> Result<()> {
        let definition = ToolDefinition {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
            category: "builtin".to_string(),
            tags: vec!["builtin".to_string()],
            namespace: tool.namespace().to_string(),
        };
        self.register(Arc::from(tool.name()), tool, definition).await
    }

    /// Register a factory for lazy loading
    pub async fn register_factory(&self, factory: Arc<dyn ToolFactory>) -> Result<()> {
        let name: Arc<str> = Arc::from(factory.tool_name());
        let definition = factory.definition();

        {
            let mut factories = self.factories.write().await;
            let mut definitions = self.definitions.write().await;

            factories.insert(name.clone(), factory);
            definitions.insert(name.clone(), definition);
        }

        {
            let mut stats = self.stats.write().await;
            stats.total_registered += 1;
        }

        debug!("Registered factory for tool: {}", name);
        Ok(())
    }

    /// Get a tool by name (loads if necessary)
    pub async fn get(&self, name: &str) -> Option<BoxedTool> {
        self.increment_op_counter().await;

        // Check if already loaded
        {
            let tools = self.tools.read().await;
            if let Some(registered) = tools.get(name) {
                registered.touch().await;
                let mut stats = self.stats.write().await;
                stats.cache_hits += 1;
                stats.total_calls += 1;
                return Some(registered.tool.clone());
            }
        }

        // Try to load from factory
        let factory = {
            let factories = self.factories.read().await;
            factories.get(name).cloned()
        };

        if let Some(factory) = factory {
            match factory.create().await {
                Ok(tool) => {
                    let definition = factory.definition();
                    let name_arc: Arc<str> = Arc::from(name);

                    // Register the loaded tool
                    let registered = Arc::new(RegisteredTool::new(tool.clone(), definition));

                    {
                        let mut tools = self.tools.write().await;
                        tools.insert(name_arc, registered);
                    }

                    {
                        let mut stats = self.stats.write().await;
                        stats.cache_misses += 1;
                        stats.total_calls += 1;
                        stats.currently_loaded += 1;
                    }

                    // Check if eviction needed
                    self.maybe_evict().await;

                    return Some(tool);
                }
                Err(e) => {
                    warn!("Failed to create tool {}: {}", name, e);
                }
            }
        }

        None
    }

    /// Get tool definition (without loading the tool)
    pub async fn get_definition(&self, name: &str) -> Option<ToolDefinition> {
        let definitions = self.definitions.read().await;
        definitions.get(name).cloned()
    }

    /// List all registered tool definitions
    pub async fn list(&self) -> Vec<ToolDefinition> {
        let definitions = self.definitions.read().await;
        definitions.values().cloned().collect()
    }

    /// List only currently loaded tools
    pub async fn list_loaded(&self) -> Vec<ToolDefinition> {
        let tools = self.tools.read().await;
        tools.values().map(|t| t.definition.clone()).collect()
    }

    /// Get registry statistics
    pub async fn stats(&self) -> RegistryStats {
        self.stats.read().await.clone()
    }

    /// Check if a tool is currently loaded
    pub async fn is_loaded(&self, name: &str) -> bool {
        let tools = self.tools.read().await;
        tools.contains_key(name)
    }

    /// Unload a specific tool
    pub async fn unload(&self, name: &str) -> bool {
        let mut tools = self.tools.write().await;
        if tools.remove(name).is_some() {
            let mut stats = self.stats.write().await;
            stats.currently_loaded -= 1;
            debug!("Unloaded tool: {}", name);
            true
        } else {
            false
        }
    }

    /// Increment operation counter and maybe trigger eviction
    async fn increment_op_counter(&self) {
        let count = self.op_counter.fetch_add(1, Ordering::Relaxed);
        if count.is_multiple_of(self.config.eviction_check_interval as u64) {
            self.maybe_evict().await;
        }
    }

    /// Evict tools if over capacity
    async fn maybe_evict(&self) {
        let tools = self.tools.read().await;
        if tools.len() <= self.config.max_loaded_tools {
            return;
        }
        drop(tools);

        // Find candidates for eviction
        let mut candidates: Vec<(Arc<str>, Duration, u64)> = Vec::new();

        {
            let tools = self.tools.read().await;
            for (name, registered) in tools.iter() {
                let idle_time = registered.idle_time().await;
                let use_count = registered.use_count.load(Ordering::Relaxed);

                // Don't evict hot tools or recently used tools
                if use_count < self.config.hot_threshold
                    && idle_time > self.config.min_idle_time
                {
                    candidates.push((name.clone(), idle_time, use_count));
                }
            }
        }

        // Sort by idle time (longest idle first)
        candidates.sort_by(|a, b| b.1.cmp(&a.1));

        // Evict until under capacity
        let to_evict = candidates
            .iter()
            .take(candidates.len().saturating_sub(self.config.max_loaded_tools))
            .map(|(name, _, _)| name.clone())
            .collect::<Vec<_>>();

        if !to_evict.is_empty() {
            let mut tools = self.tools.write().await;
            let mut stats = self.stats.write().await;

            for name in to_evict {
                if tools.remove(&name).is_some() {
                    stats.currently_loaded -= 1;
                    stats.evictions += 1;
                    info!("Evicted tool: {}", name);
                }
            }
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;

    struct TestTool {
        name: String,
    }

    #[async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Test tool"
        }

        fn input_schema(&self) -> Value {
            serde_json::json!({})
        }

        async fn execute(&self, _input: Value) -> Result<Value> {
            Ok(serde_json::json!({"result": "ok"}))
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ToolRegistry::new();
        let tool: BoxedTool = Arc::new(TestTool {
            name: "test".to_string(),
        });
        let definition = ToolDefinition {
            name: "test".to_string(),
            description: "Test".to_string(),
            input_schema: serde_json::json!({}),
            category: "test".to_string(),
            tags: vec![],
            namespace: "test".to_string(),
        };

        registry
            .register(Arc::from("test"), tool, definition)
            .await
            .unwrap();

        let retrieved = registry.get("test").await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_list_definitions() {
        let registry = ToolRegistry::new();
        let tool: BoxedTool = Arc::new(TestTool {
            name: "test".to_string(),
        });
        let definition = ToolDefinition {
            name: "test".to_string(),
            description: "Test".to_string(),
            input_schema: serde_json::json!({}),
            category: "test".to_string(),
            tags: vec![],
            namespace: "test".to_string(),
        };

        registry
            .register(Arc::from("test"), tool, definition)
            .await
            .unwrap();

        let definitions = registry.list().await;
        assert_eq!(definitions.len(), 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let registry = ToolRegistry::new();
        let tool: BoxedTool = Arc::new(TestTool {
            name: "test".to_string(),
        });
        let definition = ToolDefinition {
            name: "test".to_string(),
            description: "Test".to_string(),
            input_schema: serde_json::json!({}),
            category: "test".to_string(),
            tags: vec![],
            namespace: "test".to_string(),
        };

        registry
            .register(Arc::from("test"), tool, definition)
            .await
            .unwrap();

        // Access the tool
        registry.get("test").await;
        registry.get("test").await;

        let stats = registry.stats().await;
        assert_eq!(stats.total_registered, 1);
        assert_eq!(stats.cache_hits, 2);
    }
}
