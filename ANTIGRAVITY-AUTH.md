# Antigravity / IDE Authentication Integration

## Overview

Use your Code Assist Enterprise subscription (Cursor, Windsurf, etc.) as the LLM backend for op-dbus chatbot.

## Architecture Options

### Option A: Direct API Key Extraction

If your enterprise subscription provides API keys:

```bash
# Cursor stores some config here
~/.cursor/User/globalStorage/
~/.cursor/Machine/settings.json

# Check for API keys in environment
env | grep -iE 'anthropic|openai|cursor|windsurf'

# Check keyring (Linux)
secret-tool search service cursor
```

### Option B: MCP Bridge (Recommended)

Create a VS Code/Cursor extension that:
1. Runs inside Cursor with enterprise auth
2. Exposes an MCP server on localhost
3. op-dbus connects to it as an MCP client

```
┌────────────────────────────────────────────────────────────┐
│                    Cursor / Windsurf                       │
│  (Enterprise Auth)                                         │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐ │
│  │         op-dbus-bridge Extension                      │ │
│  │                                                       │ │
│  │  - Intercepts: chat/completions requests             │ │
│  │  - Exposes: MCP server on localhost:3333             │ │
│  │  - Routes: LLM calls through IDE's auth              │ │
│  └──────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
                              │
                              │ MCP (stdio or SSE)
                              ▼
┌────────────────────────────────────────────────────────────┐
│                    op-dbus chatbot                         │
│                                                            │
│  LLM_PROVIDER=mcp                                          │
│  MCP_LLM_SERVER=localhost:3333                             │
└────────────────────────────────────────────────────────────┘
```

---

## Implementation: MCP Bridge Extension

### package.json

```json
{
  "name": "op-dbus-bridge",
  "displayName": "OP-DBUS LLM Bridge",
  "version": "0.1.0",
  "engines": { "vscode": "^1.80.0" },
  "activationEvents": ["onStartupFinished"],
  "main": "./out/extension.js",
  "contributes": {
    "commands": [
      {
        "command": "op-dbus-bridge.start",
        "title": "Start OP-DBUS Bridge"
      },
      {
        "command": "op-dbus-bridge.stop",
        "title": "Stop OP-DBUS Bridge"
      }
    ]
  }
}
```

### extension.ts

```typescript
import * as vscode from 'vscode';
import * as http from 'http';

let server: http.Server | null = null;

export function activate(context: vscode.ExtensionContext) {
    const startCmd = vscode.commands.registerCommand('op-dbus-bridge.start', () => {
        startBridge();
    });
    
    const stopCmd = vscode.commands.registerCommand('op-dbus-bridge.stop', () => {
        stopBridge();
    });
    
    context.subscriptions.push(startCmd, stopCmd);
    
    // Auto-start on activation
    startBridge();
}

function startBridge() {
    if (server) {
        vscode.window.showInformationMessage('Bridge already running');
        return;
    }
    
    server = http.createServer(async (req, res) => {
        if (req.method === 'POST' && req.url === '/v1/chat/completions') {
            let body = '';
            req.on('data', chunk => body += chunk);
            req.on('end', async () => {
                try {
                    const request = JSON.parse(body);
                    const response = await handleChatRequest(request);
                    res.writeHead(200, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify(response));
                } catch (e) {
                    res.writeHead(500);
                    res.end(JSON.stringify({ error: String(e) }));
                }
            });
        } else if (req.method === 'GET' && req.url === '/health') {
            res.writeHead(200);
            res.end(JSON.stringify({ status: 'ok' }));
        } else {
            res.writeHead(404);
            res.end();
        }
    });
    
    server.listen(3333, '127.0.0.1', () => {
        vscode.window.showInformationMessage('OP-DBUS Bridge running on port 3333');
    });
}

function stopBridge() {
    if (server) {
        server.close();
        server = null;
        vscode.window.showInformationMessage('OP-DBUS Bridge stopped');
    }
}

async function handleChatRequest(request: any): Promise<any> {
    // Use VS Code's language model API (if available in Cursor)
    // This proxies through the IDE's authenticated session
    
    const messages = request.messages || [];
    const model = request.model || 'claude-3-5-sonnet';
    
    // Option 1: Use vscode.lm API (VS Code 1.90+)
    try {
        const models = await vscode.lm.selectChatModels({ family: 'claude' });
        if (models.length > 0) {
            const chatModel = models[0];
            const vsMessages = messages.map((m: any) => 
                m.role === 'user' 
                    ? vscode.LanguageModelChatMessage.User(m.content)
                    : vscode.LanguageModelChatMessage.Assistant(m.content)
            );
            
            const response = await chatModel.sendRequest(vsMessages, {});
            let content = '';
            for await (const chunk of response.text) {
                content += chunk;
            }
            
            return {
                id: `bridge-${Date.now()}`,
                object: 'chat.completion',
                model: model,
                choices: [{
                    index: 0,
                    message: { role: 'assistant', content },
                    finish_reason: 'stop'
                }]
            };
        }
    } catch (e) {
        console.error('vscode.lm API failed:', e);
    }
    
    // Option 2: Use Cursor's internal API (if in Cursor)
    // This requires reverse-engineering Cursor's internal commands
    try {
        // Cursor exposes some commands for chat
        const result = await vscode.commands.executeCommand(
            'cursor.chat.sendMessage',
            messages[messages.length - 1]?.content || ''
        );
        if (result) {
            return {
                id: `bridge-${Date.now()}`,
                object: 'chat.completion',
                model: model,
                choices: [{
                    index: 0,
                    message: { role: 'assistant', content: String(result) },
                    finish_reason: 'stop'
                }]
            };
        }
    } catch (e) {
        console.error('Cursor command failed:', e);
    }
    
    throw new Error('No LLM backend available');
}

export function deactivate() {
    stopBridge();
}
```

---

## Implementation: Antigravity Provider

Add to `crates/op-llm/src/antigravity.rs`:

```rust
//! Antigravity Provider - Connects to IDE's LLM via local bridge
//!
//! This provider connects to a local HTTP server (the bridge extension)
//! that proxies requests through the IDE's authenticated session.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, LlmProvider, ModelInfo,
    ProviderType, TokenUsage, ToolCallInfo, ToolChoice, ToolDefinition,
};

const DEFAULT_BRIDGE_URL: &str = "http://127.0.0.1:3333";

/// Antigravity provider configuration
#[derive(Debug, Clone)]
pub struct AntigravityConfig {
    pub bridge_url: String,
    pub default_model: String,
}

impl Default for AntigravityConfig {
    fn default() -> Self {
        Self {
            bridge_url: std::env::var("ANTIGRAVITY_BRIDGE_URL")
                .unwrap_or_else(|_| DEFAULT_BRIDGE_URL.to_string()),
            default_model: std::env::var("ANTIGRAVITY_MODEL")
                .unwrap_or_else(|_| "claude-3-5-sonnet".to_string()),
        }
    }
}

/// Antigravity LLM provider
pub struct AntigravityProvider {
    client: Client,
    config: AntigravityConfig,
}

impl AntigravityProvider {
    pub fn new(config: AntigravityConfig) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to create HTTP client"),
            config,
        }
    }

    pub fn from_env() -> Self {
        Self::new(AntigravityConfig::default())
    }

    /// Check if bridge is available
    pub async fn is_bridge_available(&self) -> bool {
        match self.client
            .get(format!("{}/health", self.config.bridge_url))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIFunction,
}

#[derive(Serialize, Deserialize)]
struct OpenAIFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    id: String,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl LlmProvider for AntigravityProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Antigravity
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Bridge doesn't list models - return known models
        Ok(vec![
            ModelInfo {
                id: "claude-3-5-sonnet".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: Some("Via IDE bridge".to_string()),
                parameters: None,
                available: self.is_bridge_available().await,
                tags: vec!["chat".to_string(), "code".to_string()],
                downloads: None,
                updated_at: None,
            },
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                description: Some("Via IDE bridge".to_string()),
                parameters: None,
                available: self.is_bridge_available().await,
                tags: vec!["chat".to_string(), "code".to_string()],
                downloads: None,
                updated_at: None,
            },
        ])
    }

    async fn search_models(&self, query: &str, _limit: usize) -> Result<Vec<ModelInfo>> {
        let models = self.list_models().await?;
        Ok(models
            .into_iter()
            .filter(|m| m.name.to_lowercase().contains(&query.to_lowercase()))
            .collect())
    }

    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>> {
        let models = self.list_models().await?;
        Ok(models.into_iter().find(|m| m.id == model_id))
    }

    async fn is_model_available(&self, _model_id: &str) -> Result<bool> {
        Ok(self.is_bridge_available().await)
    }

    async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        let request = ChatRequest {
            messages,
            tools: vec![],
            tool_choice: ToolChoice::Auto,
            max_tokens: None,
            temperature: None,
            top_p: None,
        };
        self.chat_with_request(model, request).await
    }

    async fn chat_with_request(&self, model: &str, request: ChatRequest) -> Result<ChatResponse> {
        let model = if model.is_empty() {
            &self.config.default_model
        } else {
            model
        };

        // Convert messages
        let messages: Vec<OpenAIMessage> = request
            .messages
            .iter()
            .map(|m| OpenAIMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                tool_calls: m.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| OpenAIToolCall {
                            id: tc.id.clone(),
                            call_type: "function".to_string(),
                            function: OpenAIFunction {
                                name: tc.name.clone(),
                                arguments: tc.arguments.to_string(),
                            },
                        })
                        .collect()
                }),
                tool_call_id: m.tool_call_id.clone(),
            })
            .collect();

        // Convert tools
        let tools: Option<Vec<Value>> = if request.tools.is_empty() {
            None
        } else {
            Some(request.tools.iter().map(|t| t.to_openai_format()).collect())
        };

        // Convert tool_choice
        let tool_choice = if request.tools.is_empty() {
            None
        } else {
            Some(request.tool_choice.to_api_format())
        };

        let api_request = OpenAIRequest {
            model: model.to_string(),
            messages,
            tools,
            tool_choice,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        };

        debug!("Sending request to bridge: {}", self.config.bridge_url);

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.config.bridge_url))
            .json(&api_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Bridge error {}: {}", status, text);
        }

        let api_response: OpenAIResponse = response.json().await?;

        let choice = api_response
            .choices
            .first()
            .ok_or_else(|| anyhow::anyhow!("No response choices"))?;

        // Parse tool calls
        let tool_calls = choice.message.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| ToolCallInfo {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    arguments: serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(Value::Null),
                })
                .collect()
        });

        Ok(ChatResponse {
            message: ChatMessage {
                role: choice.message.role.clone(),
                content: choice.message.content.clone(),
                tool_calls: tool_calls.clone(),
                tool_call_id: None,
            },
            model: api_response.model,
            provider: "antigravity".to_string(),
            finish_reason: choice.finish_reason.clone(),
            usage: api_response.usage.map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
            tool_calls,
        })
    }

    async fn chat_stream(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<String>>> {
        // For now, fall back to non-streaming
        let response = self.chat(model, messages).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(Ok(response.message.content)).await?;
        Ok(rx)
    }
}
```

---

## Configuration

### /etc/op-dbus/environment

```bash
# Use Antigravity (IDE bridge) as LLM provider
LLM_PROVIDER=antigravity

# Bridge configuration
ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333
ANTIGRAVITY_MODEL=claude-3-5-sonnet

# Fallback if bridge unavailable
FALLBACK_LLM_PROVIDER=gemini
```

---

## Usage Flow

1. **Install the bridge extension** in Cursor/VS Code
2. **Open Cursor** (authenticates with your enterprise account)
3. **Start the bridge** via command palette: "Start OP-DBUS Bridge"
4. **Start op-dbus** - it connects to localhost:3333
5. **Chat** - requests go through Cursor's auth

```
User → op-dbus → localhost:3333 → Cursor Extension → Cursor's Auth → Claude/GPT
```

---

## Alternative: Direct Cursor Database Integration

If you want to inspect Cursor's stored data:

```bash
# List tables in Cursor's chat DB
sqlite3 ~/.cursor/chats/*/store.db ".tables"

# Get schema
sqlite3 ~/.cursor/chats/*/store.db ".schema"

# Note: Auth tokens are NOT in SQLite - they're in:
# - macOS: Keychain
# - Linux: libsecret/GNOME Keyring
# - Windows: Credential Manager
```

To extract from Linux keyring:

```bash
# List Cursor secrets
secret-tool search service cursor

# Or use Python
python3 << 'EOF'
import secretstorage
bus = secretstorage.dbus_init()
collection = secretstorage.get_default_collection(bus)
for item in collection.get_all_items():
    if 'cursor' in str(item.get_attributes()).lower():
        print(item.get_label(), item.get_attributes())
EOF
```

---

## Security Considerations

1. **Bridge only listens on localhost** - not exposed to network
2. **No credentials stored** - uses IDE's existing auth
3. **Terms of Service** - check your enterprise agreement for API usage policies
4. **Rate limits** - enterprise subscriptions may have limits

---

## Testing

```bash
# Check if bridge is running
curl http://localhost:3333/health

# Test chat completion
curl -X POST http://localhost:3333/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet",
    "messages": [{"role": "user", "content": "Say hello"}]
  }'
```
