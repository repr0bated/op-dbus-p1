//! Resource Registry for MCP
//!
//! Provides embedded documentation resources that can be served via MCP resources.
//! This is a simple placeholder that can be extended with actual documentation.

use serde::{Deserialize, Serialize};

/// Resource information for MCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

/// Simple resource registry
pub struct ResourceRegistry {
    resources: Vec<ResourceInfo>,
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceRegistry {
    /// Create new empty resource registry
    pub fn new() -> Self {
        Self {
            resources: Vec::new(),
        }
    }

    /// Add a resource to the registry
    pub fn add_resource(&mut self, resource: ResourceInfo) {
        self.resources.push(resource);
    }

    /// List all available resources
    pub fn list_resources(&self) -> &[ResourceInfo] {
        &self.resources
    }

    /// Get a resource by URI (placeholder implementation)
    pub fn get_resource(&self, uri: &str) -> Option<&ResourceInfo> {
        self.resources.iter().find(|r| r.uri == uri)
    }

    /// Read resource content (placeholder implementation)
    pub fn read_resource(&self, uri: &str) -> Option<String> {
        // Placeholder - return simple info about the resource
        self.get_resource(uri).map(|resource| {
            format!(
                "# {}\n\n{}\n\nThis is a placeholder resource. URI: {}\n",
                resource.name,
                resource.description.as_deref().unwrap_or("No description available"),
                resource.uri
            )
        })
    }
}
