//! Refactored orchestrator using agent registry for loose coupling

use crate::agent_registry::{AgentRegistry, load_default_specs};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use zbus::{interface, Connection, connection::Builder, object_server::SignalEmitter};
use anyhow::{Result, Context};

/// Orchestrator for managing agents without tight coupling
pub struct Orchestrator {
    /// Agent registry for dynamic agent management
    registry: Arc<AgentRegistry>,
    
    /// Task queue for each agent
    task_queues: Arc<RwLock<HashMap<String, Vec<Task>>>>,
    
    /// Event listeners
    event_listeners: Arc<RwLock<Vec<Box<dyn EventListener>>>>,
}

/// Task to be executed by an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub agent_id: String,
    pub task_type: String,
    pub payload: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed { result: Value },
    Failed { error: String },
}

/// Event listener trait for extensibility
#[async_trait::async_trait]
pub trait EventListener: Send + Sync {
    async fn on_agent_spawned(&self, agent_id: &str, agent_type: &str);
    async fn on_agent_died(&self, agent_id: &str, reason: &str);
    async fn on_task_completed(&self, task_id: &str, result: &Value);
    async fn on_error(&self, context: &str, error: &str);
}

impl Orchestrator {
    /// Create a new orchestrator with agent registry
    pub async fn new() -> Result<Self> {
        let registry = Arc::new(AgentRegistry::new());
        
        // Load default agent specifications
        load_default_specs(&registry).await
            .context("Failed to load default agent specifications")?;
        
        // Try to load custom specs from config directory
        let config_dir = PathBuf::from("/etc/op-dbus/agents");
        if config_dir.exists() {
            if let Err(e) = registry.load_specs_from_directory(&config_dir).await {
                log::warn!("Failed to load custom agent specs: {}", e);
            }
        }
        
        Ok(Self {
            registry,
            task_queues: Arc::new(RwLock::new(HashMap::new())),
            event_listeners: Arc::new(RwLock::new(Vec::new())),
        })
    }
    
    /// Create with a custom agent registry
    pub fn with_registry(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            task_queues: Arc::new(RwLock::new(HashMap::new())),
            event_listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Register an event listener
    pub async fn add_listener(&self, listener: Box<dyn EventListener>) {
        let mut listeners = self.event_listeners.write().await;
        listeners.push(listener);
    }
    
    /// Notify all listeners of an event
    async fn notify_agent_spawned(&self, agent_id: &str, agent_type: &str) {
        let listeners = self.event_listeners.read().await;
        for listener in listeners.iter() {
            listener.on_agent_spawned(agent_id, agent_type).await;
        }
    }
    
    async fn notify_agent_died(&self, agent_id: &str, reason: &str) {
        let listeners = self.event_listeners.read().await;
        for listener in listeners.iter() {
            listener.on_agent_died(agent_id, reason).await;
        }
    }
    
    async fn notify_task_completed(&self, task_id: &str, result: &Value) {
        let listeners = self.event_listeners.read().await;
        for listener in listeners.iter() {
            listener.on_task_completed(task_id, result).await;
        }
    }
    
    async fn notify_error(&self, context: &str, error: &str) {
        let listeners = self.event_listeners.read().await;
        for listener in listeners.iter() {
            listener.on_error(context, error).await;
        }
    }
}

#[interface(name = "org.dbusmcp.Orchestrator")]
impl Orchestrator {
    /// Spawn a new agent instance dynamically
    async fn spawn_agent(
        &self,
        agent_type: String,
        config_json: String,
    ) -> zbus::fdo::Result<String> {
        let config: Option<Value> = if config_json.is_empty() {
            None
        } else {
            Some(serde_json::from_str(&config_json)
                .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid config: {}", e)))?)
        };
        
        match self.registry.spawn_agent(&agent_type, config).await {
            Ok(agent_id) => {
                // Notify listeners
                self.notify_agent_spawned(&agent_id, &agent_type).await;
                
                // Initialize task queue for this agent
                let mut queues = self.task_queues.write().await;
                queues.insert(agent_id.clone(), Vec::new());
                
                Ok(agent_id)
            }
            Err(e) => {
                self.notify_error("spawn_agent", &e.to_string()).await;
                Err(zbus::fdo::Error::Failed(format!("Failed to spawn agent: {}", e)))
            }
        }
    }
    
    /// Send a task to an agent
    async fn send_task(
        &self,
        agent_id: String,
        task_json: String,
    ) -> zbus::fdo::Result<String> {
        let task_data: Value = serde_json::from_str(&task_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid task: {}", e)))?;
        
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.clone(),
            task_type: task_data["type"].as_str().unwrap_or("unknown").to_string(),
            payload: task_data,
            created_at: chrono::Utc::now(),
            status: TaskStatus::Pending,
        };
        
        // Add to task queue
        let mut queues = self.task_queues.write().await;
        queues.entry(agent_id.clone())
            .or_insert_with(Vec::new)
            .push(task.clone());
        
        // In a real implementation, this would send to the agent via D-Bus
        // For now, return the task ID
        Ok(task.id)
    }
    
    /// Get status of an agent
    async fn get_agent_status(&self, agent_id: String) -> zbus::fdo::Result<String> {
        match self.registry.get_instance_status(&agent_id).await {
            Ok(instance) => {
                let status_json = serde_json::to_string(&instance)
                    .map_err(|e| zbus::fdo::Error::Failed(format!("Serialization error: {}", e)))?;
                Ok(status_json)
            }
            Err(e) => {
                Err(zbus::fdo::Error::Failed(format!("Failed to get status: {}", e)))
            }
        }
    }
    
    /// List all active agents
    async fn list_agents(&self) -> zbus::fdo::Result<Vec<String>> {
        let instances = self.registry.list_instances().await;
        Ok(instances.into_iter().map(|i| i.id).collect())
    }
    
    /// List available agent types
    async fn list_agent_types(&self) -> zbus::fdo::Result<Vec<String>> {
        Ok(self.registry.list_agent_types().await)
    }
    
    /// Get agent type specification
    async fn get_agent_spec(&self, agent_type: String) -> zbus::fdo::Result<String> {
        match self.registry.get_spec(&agent_type).await {
            Some(spec) => {
                let spec_json = serde_json::to_string(&spec)
                    .map_err(|e| zbus::fdo::Error::Failed(format!("Serialization error: {}", e)))?;
                Ok(spec_json)
            }
            None => {
                Err(zbus::fdo::Error::Failed(format!("Unknown agent type: {}", agent_type)))
            }
        }
    }
    
    /// Kill an agent instance
    async fn kill_agent(&self, agent_id: String) -> zbus::fdo::Result<bool> {
        match self.registry.kill_agent(&agent_id).await {
            Ok(()) => {
                self.notify_agent_died(&agent_id, "Killed by user").await;
                
                // Remove task queue
                let mut queues = self.task_queues.write().await;
                queues.remove(&agent_id);
                
                Ok(true)
            }
            Err(e) => {
                self.notify_error("kill_agent", &e.to_string()).await;
                Err(zbus::fdo::Error::Failed(format!("Failed to kill agent: {}", e)))
            }
        }
    }
    
    /// Get pending tasks for an agent
    async fn get_pending_tasks(&self, agent_id: String) -> zbus::fdo::Result<String> {
        let queues = self.task_queues.read().await;
        let tasks = queues.get(&agent_id)
            .map(|tasks| tasks.clone())
            .unwrap_or_default();
        
        let tasks_json = serde_json::to_string(&tasks)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Serialization error: {}", e)))?;
        
        Ok(tasks_json)
    }
    
    /// Mark a task as completed
    async fn complete_task(
        &self,
        task_id: String,
        result_json: String,
    ) -> zbus::fdo::Result<bool> {
        let result: Value = serde_json::from_str(&result_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid result: {}", e)))?;
        
        // Find and update the task
        let mut queues = self.task_queues.write().await;
        for tasks in queues.values_mut() {
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = TaskStatus::Completed { result: result.clone() };
                
                // Notify listeners
                self.notify_task_completed(&task_id, &result).await;
                
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// Signals
    #[zbus(signal)]
    async fn agent_spawned(
        signal_emitter: signal_ctx: &object_server::SignalEmitterSignalEmitter<'_>,
        agent_id: String,
        agent_type: String,
    ) -> zbus::Result<()>;
    
    #[zbus(signal)]
    async fn agent_died(
        signal_emitter: signal_ctx: &object_server::SignalEmitterSignalEmitter<'_>,
        agent_id: String,
        reason: String,
    ) -> zbus::Result<()>;
    
    #[zbus(signal)]
    async fn task_completed(
        signal_emitter: signal_ctx: &object_server::SignalEmitterSignalEmitter<'_>,
        task_id: String,
        result: String,
    ) -> zbus::Result<()>;
}

/// Example event listener for logging
pub struct LoggingEventListener;

#[async_trait::async_trait]
impl EventListener for LoggingEventListener {
    async fn on_agent_spawned(&self, agent_id: &str, agent_type: &str) {
        log::info!("Agent spawned: {} (type: {})", agent_id, agent_type);
    }
    
    async fn on_agent_died(&self, agent_id: &str, reason: &str) {
        log::warn!("Agent died: {} (reason: {})", agent_id, reason);
    }
    
    async fn on_task_completed(&self, task_id: &str, result: &Value) {
        log::info!("Task completed: {} with result: {:?}", task_id, result);
    }
    
    async fn on_error(&self, context: &str, error: &str) {
        log::error!("Error in {}: {}", context, error);
    }
}

/// Main entry point for the refactored orchestrator
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();
    
    log::info!("Starting refactored orchestrator with agent registry");
    
    // Create orchestrator
    let orchestrator = Orchestrator::new().await?;
    
    // Add logging listener
    orchestrator.add_listener(Box::new(LoggingEventListener)).await;
    
    // Set up D-Bus connection
    let connection = Builder::session()?
        .name("org.dbusmcp.Orchestrator")?
        .serve_at("/org/dbusmcp/Orchestrator", orchestrator)?
        .build()
        .await?;
    
    log::info!("Orchestrator ready on D-Bus");
    log::info!("Service: org.dbusmcp.Orchestrator");
    log::info!("Path: /org/dbusmcp/Orchestrator");
    
    // Keep running
    std::future::pending::<()>().await;
    
    Ok(())
}