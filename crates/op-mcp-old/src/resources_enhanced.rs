//! Enhanced MCP resources - combines embedded docs with runtime markdown scanning
//!
//! This module provides both:
//! 1. Embedded resources (compiled into binary for core documentation)
//! 2. Runtime-scanned resources (markdown files from /git/agents and /git/commands)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
    pub content: String,
}

/// Enhanced registry combining embedded and scanned resources
pub struct ResourceRegistry {
    resources: HashMap<String, Resource>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            resources: HashMap::new(),
        };

        // Load embedded resources
        registry.load_embedded_resources();

        // Scan and load markdown files from agents and commands repos
        registry.scan_markdown_files();

        registry
    }

    /// Load all embedded resources (existing functionality)
    fn load_embedded_resources(&mut self) {
        // Embed agent documentation and specifications
        self.resources.insert(
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
        self.add_embedded_resource(
            "agent://spec/executor",
            "Executor Agent Specification",
            "Secure command execution agent with whitelist-based security",
            include_str!("../../agents/AGENT-EXECUTOR.md"),
        );

        self.add_embedded_resource(
            "agent://spec/systemd",
            "Systemd Agent Specification",
            "systemd service management agent via systemctl",
            include_str!("../../agents/AGENT-SYSTEMD.md"),
        );

        self.add_embedded_resource(
            "agent://spec/network",
            "Network Agent Specification",
            "Network diagnostics and information gathering agent",
            include_str!("../../agents/AGENT-NETWORK.md"),
        );

        self.add_embedded_resource(
            "agent://spec/file",
            "File Agent Specification",
            "Secure file operations agent with path validation",
            include_str!("../../agents/AGENT-FILE.md"),
        );

        self.add_embedded_resource(
            "agent://spec/monitor",
            "Monitor Agent Specification",
            "System monitoring and metrics collection agent",
            include_str!("../../agents/AGENT-MONITOR.md"),
        );

        self.add_embedded_resource(
            "agent://spec/packagekit",
            "PackageKit Agent Specification",
            "Package management agent via D-Bus PackageKit interface",
            include_str!("../../agents/AGENT-PACKAGEKIT.md"),
        );

        // Memory and context management agents
        self.add_embedded_resource(
            "agent://spec/memory-graph",
            "Knowledge Graph Memory Agent",
            "Persistent memory using knowledge graph with entities, relations, and observations",
            include_str!("../../agents/AGENT-MEMORY-GRAPH.md"),
        );

        self.add_embedded_resource(
            "agent://spec/memory-vector",
            "Vector Memory Agent",
            "Semantic memory storage and retrieval using vector embeddings and Qdrant",
            include_str!("../../agents/AGENT-MEMORY-VECTOR.md"),
        );

        self.add_embedded_resource(
            "agent://spec/memory-buffer",
            "Conversation Buffer Memory Agent",
            "Multiple conversation memory strategies: buffer, window, summary, and hybrid modes",
            include_str!("../../agents/AGENT-MEMORY-BUFFER.md"),
        );

        // Utility agents
        self.add_embedded_resource(
            "agent://spec/code-sandbox",
            "Code Sandbox Agent",
            "Secure sandboxed code execution for Python and JavaScript with resource limits",
            include_str!("../../agents/AGENT-CODE-SANDBOX.md"),
        );

        self.add_embedded_resource(
            "agent://spec/web-scraper",
            "Web Scraper Agent",
            "Browser automation and web scraping with structured data extraction via Playwright",
            include_str!("../../agents/AGENT-WEB-SCRAPER.md"),
        );

        // MCP documentation
        self.add_embedded_resource(
            "mcp://docs/complete-guide",
            "MCP Complete Guide",
            "Complete guide to the Model Context Protocol integration",
            include_str!("../../docs/MCP-COMPLETE-GUIDE.md"),
        );

        self.add_embedded_resource(
            "mcp://docs/developer-guide",
            "MCP Developer Guide",
            "Developer guide for extending the MCP server",
            include_str!("../../docs/MCP-DEVELOPER-GUIDE.md"),
        );

        self.add_embedded_resource(
            "mcp://docs/api-reference",
            "MCP API Reference",
            "Complete API reference for MCP tools and resources",
            include_str!("../../docs/MCP-API-REFERENCE.md"),
        );

        self.add_embedded_resource(
            "mcp://docs/chat-console",
            "MCP Chat Console Guide",
            "Guide to using the interactive MCP chat console",
            include_str!("../../docs/MCP-CHAT-CONSOLE.md"),
        );

        // D-Bus documentation
        self.add_embedded_resource(
            "dbus://design/hierarchical",
            "Hierarchical D-Bus Design",
            "Design document for the hierarchical D-Bus abstraction layer",
            include_str!("../../HIERARCHICAL_DBUS_DESIGN.md"),
        );

        self.add_embedded_resource(
            "dbus://guide/introspection",
            "D-Bus Introspection with zbus",
            "Comprehensive guide to D-Bus introspection using Rust zbus",
            include_str!("../../d_bus_introspection_with_zbus.md"),
        );

        self.add_embedded_resource(
            "dbus://guide/indexer-implementation",
            "D-Bus Indexer Implementation Guide",
            "Implementation guide for the D-Bus indexer based on zbus patterns",
            include_str!("../../DBUS_INDEXER_IMPLEMENTATION_GUIDE.md"),
        );

        // Other documentation
        self.add_embedded_resource(
            "snapshot://automation",
            "Snapshot Automation Guide",
            "Guide to BTRFS snapshot automation for D-Bus index",
            include_str!("../../SNAPSHOT_AUTOMATION.md"),
        );

        self.add_embedded_resource(
            "plugin://development-guide",
            "Plugin Development Guide",
            "Complete guide to developing plugins for op-dbus",
            include_str!("../../PLUGIN-DEVELOPMENT-GUIDE.md"),
        );

        self.add_embedded_resource(
            "architecture://correct",
            "Correct Architecture",
            "The correct architecture for op-dbus system",
            include_str!("../../docs/CORRECT-ARCHITECTURE.md"),
        );

        self.add_embedded_resource(
            "architecture://final",
            "Final Architecture",
            "Final architecture design for the distributed system",
            include_str!("../../docs/FINAL-ARCHITECTURE.md"),
        );

        self.add_embedded_resource(
            "ai://prompt-templates",
            "Prompt Templates and Context Patterns",
            "Prompt templates, RAG patterns, and context management strategies for AI",
            include_str!("../../docs/PROMPT-TEMPLATES.md"),
        );

        self.add_embedded_resource(
            "ai://memory-patterns",
            "AI Memory and Context Management",
            "Memory hierarchy, context management, and knowledge retrieval patterns",
            include_str!("../../docs/MEMORY-PATTERNS.md"),
        );

        self.add_embedded_resource(
            "spec://dbus/common-interfaces",
            "Common D-Bus Interfaces Reference",
            "Public D-Bus interface specifications for systemd, NetworkManager, BlueZ, etc.",
            include_str!("../../docs/DBUS-COMMON-INTERFACES.md"),
        );

        self.add_embedded_resource(
            "spec://mcp/protocol",
            "MCP Protocol Specification",
            "Model Context Protocol (MCP) 2024-11-05 specification reference",
            include_str!("../../docs/MCP-PROTOCOL-SPEC.md"),
        );
    }

    /// Helper to add embedded resources
    fn add_embedded_resource(&mut self, uri: &str, name: &str, description: &str, content: &str) {
        self.resources.insert(
            uri.to_string(),
            Resource {
                uri: uri.to_string(),
                name: name.to_string(),
                description: description.to_string(),
                mime_type: "text/markdown".to_string(),
                content: content.to_string(),
            },
        );
    }

    /// Scan and load markdown files from /git/agents and /git/commands
    fn scan_markdown_files(&mut self) {
        let base_dirs = vec![("/git/agents", "agents"), ("/git/commands", "commands")];

        for (base_path, scheme) in base_dirs {
            if let Ok(files) = Self::find_markdown_files(base_path) {
                for file_path in files {
                    if let Ok(relative) = file_path.strip_prefix(base_path) {
                        let uri = format!("{}://{}", scheme, relative.display());
                        let name = relative.display().to_string();

                        // Read file content
                        if let Ok(content) = fs::read_to_string(&file_path) {
                            // Extract description from first heading
                            let description = Self::extract_description(&content);

                            self.resources.insert(
                                uri.clone(),
                                Resource {
                                    uri,
                                    name,
                                    description,
                                    mime_type: "text/markdown".to_string(),
                                    content,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    /// Recursively find all markdown files in a directory
    fn find_markdown_files(dir: &str) -> Result<Vec<PathBuf>, std::io::Error> {
        let mut files = Vec::new();
        Self::scan_directory(Path::new(dir), &mut files)?;
        Ok(files)
    }

    fn scan_directory(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();

                // Skip hidden directories and node_modules
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name == "node_modules" {
                        continue;
                    }
                }

                if path.is_dir() {
                    Self::scan_directory(&path, files)?;
                } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    /// Extract description from markdown content (first heading or first line)
    fn extract_description(content: &str) -> String {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                return trimmed.trim_start_matches('#').trim().to_string();
            } else if !trimmed.is_empty() {
                return trimmed.chars().take(100).collect::<String>() + "...";
            }
        }
        "Markdown documentation".to_string()
    }

    /// List all available resources
    pub fn list_resources(&self) -> Vec<&Resource> {
        self.resources.values().collect()
    }

    /// Get a specific resource by URI
    pub fn get_resource(&self, uri: &str) -> Option<&Resource> {
        self.resources.get(uri)
    }

    /// Search resources by keyword
    pub fn search(&self, query: &str) -> Vec<&Resource> {
        let query_lower = query.to_lowercase();
        self.resources
            .values()
            .filter(|r| {
                r.name.to_lowercase().contains(&query_lower)
                    || r.description.to_lowercase().contains(&query_lower)
                    || r.content.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Get resources by category (extracted from URI scheme)
    pub fn get_by_category(&self, category: &str) -> Vec<&Resource> {
        let scheme = format!("{}://", category);
        self.resources
            .values()
            .filter(|r| r.uri.starts_with(&scheme))
            .collect()
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enhanced_resource_registry() {
        let registry = ResourceRegistry::new();

        // Test embedded resources
        let resources = registry.list_resources();
        assert!(!resources.is_empty(), "Should have resources");

        // Test agents category
        let agents = registry.get_by_category("agents");
        assert!(!agents.is_empty(), "Should have agent resources");

        // Test commands category
        let commands = registry.get_by_category("commands");
        println!("Found {} command resources", commands.len());

        // Test search
        let results = registry.search("agent");
        assert!(!results.is_empty(), "Should find agent-related resources");
    }
}
