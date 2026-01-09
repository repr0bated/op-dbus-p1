# Antigravity Integration - Agentic AI Without Vertex

## What is Antigravity?

**Antigravity** = Agentic capabilities using standard (free) APIs

| Component | What | Cost |
|-----------|------|------|
| **Google OAuth** | User identity only | Free |
| **Gemini API** | `generativelanguage.googleapis.com` | Free tier (15 RPM, 1M tokens/day) |
| **MCP Agents** | Your op-dbus agents | Free (your infra) |
| **Agentic Layer** | Auto-routing, memory, orchestration | Free (your code) |

**NOT Vertex AI** — That requires GCP project + billing + API enablement = $$$

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           ANTIGRAVITY                                       │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    User Identity (Google OAuth)                     │   │
│  │  - Login with Google account                                        │   │
│  │  - Get user profile (name, email, picture)                         │   │
│  │  - Session management                                               │   │
│  │  - NOT for API auth — just identity                                │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    LLM Layer (Standard Gemini API)                  │   │
│  │  Endpoint: https://generativelanguage.googleapis.com/v1beta         │   │
│  │  Auth: API key (GEMINI_API_KEY) — NOT Vertex AI                    │   │
│  │                                                                     │   │
│  │  Free Tier: 15 RPM, 1M tokens/day, 1500 requests/day               │   │
│  │                                                                     │   │
│  │  Models: gemini-2.0-flash, gemini-1.5-pro, gemini-1.5-flash        │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    Agentic Layer                                    │   │
│  │  - MCP Client → Connects to your op-dbus MCP servers               │   │
│  │  - Auto-routing → Select best agent for task                       │   │
│  │  - Memory → Session + persistent (via memory agent)                │   │
│  │  - Context → Managed via context_manager agent                     │   │
│  │  - Sequential thinking → Step-by-step reasoning                    │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                              │
                              │ MCP (SSE)
                              ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    YOUR EXISTING OP-DBUS INFRASTRUCTURE                     │
│                                                                             │
│  /mcp/agents (11 agents)        /mcp/compact (meta-tools)                  │
│  ┌─────────────────────┐        ┌─────────────────────┐                    │
│  │ rust_pro           │        │ list_tools          │                    │
│  │ backend_architect  │        │ search_tools        │                    │
│  │ sequential_thinking│        │ get_tool_schema     │                    │
│  │ memory             │        │ execute_tool        │                    │
│  │ context_manager    │        └─────────────────────┘                    │
│  │ mem0               │                                                    │
│  │ search_specialist  │        D-Bus Services                             │
│  │ python_pro         │        ┌─────────────────────┐                    │
│  │ debugger           │        │ org.dbusmcp.Agent.* │                    │
│  │ deployment         │        │ 80+ agent types     │                    │
│  │ prompt_engineer    │        └─────────────────────┘                    │
│  └─────────────────────┘                                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Step 1: Get Gemini API Key (Free)

```bash
# Go to https://aistudio.google.com/
# Click "Get API Key"
# Create key (no billing required)
export GEMINI_API_KEY="your-api-key-here"
```

**Free Tier Limits:**
- 15 requests/minute
- 1 million tokens/day
- 1,500 requests/day

---

## Step 2: Create Antigravity Provider

### `crates/op-llm/src/antigravity.rs`

```rust
//! Antigravity Provider - Agentic AI Without Vertex
//!
//! Uses:
//! - Google OAuth for user identity (optional)
//! - Standard Gemini API (NOT Vertex AI) for LLM
//! - Your existing MCP agents for agentic capabilities
//!
//! NO VERTEX AI. NO GCP BILLING.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, LlmProvider, ModelInfo,
    ProviderType, TokenUsage, ToolCallInfo, ToolChoice, ToolDefinition,
};

// =============================================================================
// STANDARD GEMINI API (NOT VERTEX)
// =============================================================================

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Antigravity configuration
#[derive(Debug, Clone)]
pub struct AntigravityConfig {
    /// Gemini API key (from aistudio.google.com)
    pub api_key: String,
    /// Default model
    pub default_model: String,
    /// Enable auto model selection
    pub auto_routing: bool,
    /// MCP server URL for agents
    pub mcp_server_url: Option<String>,
    /// Enable agentic mode
    pub agentic_mode: bool,
    /// User info from Google OAuth (optional)
    pub user: Option<GoogleUser>,
}

impl AntigravityConfig {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_API_KEY"))
            .context(
                "GEMINI_API_KEY not set. Get one free at https://aistudio.google.com/"
            )?;

        Ok(Self {
            api_key,
            default_model: std::env::var("ANTIGRAVITY_MODEL")
                .unwrap_or_else(|_| "gemini-2.0-flash".to_string()),
            auto_routing: std::env::var("ANTIGRAVITY_AUTO_ROUTING")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),
            mcp_server_url: std::env::var("MCP_SERVER_URL").ok()
                .or_else(|| Some("http://localhost:8080/mcp/agents".to_string())),
            agentic_mode: std::env::var("ANTIGRAVITY_AGENTIC")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),
            user: None,
        })
    }
}

/// Google user info from OAuth
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

// =============================================================================
// GEMINI API TYPES (Standard API, not Vertex)
// =============================================================================

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(rename = "toolConfig", skip_serializing_if = "Option::is_none")]
    tool_config: Option<GeminiToolConfig>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: Value,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Serialize)]
struct GeminiToolConfig {
    #[serde(rename = "functionCallingConfig")]
    function_calling_config: FunctionCallingConfig,
}

#[derive(Debug, Serialize)]
struct FunctionCallingConfig {
    mode: String, // "AUTO", "ANY", "NONE"
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(rename = "topP", skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
    error: Option<GeminiError>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiError {
    code: i32,
    message: String,
}

// =============================================================================
// ANTIGRAVITY PROVIDER
// =============================================================================

/// Antigravity LLM Provider
/// 
/// Agentic AI using standard Gemini API (NOT Vertex AI)
pub struct AntigravityProvider {
    config: AntigravityConfig,
    client: Client,
    mcp_tools: RwLock<Vec<ToolDefinition>>,
}

impl AntigravityProvider {
    /// Create from environment
    pub fn from_env() -> Result<Self> {
        let config = AntigravityConfig::from_env()?;
        Self::new(config)
    }

    /// Create with config
    pub fn new(config: AntigravityConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;

        info!("Antigravity provider initialized");
        info!("  Mode: Standard Gemini API (NOT Vertex AI)");
        info!("  Model: {}", config.default_model);
        info!("  Agentic: {}", config.agentic_mode);
        info!("  MCP: {:?}", config.mcp_server_url);

        Ok(Self {
            config,
            client,
            mcp_tools: RwLock::new(Vec::new()),
        })
    }

    /// Set user from OAuth login
    pub fn set_user(&mut self, user: GoogleUser) {
        info!("User authenticated: {} ({})", user.name, user.email);
        self.config.user = Some(user);
    }

    /// Get current user
    pub fn user(&self) -> Option<&GoogleUser> {
        self.config.user.as_ref()
    }

    /// Load tools from MCP server
    pub async fn load_mcp_tools(&self) -> Result<Vec<ToolDefinition>> {
        let url = match &self.config.mcp_server_url {
            Some(url) => url,
            None => return Ok(Vec::new()),
        };

        debug!("Loading MCP tools from {}", url);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        });

        let message_url = if url.ends_with("/sse") {
            url.replace("/sse", "/message")
        } else {
            format!("{}/message", url.trim_end_matches('/'))
        };

        let response = match self.client
            .post(&message_url)
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to connect to MCP server: {}", e);
                return Ok(Vec::new());
            }
        };

        if !response.status().is_success() {
            warn!("MCP server error: {}", response.status());
            return Ok(Vec::new());
        }

        let result: Value = response.json().await?;
        let tools = result
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(ToolDefinition {
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t.get("description")?.as_str()?.to_string(),
                            parameters: t.get("inputSchema").cloned().unwrap_or(json!({})),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        info!("Loaded {} MCP tools", tools.len());
        *self.mcp_tools.write().await = tools.clone();

        Ok(tools)
    }

    /// Execute MCP tool
    pub async fn execute_mcp_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let url = self.config.mcp_server_url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP server not configured"))?;

        let message_url = if url.ends_with("/sse") {
            url.replace("/sse", "/message")
        } else {
            format!("{}/message", url.trim_end_matches('/'))
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });

        let response = self.client
            .post(&message_url)
            .json(&request)
            .send()
            .await?;

        let result: Value = response.json().await?;

        if let Some(error) = result.get("error") {
            anyhow::bail!("MCP error: {}", error);
        }

        Ok(result.get("result").cloned().unwrap_or(json!({})))
    }

    /// Auto-select model based on task
    fn select_model(&self, messages: &[ChatMessage], has_tools: bool) -> String {
        if !self.config.auto_routing {
            return self.config.default_model.clone();
        }

        let total_length: usize = messages.iter().map(|m| m.content.len()).sum();
        let has_code = messages.iter().any(|m| {
            m.content.contains("```") ||
            m.content.contains("fn ") ||
            m.content.contains("def ") ||
            m.content.contains("class ")
        });
        let needs_reasoning = messages.iter().any(|m| {
            let lower = m.content.to_lowercase();
            lower.contains("think") ||
            lower.contains("reason") ||
            lower.contains("step by step") ||
            lower.contains("analyze")
        });

        let model = if has_tools {
            "gemini-2.0-flash"  // Best for tool use
        } else if needs_reasoning {
            "gemini-2.0-flash-thinking-exp-01-21"  // Reasoning
        } else if has_code && total_length > 5000 {
            "gemini-1.5-pro"  // Long code context
        } else {
            "gemini-2.0-flash"  // Fast default
        };

        debug!("Auto-selected: {} (code={}, reasoning={}, len={})",
            model, has_code, needs_reasoning, total_length);

        model.to_string()
    }

    /// Convert messages to Gemini format
    fn convert_messages(&self, messages: &[ChatMessage]) -> (Vec<GeminiContent>, Option<GeminiContent>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            if msg.role == "system" {
                system_instruction = Some(GeminiContent {
                    role: None,
                    parts: vec![GeminiPart::Text { text: msg.content.clone() }],
                });
                continue;
            }

            let role = match msg.role.as_str() {
                "assistant" => "model",
                "tool" => "user",
                _ => "user",
            };

            // Handle tool calls
            if let Some(ref tool_calls) = msg.tool_calls {
                let parts: Vec<GeminiPart> = tool_calls.iter().map(|tc| {
                    GeminiPart::FunctionCall {
                        function_call: GeminiFunctionCall {
                            name: tc.name.clone(),
                            args: tc.arguments.clone(),
                        }
                    }
                }).collect();
                contents.push(GeminiContent {
                    role: Some(role.to_string()),
                    parts,
                });
            }
            // Handle tool results
            else if msg.role == "tool" {
                if let Some(ref tool_call_id) = msg.tool_call_id {
                    contents.push(GeminiContent {
                        role: Some("user".to_string()),
                        parts: vec![GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name: tool_call_id.clone(),
                                response: json!({ "result": msg.content }),
                            }
                        }],
                    });
                }
            }
            // Regular text
            else {
                contents.push(GeminiContent {
                    role: Some(role.to_string()),
                    parts: vec![GeminiPart::Text { text: msg.content.clone() }],
                });
            }
        }

        (contents, system_instruction)
    }

    /// Convert tools to Gemini format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Option<Vec<GeminiTool>> {
        if tools.is_empty() {
            return None;
        }

        let declarations: Vec<GeminiFunctionDeclaration> = tools.iter().map(|t| {
            GeminiFunctionDeclaration {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            }
        }).collect();

        Some(vec![GeminiTool { function_declarations: declarations }])
    }

    /// Convert tool choice to Gemini format
    fn convert_tool_choice(&self, choice: &ToolChoice) -> Option<GeminiToolConfig> {
        let mode = match choice {
            ToolChoice::Auto => "AUTO",
            ToolChoice::None => "NONE",
            ToolChoice::Required => "ANY",
            ToolChoice::Tool(_) => "ANY",
        };

        Some(GeminiToolConfig {
            function_calling_config: FunctionCallingConfig {
                mode: mode.to_string(),
            },
        })
    }
}

#[async_trait]
impl LlmProvider for AntigravityProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Antigravity
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "gemini-2.0-flash".to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                description: Some("Fast, good for tools (FREE TIER)".to_string()),
                parameters: None,
                available: true,
                tags: vec!["fast".to_string(), "tools".to_string(), "free".to_string()],
                downloads: None,
                updated_at: None,
            },
            ModelInfo {
                id: "gemini-2.0-flash-thinking-exp-01-21".to_string(),
                name: "Gemini Flash Thinking".to_string(),
                description: Some("Extended reasoning (FREE TIER)".to_string()),
                parameters: None,
                available: true,
                tags: vec!["reasoning".to_string(), "free".to_string()],
                downloads: None,
                updated_at: None,
            },
            ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                description: Some("High quality, 2M context (FREE TIER)".to_string()),
                parameters: None,
                available: true,
                tags: vec!["quality".to_string(), "long-context".to_string(), "free".to_string()],
                downloads: None,
                updated_at: None,
            },
            ModelInfo {
                id: "gemini-1.5-flash".to_string(),
                name: "Gemini 1.5 Flash".to_string(),
                description: Some("Fast and efficient (FREE TIER)".to_string()),
                parameters: None,
                available: true,
                tags: vec!["fast".to_string(), "free".to_string()],
                downloads: None,
                updated_at: None,
            },
        ])
    }

    async fn search_models(&self, query: &str, limit: usize) -> Result<Vec<ModelInfo>> {
        let models = self.list_models().await?;
        let query_lower = query.to_lowercase();
        Ok(models
            .into_iter()
            .filter(|m| m.id.to_lowercase().contains(&query_lower) ||
                       m.name.to_lowercase().contains(&query_lower))
            .take(limit)
            .collect())
    }

    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>> {
        let models = self.list_models().await?;
        Ok(models.into_iter().find(|m| m.id == model_id))
    }

    async fn is_model_available(&self, _model_id: &str) -> Result<bool> {
        Ok(true)
    }

    async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        let request = ChatRequest::new(messages);
        self.chat_with_request(model, request).await
    }

    async fn chat_with_request(&self, model: &str, mut request: ChatRequest) -> Result<ChatResponse> {
        // Load MCP tools if agentic mode and no tools provided
        if self.config.agentic_mode && request.tools.is_empty() {
            if let Ok(tools) = self.load_mcp_tools().await {
                request.tools = tools;
            }
        }

        // Auto-select model
        let actual_model = if model == "auto" || model.is_empty() {
            self.select_model(&request.messages, !request.tools.is_empty())
        } else {
            model.to_string()
        };

        // Build URL: {BASE}/models/{model}:generateContent?key={KEY}
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            GEMINI_API_BASE,
            actual_model,
            self.config.api_key
        );

        // Convert to Gemini format
        let (contents, system_instruction) = self.convert_messages(&request.messages);
        let tools = self.convert_tools(&request.tools);
        let tool_config = if tools.is_some() {
            self.convert_tool_choice(&request.tool_choice)
        } else {
            None
        };

        let gemini_request = GeminiRequest {
            contents,
            system_instruction,
            tools,
            tool_config,
            generation_config: Some(GeminiGenerationConfig {
                temperature: request.temperature,
                top_p: request.top_p,
                max_output_tokens: request.max_tokens.map(|t| t as u32),
            }),
        };

        debug!("Antigravity request to: {}", url.split('?').next().unwrap_or(&url));

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&gemini_request)
            .send()
            .await
            .context("Failed to send Gemini request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Gemini API error {}: {}", status, body));
        }

        let result: GeminiResponse = response.json().await
            .context("Failed to parse Gemini response")?;

        if let Some(error) = result.error {
            return Err(anyhow::anyhow!("Gemini API error {}: {}", error.code, error.message));
        }

        // Parse response
        let candidate = result.candidates
            .and_then(|c| c.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("No response from Gemini"))?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for part in candidate.content.parts {
            match part {
                GeminiPart::Text { text } => text_parts.push(text),
                GeminiPart::FunctionCall { function_call } => {
                    tool_calls.push(ToolCallInfo {
                        id: format!("call_{}", tool_calls.len()),
                        name: function_call.name,
                        arguments: function_call.args,
                    });
                }
                _ => {}
            }
        }

        let text = text_parts.join("");
        let tool_calls_opt = if tool_calls.is_empty() { None } else { Some(tool_calls.clone()) };

        let usage = result.usage_metadata.map(|u| TokenUsage {
            prompt_tokens: u.prompt_token_count.unwrap_or(0),
            completion_tokens: u.candidates_token_count.unwrap_or(0),
            total_tokens: u.total_token_count.unwrap_or(0),
        });

        Ok(ChatResponse {
            message: ChatMessage {
                role: "assistant".to_string(),
                content: text,
                tool_calls: tool_calls_opt.clone(),
                tool_call_id: None,
            },
            model: actual_model,
            provider: "antigravity".to_string(),
            finish_reason: candidate.finish_reason,
            usage,
            tool_calls: tool_calls_opt,
        })
    }

    async fn chat_stream(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<String>>> {
        let response = self.chat(model, messages).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(Ok(response.message.content)).await?;
        Ok(rx)
    }
}

// =============================================================================
// AGENTIC SESSION
// =============================================================================

/// Agentic session with memory and context
#[derive(Debug, Clone)]
pub struct AgenticSession {
    pub session_id: String,
    pub user: Option<GoogleUser>,
    /// Active MCP tools
    pub tools: Vec<String>,
    /// Session memory (via memory agent)
    pub memory: std::collections::HashMap<String, Value>,
    /// Conversation context
    pub context: Value,
}

impl AgenticSession {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            user: None,
            tools: vec![
                "memory_remember".to_string(),
                "memory_recall".to_string(),
                "context_manager_save".to_string(),
                "context_manager_load".to_string(),
                "sequential_thinking_think".to_string(),
            ],
            memory: std::collections::HashMap::new(),
            context: json!({}),
        }
    }

    pub fn with_user(mut self, user: GoogleUser) -> Self {
        self.user = Some(user);
        self
    }

    pub fn remember(&mut self, key: &str, value: Value) {
        self.memory.insert(key.to_string(), value);
    }

    pub fn recall(&self, key: &str) -> Option<&Value> {
        self.memory.get(key)
    }
}
```

---

## Step 3: Add ProviderType

Update `crates/op-llm/src/provider.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderType {
    Ollama,
    OpenAI,
    Anthropic,
    Gemini,      // Vertex AI (expensive)
    Antigravity, // Standard Gemini API (free tier)
    HuggingFace,
    Perplexity,
}
```

---

## Step 4: Update lib.rs

```rust
pub mod antigravity;
pub use antigravity::{AntigravityProvider, AntigravityConfig, AgenticSession, GoogleUser};
```

---

## Step 5: Environment Configuration

### `/etc/op-dbus/environment`

```bash
# =============================================================================
# ANTIGRAVITY - Agentic AI Without Vertex
# =============================================================================

# LLM Provider (use antigravity, NOT gemini)
LLM_PROVIDER=antigravity

# Gemini API Key (get FREE at https://aistudio.google.com/)
GEMINI_API_KEY=your-api-key-here

# Model (all FREE TIER)
ANTIGRAVITY_MODEL=gemini-2.0-flash
ANTIGRAVITY_AUTO_ROUTING=true

# Agentic features (uses YOUR MCP servers)
ANTIGRAVITY_AGENTIC=true
MCP_SERVER_URL=http://localhost:8080/mcp/agents

# Google OAuth (OPTIONAL - for user identity only)
# GOOGLE_CLIENT_ID=xxx.apps.googleusercontent.com
# GOOGLE_CLIENT_SECRET=xxx
```

---

## Usage

```rust
use op_llm::antigravity::{AntigravityProvider, AgenticSession};
use op_llm::provider::{ChatMessage, LlmProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create provider (uses FREE Gemini API)
    let provider = AntigravityProvider::from_env()?;
    
    // Load your MCP tools
    let tools = provider.load_mcp_tools().await?;
    println!("Loaded {} agentic tools", tools.len());
    
    // Chat with auto model selection
    let response = provider.chat("auto", vec![
        ChatMessage::user("Use the rust_pro agent to check this project"),
    ]).await?;
    
    // Handle tool calls
    if let Some(tool_calls) = &response.tool_calls {
        for tc in tool_calls {
            println!("Executing: {}({})", tc.name, tc.arguments);
            let result = provider.execute_mcp_tool(&tc.name, tc.arguments.clone()).await?;
            println!("Result: {}", result);
        }
    }
    
    Ok(())
}
```

---

## Summary

| Feature | Antigravity | Gemini (Vertex) |
|---------|-------------|------------------|
| **API** | generativelanguage.googleapis.com | {region}-aiplatform.googleapis.com |
| **Auth** | API key | Service account / OAuth |
| **Cost** | FREE (15 RPM, 1M tokens/day) | $$$$ (pay per token) |
| **Setup** | Get key from aistudio.google.com | GCP project + billing + API enable |
| **Agentic** | ✅ Via your MCP servers | ✅ Via your MCP servers |
| **Best For** | Development, free tier | Production, high volume |

**Antigravity = FREE agentic AI using your existing op-dbus infrastructure.**
