//! Embedded MCP resources - documentation and agent definitions
//!
//! This module embeds markdown documentation files directly into the MCP server binary,
//! making them available via the MCP resources protocol without requiring external files.
//!
//! Comprehensive agents (148+ files) are bundled using rust-embed for efficient
//! memory-mapped access and portability without requiring recompilation.

use crate::embedded_agents;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
    pub content: String,
}

/// Registry of embedded resources
pub struct ResourceRegistry {
    resources: HashMap<String, Resource>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        let mut resources = HashMap::new();

        // Embed agent documentation and specifications
        resources.insert(
            "agent://agents/overview".to_string(),
            Resource {
                uri: "agent://agents/overview".to_string(),
                name: "Agent System Overview".to_string(),
                description: "Complete overview of the agent-based architecture and guidelines"
                    .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../AGENTS.md").to_string(),
            },
        );

        // Individual agent specifications
        resources.insert(
            "agent://spec/executor".to_string(),
            Resource {
                uri: "agent://spec/executor".to_string(),
                name: "Executor Agent Specification".to_string(),
                description: "Secure command execution agent with whitelist-based security"
                    .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-EXECUTOR.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/systemd".to_string(),
            Resource {
                uri: "agent://spec/systemd".to_string(),
                name: "Systemd Agent Specification".to_string(),
                description: "systemd service management agent via systemctl".to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-SYSTEMD.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/network".to_string(),
            Resource {
                uri: "agent://spec/network".to_string(),
                name: "Network Agent Specification".to_string(),
                description: "Network diagnostics and information gathering agent".to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-NETWORK.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/file".to_string(),
            Resource {
                uri: "agent://spec/file".to_string(),
                name: "File Agent Specification".to_string(),
                description: "Secure file operations agent with path validation".to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-FILE.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/monitor".to_string(),
            Resource {
                uri: "agent://spec/monitor".to_string(),
                name: "Monitor Agent Specification".to_string(),
                description: "System monitoring and metrics collection agent".to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-MONITOR.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/packagekit".to_string(),
            Resource {
                uri: "agent://spec/packagekit".to_string(),
                name: "PackageKit Agent Specification".to_string(),
                description: "Package management agent via D-Bus PackageKit interface".to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-PACKAGEKIT.md").to_string(),
            },
        );

        // Memory and context management agents
        resources.insert(
            "agent://spec/memory-graph".to_string(),
            Resource {
                uri: "agent://spec/memory-graph".to_string(),
                name: "Knowledge Graph Memory Agent".to_string(),
                description: "Persistent memory using knowledge graph with entities, relations, and observations"
                    .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-MEMORY-GRAPH.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/memory-vector".to_string(),
            Resource {
                uri: "agent://spec/memory-vector".to_string(),
                name: "Vector Memory Agent".to_string(),
                description:
                    "Semantic memory storage and retrieval using vector embeddings and Qdrant"
                        .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-MEMORY-VECTOR.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/memory-buffer".to_string(),
            Resource {
                uri: "agent://spec/memory-buffer".to_string(),
                name: "Conversation Buffer Memory Agent".to_string(),
                description: "Multiple conversation memory strategies: buffer, window, summary, and hybrid modes"
                    .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-MEMORY-BUFFER.md").to_string(),
            },
        );

        // Utility agents
        resources.insert(
            "agent://spec/code-sandbox".to_string(),
            Resource {
                uri: "agent://spec/code-sandbox".to_string(),
                name: "Code Sandbox Agent".to_string(),
                description:
                    "Secure sandboxed code execution for Python and JavaScript with resource limits"
                        .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-CODE-SANDBOX.md").to_string(),
            },
        );

        resources.insert(
            "agent://spec/web-scraper".to_string(),
            Resource {
                uri: "agent://spec/web-scraper".to_string(),
                name: "Web Scraper Agent".to_string(),
                description: "Browser automation and web scraping with structured data extraction via Playwright"
                    .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../agents/AGENT-WEB-SCRAPER.md").to_string(),
            },
        );

        // Load comprehensive agents from embedded resources (148+ total)
        // Uses rust-embed for efficient bundling without recompilation overhead
        for (uri, resource) in embedded_agents::load_comprehensive_agents() {
            resources.insert(uri, resource);
        }

        // MCP Protocol specification
        resources.insert(
            "spec://mcp/protocol".to_string(),
            Resource {
                uri: "spec://mcp/protocol".to_string(),
                name: "MCP Protocol Specification".to_string(),
                description: "Model Context Protocol (MCP) 2024-11-05 specification reference"
                    .to_string(),
                mime_type: "text/markdown".to_string(),
                content: include_str!("../../docs/MCP-PROTOCOL-SPEC.md").to_string(),
            },
        );

        Self { resources }
    }

    /// List all available resources
    pub fn list_resources(&self) -> Vec<&Resource> {
        self.resources.values().collect()
    }

    /// Get a specific resource by URI
    pub fn get_resource(&self, uri: &str) -> Option<&Resource> {
        self.resources.get(uri)
    }

    /// Get all resource URIs
    pub fn get_resource_uris(&self) -> Vec<String> {
        self.resources.keys().cloned().collect()
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
