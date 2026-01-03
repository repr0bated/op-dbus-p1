//! Memory Agent
//!
//! Provides persistent memory storage for the system.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::fs;
use std::path::PathBuf;

use crate::agents::base::{AgentTask, AgentTrait, TaskResult};
use crate::security::SecurityProfile;

pub struct MemoryAgent {
    agent_id: String,
    profile: SecurityProfile,
    memory_path: PathBuf,
    cache: Arc<RwLock<HashMap<String, String>>>,
}

impl MemoryAgent {
    pub fn new(agent_id: String) -> Self {
        let memory_path = PathBuf::from("/var/lib/op-dbus/memory.json");
        let cache = if let Ok(content) = fs::read_to_string(&memory_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Self {
            agent_id,
            profile: SecurityProfile::orchestration("memory", vec!["*"]),
            memory_path,
            cache: Arc::new(RwLock::new(cache)),
        }
    }

    fn persist(&self) -> Result<(), String> {
        let cache = self.cache.read().map_err(|_| "Failed to acquire lock")?;
        let content = serde_json::to_string_pretty(&*cache).map_err(|e| e.to_string())?;
        fs::write(&self.memory_path, content).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn remember(&self, key: Option<&str>, value: Option<&str>) -> Result<String, String> {
        let key = key.ok_or("Key required")?;
        let value = value.ok_or("Value required")?;

        {
            let mut cache = self.cache.write().map_err(|_| "Failed to acquire lock")?;
            cache.insert(key.to_string(), value.to_string());
        }
        self.persist()?;

        Ok(format!("Remembered: {}", key))
    }

    fn recall(&self, key: Option<&str>) -> Result<String, String> {
        let key = key.ok_or("Key required")?;

        let cache = self.cache.read().map_err(|_| "Failed to acquire lock")?;
        
        // Simple exact match first
        if let Some(value) = cache.get(key) {
            return Ok(format!("Recalled (exact): {} = {}", key, value));
        }

        // Fuzzy search / contains
        let matches: Vec<(String, String)> = cache.iter()
            .filter(|(k, _)| k.contains(key))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if matches.is_empty() {
            Err(format!("Nothing found for '{}'", key))
        } else {
            let result = matches.iter()
                .map(|(k, v)| format!("{} = {}", k, v))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(format!("Recalled (matches):\n{}", result))
        }
    }

    fn forget(&self, key: Option<&str>) -> Result<String, String> {
        let key = key.ok_or("Key required")?;
        
        {
            let mut cache = self.cache.write().map_err(|_| "Failed to acquire lock")?;
            cache.remove(key);
        }
        self.persist()?;

        Ok(format!("Forgotten: {}", key))
    }
}

#[async_trait]
impl AgentTrait for MemoryAgent {
    fn agent_type(&self) -> &str {
        "memory"
    }
    fn name(&self) -> &str {
        "Memory Agent"
    }
    fn description(&self) -> &str {
        "Persistent memory management for storing facts and context across sessions"
    }

    fn operations(&self) -> Vec<String> {
        vec![
            "remember".to_string(),
            "recall".to_string(),
            "forget".to_string(),
        ]
    }

    fn security_profile(&self) -> &SecurityProfile {
        &self.profile
    }

    async fn execute(&self, task: AgentTask) -> Result<TaskResult, String> {
        let result = match task.operation.as_str() {
            "remember" => self.remember(task.path.as_deref(), task.args.as_deref()),
            "recall" => self.recall(task.path.as_deref().or(task.args.as_deref())),
            "forget" => self.forget(task.path.as_deref().or(task.args.as_deref())),
            _ => Err(format!("Unknown operation: {}", task.operation)),
        };

        match result {
            Ok(data) => Ok(TaskResult::success(&task.operation, data)),
            Err(e) => Ok(TaskResult::failure(&task.operation, e)),
        }
    }
}
