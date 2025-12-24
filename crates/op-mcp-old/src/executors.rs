//! LLM Executors for different AI models
//!
//! This module provides executor implementations for various LLM providers,
//! allowing agents to work with any supported model.

use super::llm_agents::{Agent, AgentExecutor, AgentRequest, AgentResponse, ModelType};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Claude API executor
pub struct ClaudeExecutor {
    client: Client,
    api_key: String,
}

impl ClaudeExecutor {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

#[async_trait]
impl AgentExecutor for ClaudeExecutor {
    async fn execute_agent(&self, agent: &Agent, request: AgentRequest) -> Result<AgentResponse> {
        let model = match agent.spec.model.as_ref().unwrap_or(&ModelType::Sonnet) {
            ModelType::Sonnet => "claude-3-5-sonnet-20241022",
            ModelType::Haiku => "claude-3-5-haiku-20241022",
            _ => "claude-3-5-sonnet-20241022", // fallback
        };

        let messages = vec![
            serde_json::json!({
                "role": "system",
                "content": agent.system_prompt
            }),
            serde_json::json!({
                "role": "user",
                "content": request.task
            })
        ];

        let payload = serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "messages": messages,
            "temperature": 0.7
        });

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Claude API error: {}", error_text);
        }

        let response_json: Value = response.json().await?;
        let content = response_json["content"][0]["text"]
            .as_str()
            .unwrap_or("No response content");

        Ok(AgentResponse {
            result: content.to_string(),
            metadata: Some(HashMap::from([
                ("model".to_string(), Value::String(model.to_string())),
                ("tokens_used".to_string(), response_json["usage"]["input_tokens"].clone()),
            ])),
            model_used: Some(agent.spec.model.clone().unwrap_or(ModelType::Sonnet)),
        })
    }

    fn supported_models(&self) -> Vec<ModelType> {
        vec![ModelType::Sonnet, ModelType::Haiku]
    }
}

/// OpenAI API executor
pub struct OpenAIExecutor {
    client: Client,
    api_key: String,
}

impl OpenAIExecutor {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

#[async_trait]
impl AgentExecutor for OpenAIExecutor {
    async fn execute_agent(&self, agent: &Agent, request: AgentRequest) -> Result<AgentResponse> {
        let model = match agent.spec.model.as_ref().unwrap_or(&ModelType::GPT4) {
            ModelType::GPT4 => "gpt-4",
            ModelType::GPT35 => "gpt-3.5-turbo",
            _ => "gpt-4", // fallback
        };

        let messages = vec![
            serde_json::json!({
                "role": "system",
                "content": agent.system_prompt
            }),
            serde_json::json!({
                "role": "user",
                "content": request.task
            })
        ];

        let payload = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": 4096,
            "temperature": 0.7
        });

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("OpenAI API error: {}", error_text);
        }

        let response_json: Value = response.json().await?;
        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("No response content");

        Ok(AgentResponse {
            result: content.to_string(),
            metadata: Some(HashMap::from([
                ("model".to_string(), Value::String(model.to_string())),
                ("finish_reason".to_string(), response_json["choices"][0]["finish_reason"].clone()),
            ])),
            model_used: Some(agent.spec.model.clone().unwrap_or(ModelType::GPT4)),
        })
    }

    fn supported_models(&self) -> Vec<ModelType> {
        vec![ModelType::GPT4, ModelType::GPT35]
    }
}

/// Mock executor for testing (returns formatted responses without API calls)
pub struct MockExecutor;

impl MockExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentExecutor for MockExecutor {
    async fn execute_agent(&self, agent: &Agent, request: AgentRequest) -> Result<AgentResponse> {
        let model = agent.spec.model.as_ref().unwrap_or(&ModelType::Sonnet);

        let mock_response = format!(
            "[MOCK RESPONSE] Agent '{}' executed task: {}\n\nModel: {:?}\nCategory: {}\n\nThis is a mock response for testing purposes.",
            agent.spec.name,
            request.task,
            model,
            agent.category
        );

        Ok(AgentResponse {
            result: mock_response,
            metadata: Some(HashMap::from([
                ("mock".to_string(), Value::Bool(true)),
                ("agent_name".to_string(), Value::String(agent.spec.name.clone())),
            ])),
            model_used: Some(model.clone()),
        })
    }

    fn supported_models(&self) -> Vec<ModelType> {
        vec![
            ModelType::Sonnet,
            ModelType::Haiku,
            ModelType::GPT4,
            ModelType::GPT35,
        ]
    }
}

/// Factory for creating executors based on environment/configuration
pub struct ExecutorFactory;

impl ExecutorFactory {
    /// Create executors based on available API keys and configuration
    pub fn create_executors() -> Vec<Box<dyn AgentExecutor>> {
        let mut executors = Vec::new();

        // Add mock executor for testing
        executors.push(Box::new(MockExecutor::new()) as Box<dyn AgentExecutor>);

        // Add Claude executor if API key is available
        if let Ok(claude_key) = std::env::var("ANTHROPIC_API_KEY") {
            executors.push(Box::new(ClaudeExecutor::new(claude_key)));
        }

        // Add OpenAI executor if API key is available
        if let Ok(openai_key) = std::env::var("OPENAI_API_KEY") {
            executors.push(Box::new(OpenAIExecutor::new(openai_key)));
        }

        executors
    }

    /// Get recommended executor priority (Claude > OpenAI > Mock)
    pub fn get_executor_priority() -> Vec<String> {
        vec![
            "ClaudeExecutor".to_string(),
            "OpenAIExecutor".to_string(),
            "MockExecutor".to_string(),
        ]
    }
}