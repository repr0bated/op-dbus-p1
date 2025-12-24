//! Embedded comprehensive agents using rust-embed
//!
//! This module bundles the entire comprehensive-agents directory into the binary
//! for portability, while keeping file access efficient via memory mapping.

use crate::resources::Resource;
// TODO: rust_embed is an optional dependency  
// use rust_embed::RustEmbed;
use std::collections::HashMap;

/// Embedded comprehensive agents directory
#[derive(RustEmbed)]
#[folder = "comprehensive-agents"]
pub struct ComprehensiveAgents;

/// Load comprehensive agents as MCP resources
pub fn load_comprehensive_agents() -> HashMap<String, Resource> {
    let mut resources = HashMap::new();

    // Iterate through all embedded files
    for file in ComprehensiveAgents::iter() {
        let file_path = file.as_ref();

        // Only load markdown files in agents subdirectories
        if file_path.ends_with(".md") && file_path.contains("/agents/") {
            if let Some(content) = ComprehensiveAgents::get(file_path) {
                // Extract plugin and agent name from path: plugins/{plugin}/agents/{agent}.md
                let parts: Vec<&str> = file_path.split('/').collect();
                if parts.len() >= 4 && parts[0] == "plugins" && parts[2] == "agents" {
                    let plugin_name = parts[1];
                    let agent_name = parts[3].strip_suffix(".md").unwrap_or("unknown");

                    // Create unique URI with plugin namespace
                    let uri = format!("agent://comprehensive/{}/{}", plugin_name, agent_name);

                    // Generate human-readable name and description
                    let display_name = agent_name
                        .replace('-', " ")
                        .replace('_', " ")
                        .split_whitespace()
                        .map(|word| {
                            let mut chars = word.chars();
                            match chars.next() {
                                None => String::new(),
                                Some(first) => {
                                    first.to_uppercase().collect::<String>() + chars.as_str()
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");

                    let plugin_display = plugin_name
                        .replace('-', " ")
                        .split_whitespace()
                        .map(|word| {
                            let mut chars = word.chars();
                            match chars.next() {
                                None => String::new(),
                                Some(first) => {
                                    first.to_uppercase().collect::<String>() + chars.as_str()
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");

                    let description = format!(
                        "{} agent from {} plugin",
                        display_name.to_lowercase(),
                        plugin_display.to_lowercase()
                    );

                    // Convert content to string
                    let content_str = String::from_utf8_lossy(&content.data).to_string();

                    resources.insert(
                        uri.clone(),
                        Resource {
                            uri,
                            name: format!("{} Agent ({})", display_name, plugin_display),
                            description,
                            mime_type: "text/markdown".to_string(),
                            content: content_str,
                        },
                    );
                }
            }
        }
    }

    resources
}
