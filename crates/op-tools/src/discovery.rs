//! Tool discovery for finding and loading external tools

use super::Tool;
use op_core::ToolDefinition;
use std::path::Path;
use tracing::{info, warn};

/// Tool discovery configuration
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub search_paths: Vec<String>,
    pub recursive_search: bool,
    pub plugin_extensions: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            search_paths: vec![
                "./tools".to_string(),
                "./plugins".to_string(),
                "/usr/local/lib/op-dbus/tools".to_string(),
            ],
            recursive_search: true,
            plugin_extensions: vec!["tool".to_string(), "plugin".to_string(), "dll".to_string(), "so".to_string()],
        }
    }
}

/// Tool discovery system
pub struct ToolDiscovery {
    config: DiscoveryConfig,
}

impl ToolDiscovery {
    /// Create a new tool discovery system
    pub fn new() -> Self {
        Self {
            config: DiscoveryConfig::default(),
        }
    }

    /// Create a disabled tool discovery system
    pub fn disabled() -> Self {
        Self {
            config: DiscoveryConfig {
                enabled: false,
                ..Default::default()
            },
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: DiscoveryConfig) -> Self {
        Self { config }
    }

    /// Discover tools from configured paths
    pub async fn discover_tools(&self) -> anyhow::Result<Vec<Box<dyn Tool>>> {
        if !self.config.enabled {
            info!("Tool discovery is disabled");
            return Ok(vec![]);
        }

        let mut discovered_tools = Vec::new();

        for search_path in &self.config.search_paths {
            info!("Searching for tools in: {}", search_path);
            let tools = self.discover_tools_in_path(search_path).await?;
            discovered_tools.extend(tools);
        }

        info!("Discovered {} external tools", discovered_tools.len());
        Ok(discovered_tools)
    }

    /// Discover tools in a specific directory
    async fn discover_tools_in_path(&self, path: &str) -> anyhow::Result<Vec<Box<dyn Tool>>> {
        let path = Path::new(path);
        
        if !path.exists() {
            warn!("Discovery path does not exist: {}", path.display());
            return Ok(vec![]);
        }

        if !path.is_dir() {
            warn!("Discovery path is not a directory: {}", path.display());
            return Ok(vec![]);
        }

        let mut tools = Vec::new();

        if self.config.recursive_search {
            // Recursive search through directory tree
            for entry in walkdir::WalkDir::new(path) {
                match entry {
                    Ok(entry) => {
                        if entry.file_type().is_file() {
                            if let Some(tool) = self.load_tool_from_file(entry.path()).await {
                                tools.push(tool);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error accessing directory entry: {}", e);
                    }
                }
            }
        } else {
            // Non-recursive search in current directory
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_file() {
                    if let Some(tool) = self.load_tool_from_file(&path).await {
                        tools.push(tool);
                    }
                }
            }
        }

        Ok(tools)
    }

    /// Load a tool from a file
    async fn load_tool_from_file(&self, file_path: &Path) -> Option<Box<dyn Tool>> {
        // Check if file has a supported extension
        if let Some(extension) = file_path.extension().and_then(|ext| ext.to_str()) {
            if !self.config.plugin_extensions.iter().any(|ext| ext == extension) {
                return None;
            }
        } else {
            return None;
        }

        // For now, we'll implement a simple file-based tool loader
        // In a real implementation, you'd have different loaders for different plugin types
        match self.load_simple_tool(file_path).await {
            Some(tool) => {
                info!("Loaded tool from file: {}", file_path.display());
                Some(tool)
            }
            None => {
                warn!("Failed to load tool from file: {}", file_path.display());
                None
            }
        }
    }

    /// Load a simple tool from a configuration file
    async fn load_simple_tool(&self, file_path: &Path) -> Option<Box<dyn Tool>> {
        // For demo purposes, we'll try to load tools from JSON config files
        if file_path.extension().and_then(|e| e.to_str()) == Some("json") {
            match std::fs::read_to_string(file_path) {
                Ok(content) => {
                    match serde_json::from_str::<SimpleToolConfig>(&content) {
                        Ok(config) => Some(Box::new(SimpleTool::new(config))),
                        Err(e) => {
                            warn!("Invalid tool configuration in {}: {}", file_path.display(), e);
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read tool file {}: {}", file_path.display(), e);
                    None
                }
            }
        } else {
            // For other file types, we could implement different loaders
            // For now, just return None
            None
        }
    }
}

/// Simple tool loaded from configuration
struct SimpleTool {
    config: SimpleToolConfig,
}

impl SimpleTool {
    fn new(config: SimpleToolConfig) -> Self {
        Self { config }
    }
}

impl Tool for SimpleTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.config.name.clone(),
            description: self.config.description.clone(),
            input_schema: self.config.input_schema.clone(),
            category: self.config.category.clone(),
            tags: self.config.tags.clone(),
            security_level: self.config.security_level.clone().unwrap_or(op_core::SecurityLevel::Low),
        }
    }

    async fn execute(&self, request: op_core::ToolRequest) -> op_core::ToolResult {
        let execution_id = uuid::Uuid::new_v4();
        let start_time = std::time::Instant::now();

        // Simple tool execution - just echo back a formatted result
        let result = op_core::ToolResult {
            success: true,
            content: serde_json::json!({
                "tool_name": self.config.name,
                "executed_at": chrono::Utc::now().to_rfc3339(),
                "input": request.arguments,
                "output": format!("Executed simple tool: {}", self.config.name)
            }),
            duration_ms: start_time.elapsed().as_millis() as u64,
            execution_id,
        };

        result
    }
}

/// Configuration for simple tools loaded from files
#[derive(Debug, Clone, serde::Deserialize)]
struct SimpleToolConfig {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    category: String,
    tags: Vec<String>,
    security_level: Option<op_core::SecurityLevel>,
    command: Option<String>,
    args: Option<Vec<String>>,
}

/// Tool source for discovering tools from different sources
#[derive(Debug, Clone)]
pub enum ToolSource {
    Directory { path: String, recursive: bool },
    File { path: String },
    Url { url: String },
    Registry { name: String },
}

/// Tool discovery result
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub source: ToolSource,
    pub tools_found: usize,
    pub tools_loaded: usize,
    pub errors: Vec<String>,
}

impl DiscoveryResult {
    /// Create a new discovery result
    pub fn new(source: ToolSource) -> Self {
        Self {
            source,
            tools_found: 0,
            tools_loaded: 0,
            errors: Vec::new(),
        }
    }

    /// Add an error to the result
    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }
}

/// Tool discovery statistics
#[derive(Debug, Clone)]
pub struct DiscoveryStats {
    pub total_sources: usize,
    pub successful_sources: usize,
    pub failed_sources: usize,
    pub total_tools_found: usize,
    pub total_tools_loaded: usize,
    pub discovery_time_ms: u64,
}

impl Default for DiscoveryStats {
    fn default() -> Self {
        Self {
            total_sources: 0,
            successful_sources: 0,
            failed_sources: 0,
            total_tools_found: 0,
            total_tools_loaded: 0,
            discovery_time_ms: 0,
        }
    }
}