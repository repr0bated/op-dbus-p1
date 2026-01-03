//! Mem0 Wrapper Agent - Temporarily Disabled
//!
//! This agent wraps the Mem0 Python library for semantic memory.
//! Currently disabled pending embedder configuration (needs Ollama or local embeddings).
//!
//! To re-enable:
//! 1. Configure Ollama with nomic-embed-text model, OR
//! 2. Set up HuggingFace embeddings with proper cache paths, OR  
//! 3. Provide OPENAI_API_KEY for OpenAI embeddings

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::{Agent, AgentInfo, Task, TaskResult};

/// Mem0 wrapper state
struct Mem0State {
    initialized: bool,
    available: bool,
    last_error: Option<String>,
}

impl Default for Mem0State {
    fn default() -> Self {
        Self {
            initialized: false,
            available: false,
            last_error: Some("Mem0 temporarily disabled - pending embedder configuration".to_string()),
        }
    }
}

/// Mem0 Wrapper Agent
pub struct Mem0WrapperAgent {
    id: String,
    state: Mutex<Mem0State>,
    python_path: String,
    mem0_dir: String,
}

impl Mem0WrapperAgent {
    pub fn new(id: String) -> Self {
        let python_path = std::env::var("PYTHON_PATH")
            .unwrap_or_else(|_| "/usr/bin/python3".to_string());
        let mem0_dir = std::env::var("MEM0_DIR")
            .unwrap_or_else(|_| "/var/lib/op-dbus/.mem0".to_string());
            
        Self {
            id,
            state: Mutex::new(Mem0State::default()),
            python_path,
            mem0_dir,
        }
    }
}

#[async_trait]
impl Agent for Mem0WrapperAgent {
    fn info(&self) -> AgentInfo {
        AgentInfo {
            name: "mem0".to_string(),
            description: "Semantic memory using Mem0 (temporarily disabled)".to_string(),
            version: "0.1.0".to_string(),
            operations: vec![
                "add".to_string(),
                "search".to_string(),
                "get_all".to_string(),
                "delete".to_string(),
                "update".to_string(),
            ],
            capabilities: vec!["memory".to_string(), "semantic-search".to_string()],
        }
    }

    async fn execute(&self, task: Task) -> Result<TaskResult, String> {
        // Return graceful "not available" response
        let error_msg = "Mem0 temporarily disabled - pending embedder configuration. \
                         To enable: configure Ollama with nomic-embed-text, or provide OPENAI_API_KEY";
        
        warn!("Mem0 agent called but disabled: {}", task.operation);
        
        Ok(TaskResult {
            success: false,
            output: json!({
                "available": false,
                "error": error_msg,
                "operation": task.operation,
                "hint": "Use memory_remember/memory_recall for key-value memory instead"
            }),
            artifacts: vec![],
            metadata: {
                let mut m = HashMap::new();
                m.insert("status".to_string(), json!("disabled"));
                m
            },
        })
    }
}
