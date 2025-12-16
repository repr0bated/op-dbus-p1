//! Resource registry for MCP documentation

/// Resource registry for embedded documentation
pub struct ResourceRegistry {
    // Minimal implementation - can be extended with actual docs
}

impl ResourceRegistry {
    /// Create new resource registry
    pub fn new() -> Self {
        Self {}
    }

    /// List all available resources
    pub fn list_resources(&self) -> Vec<Resource> {
        // Return empty for now - can be extended with embedded docs
        vec![]
    }

    /// Get a resource by URI
    pub fn get_resource(&self, _uri: &str) -> Option<Resource> {
        // Not implemented in minimal version
        None
    }
}

/// Resource representation
#[derive(Debug, Clone)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

impl Resource {
    /// Create a new resource
    pub fn new(uri: impl Into<String>, name: impl Into<String>, description: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: description.into(),
            mime_type: mime_type.into(),
        }
    }
}