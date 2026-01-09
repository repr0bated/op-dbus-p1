//! Trait-based Agent Executor
//!
//! Executes agents using the existing AgentTrait implementations
//! instead of requiring separate D-Bus service processes.
//!
//! This is the recommended executor for production use.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};

use op_agents::agents::base::{AgentTrait, AgentTask, TaskResult};

// Import agent implementations
use op_agents::agents::{
    language::{RustProAgent, PythonProAgent},
    architecture::BackendArchitectAgent,
    infrastructure::NetworkEngineerAgent,
    orchestration::{MemoryAgent, ContextManagerAgent, SequentialThinkingAgent, Mem0WrapperAgent},
    seo::SearchSpecialistAgent,
    infrastructure::DeploymentAgent,
    analysis::DebuggerAgent,
    aiml::PromptEngineerAgent,
};

use super::agents_server::AgentExecutor;

/// Agent entry in the registry
struct AgentEntry {
    agent: Box<dyn AgentTrait + Send + Sync>,
    started: bool,
}

/// Trait-based agent executor
/// 
/// Uses the existing AgentTrait implementations to execute agent operations.
/// No D-Bus services required.
pub struct TraitAgentExecutor {
    agents: Arc<RwLock<HashMap<String, AgentEntry>>>,
}

impl TraitAgentExecutor {
    /// Create a new executor with default agents registered
    pub fn new() -> Self {
        let executor = Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Register agents synchronously during construction
        // We'll use a blocking approach since this is initialization
        let agents = executor.agents.clone();
        
        tokio::spawn(async move {
            let mut map = agents.write().await;
            
            // Core run-on-connection agents
            Self::register_agent(&mut map, "rust_pro", Box::new(RustProAgent::new("rust_pro".to_string())));
            Self::register_agent(&mut map, "python_pro", Box::new(PythonProAgent::new("python_pro".to_string())));
            Self::register_agent(&mut map, "backend_architect", Box::new(BackendArchitectAgent::new("backend_architect".to_string())));
            Self::register_agent(&mut map, "network_engineer", Box::new(NetworkEngineerAgent::new("network_engineer".to_string())));
            Self::register_agent(&mut map, "memory", Box::new(MemoryAgent::new("memory".to_string())));
            Self::register_agent(&mut map, "context_manager", Box::new(ContextManagerAgent::new("context_manager".to_string())));
            Self::register_agent(&mut map, "sequential_thinking", Box::new(SequentialThinkingAgent::new("sequential_thinking".to_string())));
            
            // On-demand agents
            Self::register_agent(&mut map, "mem0", Box::new(Mem0WrapperAgent::new("mem0".to_string())));
            Self::register_agent(&mut map, "search_specialist", Box::new(SearchSpecialistAgent::new("search_specialist".to_string())));
            Self::register_agent(&mut map, "deployment", Box::new(DeploymentAgent::new("deployment".to_string())));
            Self::register_agent(&mut map, "debugger", Box::new(DebuggerAgent::new("debugger".to_string())));
            Self::register_agent(&mut map, "prompt_engineer", Box::new(PromptEngineerAgent::new("prompt_engineer".to_string())));
            
            info!("TraitAgentExecutor: Registered {} agents", map.len());
        });
        
        executor
    }
    
    fn register_agent(
        map: &mut HashMap<String, AgentEntry>,
        id: &str,
        agent: Box<dyn AgentTrait + Send + Sync>,
    ) {
        map.insert(id.to_string(), AgentEntry {
            agent,
            started: false,
        });
    }
    
    /// Register an additional agent at runtime
    pub async fn register(&self, id: &str, agent: Box<dyn AgentTrait + Send + Sync>) {
        let mut agents = self.agents.write().await;
        agents.insert(id.to_string(), AgentEntry {
            agent,
            started: false,
        });
        info!(agent = %id, "Registered agent");
    }
    
    /// List all registered agents
    pub async fn list_agents(&self) -> Vec<String> {
        self.agents.read().await.keys().cloned().collect()
    }
}

impl Default for TraitAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentExecutor for TraitAgentExecutor {
    async fn start_agent(&self, agent_id: &str, _dbus_service: Option<&str>) -> Result<()> {
        let mut agents = self.agents.write().await;
        
        if let Some(entry) = agents.get_mut(agent_id) {
            entry.started = true;
            info!(agent = %agent_id, "✓ Agent started (trait-based)");
            Ok(())
        } else {
            warn!(agent = %agent_id, "Agent not found in registry");
            Err(anyhow::anyhow!("Agent not registered: {}", agent_id))
        }
    }
    
    async fn stop_agent(&self, agent_id: &str) -> Result<()> {
        let mut agents = self.agents.write().await;
        
        if let Some(entry) = agents.get_mut(agent_id) {
            entry.started = false;
            info!(agent = %agent_id, "Agent stopped");
        }
        
        Ok(())
    }
    
    async fn execute(&self, agent_id: &str, operation: &str, args: Value) -> Result<Value> {
        debug!(agent = %agent_id, operation = %operation, "Executing agent");
        
        let agents = self.agents.read().await;
        
        let entry = agents.get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent_id))?;
        
        // Build task
        let task = AgentTask {
            task_type: entry.agent.agent_type().to_string(),
            operation: operation.to_string(),
            path: args.get("path").and_then(|p| p.as_str()).map(String::from),
            args: Some(serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string())),
            config: args.as_object()
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        };
        
        // Execute
        match entry.agent.execute(task).await {
            Ok(result) => {
                debug!(agent = %agent_id, success = %result.success, "Agent execution complete");
                
                Ok(json!({
                    "success": result.success,
                    "operation": result.operation,
                    "output": result.data,
                    "agent": agent_id
                }))
            }
            Err(e) => {
                error!(agent = %agent_id, error = %e, "Agent execution failed");
                Err(anyhow::anyhow!("Agent {} failed: {}", agent_id, e))
            }
        }
    }
    
    async fn is_running(&self, agent_id: &str) -> bool {
        self.agents.read().await
            .get(agent_id)
            .map(|e| e.started)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_executor_creation() {
        let executor = TraitAgentExecutor::new();
        // Give time for async registration
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let agents = executor.list_agents().await;
        assert!(!agents.is_empty());
    }
    
    #[tokio::test]
    async fn test_start_agent() {
        let executor = TraitAgentExecutor::new();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let result = executor.start_agent("memory", None).await;
        assert!(result.is_ok());
        assert!(executor.is_running("memory").await);
    }
    
    #[tokio::test]
    async fn test_execute_memory_list() {
        let executor = TraitAgentExecutor::new();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        executor.start_agent("memory", None).await.unwrap();
        
        let result = executor.execute("memory", "list", json!({})).await;
        assert!(result.is_ok());
    }
}
