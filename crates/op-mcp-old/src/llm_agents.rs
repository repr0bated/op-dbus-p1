//! Dynamic Agent System for LLM-agnostic execution
//!
//! This module provides a comprehensive agent system that loads agent specifications
//! from the filesystem and provides a unified interface for any LLM to execute them.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

/// Supported LLM model types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ModelType {
    /// Claude Sonnet (complex reasoning tasks)
    Sonnet,
    /// Claude Haiku (fast, simple tasks)
    Haiku,
    /// GPT-4 (OpenAI)
    #[serde(rename = "gpt-4")]
    GPT4,
    /// GPT-3.5 (OpenAI, faster/cheaper)
    #[serde(rename = "gpt-3.5")]
    GPT35,
    /// Any other model
    #[serde(untagged)]
    Other(String),
}

impl From<String> for ModelType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "sonnet" => ModelType::Sonnet,
            "haiku" => ModelType::Haiku,
            "gpt-4" | "gpt4" => ModelType::GPT4,
            "gpt-3.5" | "gpt35" => ModelType::GPT35,
            _ => ModelType::Other(s),
        }
    }
}

/// Agent specification loaded from YAML frontmatter
#[derive(Debug, Clone, Deserialize)]
pub struct AgentSpec {
    pub name: String,
    pub description: String,
    pub model: Option<ModelType>,
    #[serde(flatten)]
    pub metadata: HashMap<String, Value>,
}

/// Parsed agent with full specification and prompt
#[derive(Debug, Clone)]
pub struct Agent {
    pub spec: AgentSpec,
    pub system_prompt: String,
    pub file_path: PathBuf,
    pub category: String,
}

/// Agent execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub task: String,
    pub context: Option<Value>,
    pub parameters: Option<HashMap<String, Value>>,
}

/// Agent execution response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub result: String,
    pub metadata: Option<HashMap<String, Value>>,
    pub model_used: Option<ModelType>,
}

/// Agent trait for LLM-agnostic execution
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Execute an agent with the given request
    async fn execute_agent(&self, agent: &Agent, request: AgentRequest) -> Result<AgentResponse>;

    /// Get supported model types
    fn supported_models(&self) -> Vec<ModelType>;

    /// Check if this executor can handle the given model
    fn can_handle(&self, model: &ModelType) -> bool {
        self.supported_models().contains(model)
    }
}

/// Unified agent registry that works with any LLM
#[derive(Clone)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, Agent>>>,
    executors: Arc<RwLock<Vec<Box<dyn AgentExecutor>>>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            executors: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Load all agents from the filesystem
    pub async fn load_agents(&self, agents_dir: &Path) -> Result<usize> {
        let mut agents = self.agents.write().await;
        let mut loaded_count = 0;

        // Recursively find all agent markdown files
        let agent_files = find_agent_files(agents_dir).await?;

        for file_path in agent_files {
            match load_agent_from_file(&file_path).await {
                Ok(agent) => {
                    agents.insert(agent.spec.name.clone(), agent);
                    loaded_count += 1;
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load agent from {}: {}", file_path.display(), e);
                }
            }
        }

        eprintln!("Loaded {} agents from {}", loaded_count, agents_dir.display());
        Ok(loaded_count)
    }

    /// Register an agent executor
    pub async fn register_executor(&self, executor: Box<dyn AgentExecutor>) -> Result<()> {
        let mut executors = self.executors.write().await;
        executors.push(executor);
        Ok(())
    }

    /// Get an agent by name
    pub async fn get_agent(&self, name: &str) -> Option<Agent> {
        let agents = self.agents.read().await;
        agents.get(name).cloned()
    }

    /// List all available agents
    pub async fn list_agents(&self) -> Vec<String> {
        let agents = self.agents.read().await;
        agents.keys().cloned().collect()
    }

    /// Execute an agent by name with the given request
    pub async fn execute_agent(&self, name: &str, request: AgentRequest) -> Result<AgentResponse> {
        let agent = self.get_agent(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", name))?;

        let executors = self.executors.read().await;

        // Find an executor that can handle this agent's model
        let preferred_model = agent.spec.model.as_ref().unwrap_or(&ModelType::Sonnet);

        for executor in executors.iter() {
            if executor.can_handle(preferred_model) {
                return executor.execute_agent(&agent, request).await;
            }
        }

        // Try any executor as fallback
        if let Some(executor) = executors.first() {
            return executor.execute_agent(&agent, request).await;
        }

        anyhow::bail!("No suitable executor found for agent '{}' with model {:?}", name, preferred_model)
    }

    /// Get agents by category
    pub async fn get_agents_by_category(&self, category: &str) -> Vec<Agent> {
        let agents = self.agents.read().await;
        agents.values()
            .filter(|agent| agent.category == category)
            .cloned()
            .collect()
    }

    /// Get agents that work with a specific model
    pub async fn get_agents_for_model(&self, model: &ModelType) -> Vec<Agent> {
        let agents = self.agents.read().await;
        agents.values()
            .filter(|agent| {
                agent.spec.model.as_ref().unwrap_or(&ModelType::Sonnet) == model
            })
            .cloned()
            .collect()
    }
}

/// Find all agent markdown files recursively
async fn find_agent_files(base_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    find_agent_files_recursive(base_dir, &mut files).await?;
    Ok(files)
}

async fn find_agent_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
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
            // Look for agents subdirectory
            let agents_dir = path.join("agents");
            if agents_dir.exists() {
                let mut agent_entries = fs::read_dir(&agents_dir).await?;
                while let Some(agent_entry) = agent_entries.next_entry().await? {
                    let agent_path = agent_entry.path();
                    if agent_path.extension().and_then(|e| e.to_str()) == Some("md") {
                        files.push(agent_path);
                    }
                }
            } else {
                // Recurse into subdirectories
                Box::pin(find_agent_files_recursive(&path, files)).await?;
            }
        }
    }

    Ok(())
}

/// Load a single agent from a markdown file
async fn load_agent_from_file(file_path: &Path) -> Result<Agent> {
    let content = fs::read_to_string(file_path).await
        .with_context(|| format!("Failed to read agent file: {}", file_path.display()))?;

    // Parse YAML frontmatter
    let spec = AgentSpec::from_markdown(&content)?;

    // Extract category from path
    let category = file_path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(Agent {
        spec,
        system_prompt: content,
        file_path: file_path.to_path_buf(),
        category,
    })
}

impl AgentSpec {
    /// Parse agent spec from markdown content with YAML frontmatter
    pub fn from_markdown(content: &str) -> Result<Self> {
        // Check if content starts with YAML frontmatter
        if !content.trim_start().starts_with("---") {
            anyhow::bail!("Agent file must start with YAML frontmatter (---)");
        }

        // Extract frontmatter (between --- markers)
        let content_str = content.trim_start();
        let frontmatter_end = content_str[3..].find("---")
            .ok_or_else(|| anyhow::anyhow!("Invalid YAML frontmatter - missing closing ---"))?;

        let yaml_content = &content_str[3..frontmatter_end + 3];
        let mut spec: AgentSpec = serde_yaml::from_str(yaml_content)?;

        // Ensure name is present
        if spec.name.is_empty() {
            anyhow::bail!("Agent specification must include a 'name' field");
        }

        // Convert model string to enum
        if let Some(model_str) = &spec.metadata.remove("model") {
            if let Some(model_val) = model_str.as_str() {
                spec.model = Some(ModelType::from(model_val.to_string()));
            }
        }

        Ok(spec)
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}