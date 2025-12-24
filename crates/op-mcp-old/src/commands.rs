//! Dynamic Command System for MCP execution
//!
//! This module provides a comprehensive command system that loads command specifications
//! from the filesystem and provides a unified interface for executing them as MCP tools.

use super::llm_agents::{AgentRegistry, AgentRequest, ModelType};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

/// Command specification loaded from YAML frontmatter
#[derive(Debug, Clone, Deserialize)]
pub struct CommandSpec {
    pub name: Option<String>,
    pub model: Option<String>,
    #[serde(flatten)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl CommandSpec {
    /// Parse command spec from markdown content with YAML frontmatter
    pub fn from_markdown(content: &str) -> Result<Self> {
        // Check if content starts with YAML frontmatter
        if !content.trim_start().starts_with("---") {
            return Ok(Self {
                name: None,
                model: None,
                metadata: HashMap::new(),
            });
        }

        // Extract frontmatter (between --- markers)
        let content_str = content.trim_start();
        let frontmatter_end = content_str[3..].find("---")
            .ok_or_else(|| anyhow::anyhow!("Invalid YAML frontmatter - missing closing ---"))?;

        let yaml_content = &content_str[3..frontmatter_end + 3];
        let mut spec: CommandSpec = serde_yaml::from_str(yaml_content)?;

        // Ensure name is present - derive from filename if not specified
        if spec.name.is_none() {
            // We'll set the name when loading the command
        }

        Ok(spec)
    }
}

/// Parsed command with full specification and prompt
#[derive(Debug, Clone)]
pub struct Command {
    pub spec: CommandSpec,
    pub system_prompt: String,
    pub file_path: PathBuf,
    pub category: String,
}

/// Command registry that works with any LLM
#[derive(Clone)]
pub struct CommandRegistry {
    commands: Arc<RwLock<HashMap<String, Command>>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all commands from the filesystem
    pub async fn load_commands(&self, commands_dir: &Path) -> Result<usize> {
        let mut commands = self.commands.write().await;
        let mut loaded_count = 0;

        // Recursively find all command markdown files
        let command_files = find_command_files(commands_dir).await?;

        for file_path in command_files {
            match load_command_from_file(&file_path).await {
                Ok(command) => {
                    if let Some(name) = &command.spec.name {
                        commands.insert(name.clone(), command);
                        loaded_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load command from {}: {}", file_path.display(), e);
                }
            }
        }

        eprintln!("Loaded {} commands from {}", loaded_count, commands_dir.display());
        Ok(loaded_count)
    }

    /// Get a command by name
    pub async fn get_command(&self, name: &str) -> Option<Command> {
        let commands = self.commands.read().await;
        commands.get(name).cloned()
    }

    /// List all available commands
    pub async fn list_commands(&self) -> Vec<String> {
        let commands = self.commands.read().await;
        commands.keys().cloned().collect()
    }

    /// Execute a command by name with the given request
    pub async fn execute_command(
        &self,
        name: &str,
        request: AgentRequest,
        agent_registry: &AgentRegistry,
    ) -> Result<super::llm_agents::AgentResponse> {
        let command = self.get_command(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Command '{}' not found", name))?;

        // Convert command to agent format and execute
        let agent_request = AgentRequest {
            task: format!("Execute command '{}': {}", name, request.task),
            context: request.context,
            parameters: request.parameters,
        };

        // Use the agent registry to execute this command
        // For now, we'll create a temporary agent-like execution
        // In a full implementation, commands might have different execution logic

        // Try to find an appropriate executor based on the model preference
        let model = command.spec.model
            .as_ref()
            .and_then(|m| match m.as_str() {
                "claude-sonnet-4-0" | "claude-opus-4-1" => Some(ModelType::Sonnet),
                "claude-haiku-3-5" => Some(ModelType::Haiku),
                "gpt-4" => Some(ModelType::GPT4),
                "gpt-3.5-turbo" => Some(ModelType::GPT35),
                _ => Some(ModelType::Sonnet),
            })
            .unwrap_or(ModelType::Sonnet);

        // Create a temporary agent-like structure for execution
        let temp_agent = super::llm_agents::Agent {
            spec: super::llm_agents::AgentSpec {
                name: command.spec.name.clone().unwrap_or_else(|| name.to_string()),
                description: format!("Command: {}", name),
                model: Some(model),
                metadata: HashMap::new(),
            },
            system_prompt: command.system_prompt.clone(),
            file_path: command.file_path.clone(),
            category: command.category.clone(),
        };

        agent_registry.execute_agent(&temp_agent.spec.name, agent_request).await
    }

    /// Get commands by category
    pub async fn get_commands_by_category(&self, category: &str) -> Vec<Command> {
        let commands = self.commands.read().await;
        commands.values()
            .filter(|command| command.category == category)
            .cloned()
            .collect()
    }
}

/// Find all command markdown files recursively
async fn find_command_files(commands_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    find_command_files_recursive(commands_dir, &mut files).await?;
    Ok(files)
}

async fn find_command_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // Skip hidden files and directories
        if path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false) {
            continue;
        }

        if path.is_dir() {
            // Look for commands in subdirectories (boxed to avoid infinite-sized future)
            Box::pin(find_command_files_recursive(&path, files)).await?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            // Skip documentation files, only include command files
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Include files that are likely commands (not README, etc.)
            if !file_name.contains("README") && !file_name.contains("CONTRIBUTING") {
                files.push(path);
            }
        }
    }

    Ok(())
}

/// Load a single command from a markdown file
async fn load_command_from_file(file_path: &Path) -> Result<Command> {
    let content = fs::read_to_string(file_path).await
        .with_context(|| format!("Failed to read command file: {}", file_path.display()))?;

    let mut spec = CommandSpec::from_markdown(&content)?;

    // If name not specified in frontmatter, derive from filename
    if spec.name.is_none() {
        let file_name = file_path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        spec.name = Some(file_name);
    }

    // Extract category from path
    let category = file_path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(Command {
        spec,
        system_prompt: content,
        file_path: file_path.to_path_buf(),
        category,
    })
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}