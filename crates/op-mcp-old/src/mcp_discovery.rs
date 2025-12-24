//! MCP Server Discovery and Config Generation
//! Discovers hosted MCP servers and writes JSON configs for popular clients

use crate::mcp_client::{HostedMcpInfo, McpClient};
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Discover hosted MCP servers and write configs for all popular clients
pub async fn discover_and_write_configs(
    config_file: Option<&str>,
    output_dir: Option<&str>,
) -> Result<Vec<HostedMcpInfo>> {
    let client = McpClient::new();

    // Load configs from file or default location
    let config_path = config_file.unwrap_or("/home/jeremy/.config/Cursor/mcp.json");
    if std::path::Path::new(config_path).exists() {
        client
            .load_configs_from_file(config_path)
            .await
            .context("Failed to load MCP configs")?;
        eprintln!("Loaded MCP configs from: {}", config_path);
    } else {
        eprintln!("Warning: Config file not found: {}", config_path);
        eprintln!("  Skipping hosted MCP server discovery");
        return Ok(vec![]);
    }

    // Discover all configured servers
    eprintln!("\nDiscovering hosted MCP servers...");
    let discovered = client
        .discover_servers()
        .await
        .context("Failed to discover MCP servers")?;

    eprintln!("\nDiscovered {} MCP servers:", discovered.len());
    for server in &discovered {
        match &server.status {
            crate::mcp_client::McpServerStatus::Connected => {
                eprintln!("  ✓ {} ({} tools)", server.name, server.tools.len());
            }
            crate::mcp_client::McpServerStatus::Disconnected => {
                eprintln!("  ✗ {} (disconnected)", server.name);
            }
            crate::mcp_client::McpServerStatus::Error(e) => {
                eprintln!("  ✗ {} (error: {})", server.name, e);
            }
        }
    }

    // Write configs for all popular clients
    let output_base = output_dir.unwrap_or("mcp-configs");
    eprintln!("\nWriting client configurations to: {}", output_base);

    client
        .write_client_configs(output_base, &discovered)
        .await
        .context("Failed to write client configs")?;

    // Also write individual tool configs
    client
        .write_individual_tool_configs(output_base, &discovered)
        .await
        .context("Failed to write individual tool configs")?;

    eprintln!("\n✅ Config generation complete!");
    eprintln!(
        "   Client configs: {}/{{cursor,vscode,claude-desktop,aider}}/mcp.json",
        output_base
    );
    eprintln!("   Individual tools: {}/tools/", output_base);

    // Disconnect from all servers
    client.disconnect_all().await;

    Ok(discovered)
}

/// Get the default config file path for a client
pub fn get_default_config_path(client: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());

    match client {
        "cursor" => PathBuf::from(&home).join(".config/Cursor/mcp.json"),
        "vscode" => PathBuf::from(&home).join(".config/Code/User/globalStorage/mcp.json"),
        "claude-desktop" => PathBuf::from(&home).join(".config/Claude/mcp.json"),
        "aider" => PathBuf::from(&home).join(".aider/mcp.json"),
        _ => PathBuf::from(&home).join(".config/mcp.json"),
    }
}
