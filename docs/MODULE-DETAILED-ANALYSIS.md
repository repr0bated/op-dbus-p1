# Detailed Module Analysis - op-dbus-v2 Consolidation

> **Generated:** Dec 31, 2024
> **Total Modules Analyzed:** 15
> **Lines of Code:** ~9,000

---

# Table of Contents

1. [external_client.rs](#1-external_clientrs) - MCP Server Aggregation
2. [http_server.rs](#2-http_serverrs) - HTTP-to-MCP Bridge
3. [lazy_tools.rs](#3-lazy_toolsrs) - Lazy Tool Loading (COMMENTED)
4. [server.rs](#4-serverrs) - MCP JSON-RPC Server (COMMENTED)
5. [router.rs](#5-routerrs) - HTTP Router (COMMENTED)
6. [config.rs](#6-configrs) - Configuration (COMMENTED)
7. [tool_adapter.rs](#7-tool_adapterrs) - Tool Aggregation (COMMENTED)
8. [tool_adapter_orchestrated.rs](#8-tool_adapter_orchestratedrs) - Orchestration (COMMENTED)
9. [hybrid_scanner.rs](#9-hybrid_scannerrs) - System Scanner
10. [consolidated_introspection.rs](#10-consolidated_introspectionrs) - Unified Introspection
11. [comprehensive_introspection.rs](#11-comprehensive_introspectionrs) - D-Bus Introspection
12. [native_introspection.rs](#12-native_introspectionrs) - XML Parsing
13. [system_introspection.rs](#13-system_introspectionrs) - System Discovery
14. [json_introspection.rs](#14-json_introspectionrs) - JSON Conversion
15. [workflow_plugin_introspection.rs](#15-workflow_plugin_introspectionrs) - Workflow Discovery

---

# 1. external_client.rs

**Status:** INTEGRATED (Compiles Successfully)
**Location:** `crates/op-mcp/src/external_client.rs`
**Lines:** 455

## 1.1 Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           ExternalMcpManager                                 │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │              clients: RwLock<HashMap<String, ExternalMcpClient>>      │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                    │                                         │
│         ┌──────────────────────────┼──────────────────────────┐             │
│         ▼                          ▼                          ▼             │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐       │
│  │ ExternalMcpClient│     │ ExternalMcpClient│     │ ExternalMcpClient│       │
│  │ name: "github"  │     │ name: "filesystem"│     │ name: "postgres"│       │
│  │ command: "npx"  │     │ command: "npx"   │     │ command: "npx"  │       │
│  │ process: Child  │     │ process: Child   │     │ process: Child  │       │
│  │ stdin/stdout    │     │ stdin/stdout     │     │ stdin/stdout    │       │
│  │ tools: [...]    │     │ tools: [...]     │     │ tools: [...]    │       │
│  └────────┬────────┘     └────────┬────────┘     └────────┬────────┘       │
│           │                       │                       │                 │
│           ▼                       ▼                       ▼                 │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    JSON-RPC 2.0 over stdio                           │   │
│  │  {"jsonrpc":"2.0","method":"tools/list"} → {"result":{"tools":[...]}}│   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 1.2 Intended Functionality

### Primary Purpose
Connect to, manage, and aggregate multiple external MCP servers (like GitHub MCP, Filesystem MCP, Postgres MCP) into a unified tool interface.

### Core Capabilities
| Capability | Description |
|------------|-------------|
| **Process Management** | Spawns and manages MCP server subprocesses |
| **Protocol Handling** | Implements MCP JSON-RPC 2.0 over stdio |
| **Tool Aggregation** | Collects tools from all servers with namespace prefixes |
| **Authentication** | Supports multiple auth methods for API keys |
| **Lifecycle Management** | Handles startup, initialization, shutdown, retries |
| **Error Recovery** | Timeout handling, retry logic, graceful degradation |

## 1.3 Components

### Structs

| Struct | Fields | Purpose |
|--------|--------|---------|
| `ExternalMcpConfig` | name, command, args, env, api_key, api_key_env, auth_method, headers | Server configuration |
| `ExternalTool` | name, description, input_schema, server_name | Tool metadata with namespace |
| `ExternalMcpClient` | config, process, stdin, stdout, tools, next_id | Single server connection |
| `ExternalMcpManager` | clients: HashMap | Multi-server orchestrator |

### Enums

| Enum | Variants | Purpose |
|------|----------|---------|
| `AuthMethod` | None, EnvVar, BearerToken, CustomHeader | Authentication strategy |

### Functions

| Function | Signature | Purpose |
|----------|-----------|---------|
| `ExternalMcpClient::new` | `(config) -> Self` | Create client instance |
| `ExternalMcpClient::start` | `(&mut self) -> Result<()>` | Spawn process, initialize, load tools |
| `ExternalMcpClient::initialize` | `(&mut self) -> Result<()>` | Send MCP initialize request |
| `ExternalMcpClient::refresh_tools` | `(&mut self) -> Result<()>` | Fetch tools/list from server |
| `ExternalMcpClient::get_tools` | `(&self) -> Vec<ExternalTool>` | Return cached tools |
| `ExternalMcpClient::call_tool` | `(&mut self, name, args) -> Result<Value>` | Execute tool on server |
| `ExternalMcpClient::send_request` | `(&mut self, request) -> Result<Value>` | Low-level JSON-RPC send/receive |
| `ExternalMcpClient::stop` | `(&mut self) -> Result<()>` | Kill subprocess |
| `ExternalMcpManager::new` | `() -> Self` | Create manager |
| `ExternalMcpManager::add_server` | `(&self, config) -> Result<()>` | Add and start server |
| `ExternalMcpManager::load_from_file` | `(&self, path) -> Result<()>` | Load configs from JSON file |
| `ExternalMcpManager::get_all_tools` | `(&self) -> Vec<ExternalTool>` | Aggregate all tools |
| `ExternalMcpManager::call_tool` | `(&self, name, args) -> Result<Value>` | Route call to correct server |
| `ExternalMcpManager::stop_all` | `(&self) -> Result<()>` | Shutdown all servers |

## 1.4 Issues

| Issue | Severity | Description |
|-------|----------|-------------|
| **Fixed: Missing start_time** | Critical | Added `let start_time = std::time::Instant::now();` |
| **Single-request subprocess** | Minor | Each HTTP request spawns new MCP process (by design for http_server.rs) |
| **No connection pooling** | Minor | Each client maintains single connection |
| **Blocking on tool call** | Minor | `call_tool` requires mutable borrow, blocking concurrent calls |

## 1.5 Dependencies

### External Crates
```toml
anyhow = "1.0"           # Error handling
serde = "1.0"            # Serialization
serde_json = "1.0"       # JSON handling
tokio = "1"              # Async runtime (process, io-util, sync)
tracing = "0.1"          # Logging
```

### Internal Dependencies
None - self-contained module

### Relationship to Other Modules
- **Used by:** `http_server.rs` (indirectly), `tool_adapter.rs`
- **Uses:** None

## 1.6 Deployment Requirements

### Prerequisites
1. External MCP servers must be installed:
   ```bash
   npm install -g @modelcontextprotocol/server-github
   npm install -g @modelcontextprotocol/server-filesystem
   npm install -g @modelcontextprotocol/server-postgres
   ```

2. API keys configured as environment variables:
   ```bash
   export GITHUB_PERSONAL_ACCESS_TOKEN="ghp_..."
   export POSTGRES_CONNECTION_STRING="postgres://..."
   ```

### Configuration File Format
```json
[
  {
    "name": "github",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-github"],
    "api_key_env": "GITHUB_PERSONAL_ACCESS_TOKEN",
    "auth_method": "env_var"
  },
  {
    "name": "filesystem",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"],
    "auth_method": "none"
  }
]
```

### Deployment Procedure
1. Create MCP config file at `/etc/op-dbus/mcp-servers.json`
2. Ensure all MCP server commands are in PATH
3. Set required environment variables
4. Call `manager.load_from_file("/etc/op-dbus/mcp-servers.json")`

## 1.7 Ratings

| Criteria | Rating | Justification |
|----------|--------|---------------|
| **Usefulness** | ★★★★★ (5/5) | Essential for MCP ecosystem integration |
| **Code Quality** | ★★★★☆ (4/5) | Clean async code, good error handling, minor concurrency issue |
| **Implementation** | ★★★★☆ (4/5) | Robust retry logic, proper resource cleanup via Drop |
| **Documentation** | ★★★☆☆ (3/5) | Doc comments present but sparse |
| **Test Coverage** | ★☆☆☆☆ (1/5) | No tests included |

**Overall: ★★★★☆ (4/5)**

---

# 2. http_server.rs

**Status:** INTEGRATED (Compiles Successfully)
**Location:** `crates/op-mcp/src/http_server.rs`
**Lines:** 399

## 2.1 Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HttpMcpServer                                   │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                         Axum Router                                     │ │
│  │  ┌────────────────────────────────────────────────────────────────┐    │ │
│  │  │  Routes:                                                        │    │ │
│  │  │  GET  /          → handle_sse (SSE connection)                 │    │ │
│  │  │  POST /          → handle_mcp_request (JSON-RPC)               │    │ │
│  │  │  GET  /health    → health_check                                │    │ │
│  │  │  POST /mcp       → handle_mcp_request                          │    │ │
│  │  │  POST /initialize→ handle_initialize                           │    │ │
│  │  │  POST /tools/list→ handle_tools_list                           │    │ │
│  │  │  POST /tools/call→ handle_tools_call                           │    │ │
│  │  │  GET  /sse       → handle_sse                                  │    │ │
│  │  └────────────────────────────────────────────────────────────────┘    │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                     │                                        │
│                                     ▼                                        │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                         call_mcp()                                      │ │
│  │  1. Serialize request to JSON                                          │ │
│  │  2. Spawn subprocess: mcp_command (e.g., op-mcp-server)               │ │
│  │  3. Write request to stdin                                             │ │
│  │  4. Read response from stdout                                          │ │
│  │  5. Collect stderr for logging                                         │ │
│  │  6. Wait for process exit                                              │ │
│  │  7. Parse and return response                                          │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘

SSE Event Flow:
┌─────────────────────────────────────────────────────────────────────────────┐
│  Client connects to /sse                                                     │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  Initial Events (sent immediately):                                   │   │
│  │  1. endpoint: "/mcp"                                                  │   │
│  │  2. chat_control: { sseUrl, postUrl } (if configured)                │   │
│  │  3. tools: { name, count, tools: [...] }                             │   │
│  │  4. agents: { name, count, agents: [...] }                           │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                     │                                        │
│                                     ▼                                        │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  Keep-alive Events (every 30 seconds):                                │   │
│  │  ping: { counter: N }                                                 │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 2.2 Intended Functionality

### Primary Purpose
Expose MCP functionality over HTTP/SSE for remote clients (Antigravity IDE, web browsers, remote agents).

### Core Capabilities
| Capability | Description |
|------------|-------------|
| **HTTP Bridge** | Converts HTTP POST requests to MCP JSON-RPC |
| **SSE Streaming** | Real-time event stream for tool/agent discovery |
| **Process Proxy** | Spawns MCP subprocess for each request |
| **Agent Discovery** | Exposes op-agents registry via SSE |
| **Chat Control** | Optional integration with chat control MCP |
| **Health Monitoring** | `/health` endpoint for load balancers |

## 2.3 Components

### Structs

| Struct | Fields | Purpose |
|--------|--------|---------|
| `HttpMcpServer` | mcp_command, chat_control | Main server configuration |
| `McpRequest` | jsonrpc, id, method, params | JSON-RPC request |
| `McpResponse` | jsonrpc, id, result, error | JSON-RPC response |
| `ChatControlConfig` | name, description, sse_url, post_url | Optional chat control |

### Handler Functions

| Handler | Route | Method | Purpose |
|---------|-------|--------|---------|
| `health_check` | `/health` | GET | Health status |
| `handle_mcp_request` | `/`, `/mcp` | POST | Generic MCP request |
| `handle_initialize` | `/initialize` | POST | MCP initialization |
| `handle_tools_list` | `/tools/list` | POST | List available tools |
| `handle_tools_call` | `/tools/call` | POST | Execute a tool |
| `handle_sse` | `/`, `/sse` | GET | SSE event stream |

### Event Generation Methods

| Method | Event Type | Payload |
|--------|------------|---------|
| `endpoint_event` | `endpoint` | `"/mcp"` |
| `chat_control_event` | `chat_control` | `{ name, sseUrl, postUrl }` |
| `agents_event` | `agents` | `{ name, count, agents }` |
| `snapshot_tools_event` | `tools` | `{ name, count, tools }` |

## 2.4 Issues

| Issue | Severity | Description |
|-------|----------|-------------|
| **Process-per-request** | Medium | Spawns new MCP process for every request (high overhead) |
| **No request timeout** | Medium | `call_mcp` has no timeout, could hang indefinitely |
| **Environment leak** | Low | Inherits all env vars to subprocess (potential security) |
| **No auth** | Medium | No authentication on HTTP endpoints |

## 2.5 Dependencies

### External Crates
```toml
axum = "0.7"             # Web framework
futures = "0.3"          # Stream utilities
serde = "1.0"            # Serialization
serde_json = "1.0"       # JSON
tokio = "1"              # Async runtime
tracing = "0.1"          # Logging
```

### Internal Dependencies
```rust
use op_agents::list_agent_types;  # Agent registry
```

## 2.6 Deployment Requirements

### Prerequisites
1. MCP server binary must exist (e.g., `op-mcp-server`)
2. op-agents crate must be available

### Environment Variables
| Variable | Required | Description |
|----------|----------|-------------|
| `CHAT_CONTROL_MCP_BASE_URL` | No | Base URL for chat control MCP |
| `CHAT_CONTROL_MCP_SSE_URL` | No | SSE URL override |
| `CHAT_CONTROL_MCP_POST_URL` | No | POST URL override |
| `CHAT_CONTROL_MCP_NAME` | No | Display name |
| `CHAT_CONTROL_MCP_DESCRIPTION` | No | Description |

### Deployment Procedure
1. Build `op-mcp-server` binary
2. Configure environment variables
3. Create HttpMcpServer with command path:
   ```rust
   let server = HttpMcpServer::new(vec![
       "/path/to/op-mcp-server".to_string()
   ]);
   let router = server.router();
   // Mount router at desired path
   ```

## 2.7 Ratings

| Criteria | Rating | Justification |
|----------|--------|---------------|
| **Usefulness** | ★★★★★ (5/5) | Critical for remote MCP access |
| **Code Quality** | ★★★★☆ (4/5) | Clean Axum patterns, good SSE handling |
| **Implementation** | ★★★☆☆ (3/5) | Process-per-request is inefficient |
| **Documentation** | ★★★☆☆ (3/5) | Route documentation good |
| **Test Coverage** | ★☆☆☆☆ (1/5) | No tests |

**Overall: ★★★★☆ (4/5)**

---

# 3. lazy_tools.rs

**Status:** COMMENTED (Missing Dependencies)
**Location:** `crates/op-mcp/src/lazy_tools.rs`
**Lines:** 503

## 3.1 Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           LazyToolManager                                    │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                    ToolRegistry (LRU Cache)                             │ │
│  │  ┌──────────────────────────────────────────────────────────────────┐  │ │
│  │  │  Configuration:                                                   │  │ │
│  │  │  - max_loaded_tools: 50 (configurable)                           │  │ │
│  │  │  - min_idle_time: 300s (5 minutes)                               │  │ │
│  │  │  - hot_threshold: 10 (usage count to prevent eviction)           │  │ │
│  │  │  - eviction_check_interval: 10s                                  │  │ │
│  │  └──────────────────────────────────────────────────────────────────┘  │ │
│  │                                                                         │ │
│  │  Tool Lifecycle:                                                        │ │
│  │  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐            │ │
│  │  │UNLOADED │───►│ LOADING │───►│ LOADED  │───►│ EVICTED │            │ │
│  │  └─────────┘    └─────────┘    └─────────┘    └─────────┘            │ │
│  │       ▲                              │              │                  │ │
│  │       └──────────────────────────────┴──────────────┘                  │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                     │                                        │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                    ToolDiscoverySystem                                  │ │
│  │  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐           │ │
│  │  │ BuiltinSource  │  │  DbusSource    │  │ PluginSource   │           │ │
│  │  │ (compiled-in)  │  │ (runtime D-Bus)│  │ (state plugins)│           │ │
│  │  └────────────────┘  └────────────────┘  └────────────────┘           │ │
│  │  ┌────────────────┐                                                    │ │
│  │  │  AgentSource   │                                                    │ │
│  │  │ (agent tools)  │                                                    │ │
│  │  └────────────────┘                                                    │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘

Tool Loading Flow:
┌─────────────────────────────────────────────────────────────────────────────┐
│  get_tool("systemd_list_units")                                              │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  1. Check Registry (already loaded?)                                 │    │
│  │     └─► YES: Return cached tool, update usage stats                 │    │
│  │     └─► NO: Continue to step 2                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  2. Check Discovery (tool definition exists?)                        │    │
│  │     └─► NO: Return None                                             │    │
│  │     └─► YES: Get category (dbus, ovs, agent, plugin)               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  3. Load Tool by Category                                            │    │
│  │     - dbus: create_systemd_tools() or create_networkmanager_tools() │    │
│  │     - ovs: create_ovs_tools()                                       │    │
│  │     - agent: AgentToolFactory (not implemented)                     │    │
│  │     - plugin: PluginStateToolFactory (not implemented)              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  4. Register in Registry (with LRU tracking)                         │    │
│  │     - Evict oldest if at capacity                                   │    │
│  │     - Track usage for hot tools                                     │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 3.2 Intended Functionality

### Primary Purpose
Provide on-demand tool loading with intelligent caching to support large tool inventories (100s-1000s of tools) without memory exhaustion.

### Core Capabilities
| Capability | Description |
|------------|-------------|
| **Lazy Loading** | Tools loaded only when first requested |
| **LRU Caching** | Least-recently-used eviction when at capacity |
| **Hot Tool Protection** | Frequently-used tools exempt from eviction |
| **Multi-Source Discovery** | D-Bus, plugins, agents, built-in tools |
| **Context Filtering** | Filter tools by conversation context |
| **Preloading** | Essential tools loaded at startup |
| **Pagination** | Support for offset/limit in tool listing |

## 3.3 Components

### Structs

| Struct | Fields | Purpose |
|--------|--------|---------|
| `LazyToolConfig` | max_loaded_tools, min_idle_secs, enable_*_discovery, preload_essential | Configuration |
| `LazyToolManager` | registry, discovery, config | Main manager |
| `ToolListResponse` | tools, total, offset, limit, has_more | Paginated response |
| `McpToolInfo` | name, description, input_schema | Tool metadata |

### Key Methods

| Method | Purpose |
|--------|---------|
| `new()` / `with_config()` | Create manager with optional config |
| `initialize_discovery()` | Register discovery sources |
| `collect_builtin_definitions()` | Gather built-in tool definitions |
| `preload_essential_tools()` | Load systemd, OVS tools at startup |
| `get_tool()` | Load tool on demand |
| `load_dbus_tool()` | Load D-Bus tool by name |
| `load_ovs_tool()` | Load OVS tool by name |
| `list_all_tools()` | Get all tool definitions (from discovery) |
| `list_loaded_tools()` | Get currently loaded tools (from registry) |
| `search_tools()` | Search by query, category, tags |
| `get_context_relevant_tools()` | Filter by conversation context |
| `stats()` | Get registry and discovery statistics |

## 3.4 Issues

### Missing Dependencies (Blocking)

```rust
// These don't exist in current op-tools:
use op_tools::{
    builtin::{create_networkmanager_tools, create_ovs_tools, create_systemd_tools},
    discovery::{
        AgentDiscoverySource, BuiltinToolSource, DbusDiscoverySource, DiscoveryStats,
        PluginDiscoverySource, ToolDiscoverySource, ToolDiscoverySystem,
    },
    registry::{LruConfig, RegistryStats, ToolDefinition, ToolRegistry},
};
```

### Required API Additions to op-tools

| API | Current Status | Needed |
|-----|----------------|--------|
| `create_systemd_tools()` | Missing | Factory function returning Vec<BoxedTool> |
| `create_networkmanager_tools()` | Missing | Factory function returning Vec<BoxedTool> |
| `create_ovs_tools()` | Missing | Factory function returning Vec<BoxedTool> |
| `ToolDiscoverySystem` | Missing | Discovery source manager |
| `LruConfig` | Missing | LRU configuration for registry |
| `ToolDefinition.namespace` | Missing | Namespace field on tool def |

## 3.5 Dependencies

### External Crates
```toml
anyhow = "1.0"
serde = "1.0"
serde_json = "1.0"
tracing = "0.1"
```

### Internal Dependencies
```rust
use op_tools::*;  # Heavy dependency on op_tools
```

## 3.6 Deployment Requirements

### When Enabled
1. op-tools crate must have discovery and builtin modules
2. D-Bus system bus access for runtime discovery
3. Optional: Plugin directory for plugin discovery
4. Optional: Agent registry for agent discovery

### Configuration
```rust
LazyToolConfig {
    max_loaded_tools: 50,      // Tune based on memory
    min_idle_secs: 300,        // 5 minute idle before eviction
    enable_dbus_discovery: true,
    enable_plugin_discovery: true,
    enable_agent_discovery: true,
    preload_essential: true,   // Load systemd/OVS at startup
}
```

## 3.7 Ratings

| Criteria | Rating | Justification |
|----------|--------|---------------|
| **Usefulness** | ★★★★★ (5/5) | Essential for scaling to many tools |
| **Code Quality** | ★★★★★ (5/5) | Excellent architecture, well-designed |
| **Implementation** | ★★★★★ (5/5) | Production-ready patterns |
| **Integration Effort** | ★★☆☆☆ (2/5) | Requires significant op-tools additions |
| **Test Coverage** | ★★★★☆ (4/5) | Has unit tests (when deps available) |

**Overall: ★★★★★ (5/5) - High priority to enable**

---

# 4. server.rs

**Status:** COMMENTED (Depends on lazy_tools.rs)
**Location:** `crates/op-mcp/src/server.rs`
**Lines:** 439

## 4.1 Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              McpServer                                       │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │  config: McpServerConfig                                                │ │
│  │    - name: "op-mcp"                                                    │ │
│  │    - version: from Cargo.toml                                          │ │
│  │    - tool_config: LazyToolConfig                                       │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │  tool_manager: Arc<LazyToolManager>                                     │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                     │                                        │
│                                     ▼                                        │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                     run_stdio() Loop                                    │ │
│  │  ┌──────────────────────────────────────────────────────────────────┐  │ │
│  │  │  while line = stdin.readline():                                   │  │ │
│  │  │      request = parse_json(line)                                   │  │ │
│  │  │      response = handle_request(request)                           │  │ │
│  │  │      stdout.write(response)                                       │  │ │
│  │  └──────────────────────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Request Routing:                                                            │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │  initialize      → handle_initialize (protocol info, capabilities)     │ │
│  │  initialized     → success({})                                          │ │
│  │  tools/list      → handle_tools_list (paginated, context-filtered)     │ │
│  │  tools/call      → handle_tools_call (lazy load + execute)             │ │
│  │  resources/list  → handle_resources_list (embedded docs)               │ │
│  │  resources/read  → handle_resources_read (serve doc content)           │ │
│  │  ping            → success({})                                          │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 4.2 Intended Functionality

### Primary Purpose
Full MCP JSON-RPC 2.0 server with lazy tool loading, running over stdio.

### Core Capabilities
| Capability | Description |
|------------|-------------|
| **MCP Protocol** | Complete JSON-RPC 2.0 over stdio |
| **Lazy Integration** | Uses LazyToolManager for tool operations |
| **Pagination** | Supports offset/limit for tools/list |
| **Context Filtering** | Filter tools by conversation context |
| **Resource Serving** | Embedded documentation resources |
| **Statistics** | Exposes registry/discovery stats in _meta |

## 4.3 Components

### Structs

| Struct | Fields | Purpose |
|--------|--------|---------|
| `McpServerConfig` | name, version, tool_config | Server configuration |
| `McpRequest` | jsonrpc, id, method, params | JSON-RPC request |
| `McpResponse` | jsonrpc, id, result, error | JSON-RPC response |
| `McpError` | code, message, data | Error structure |
| `McpServer` | config, tool_manager | Main server |

### Embedded Resources
- `docs://architecture` - Architecture documentation (ARCHITECTURE_DOC constant)
- `docs://tools` - Tool usage documentation

## 4.4 Issues

### Blocking Dependency
```rust
use crate::lazy_tools::{get_mcp_tool_list, LazyToolConfig, LazyToolManager};
```

**Cannot compile until lazy_tools.rs is fixed.**

## 4.5 Ratings

| Criteria | Rating | Justification |
|----------|--------|---------------|
| **Usefulness** | ★★★★☆ (4/5) | Standard MCP server implementation |
| **Code Quality** | ★★★★☆ (4/5) | Clean, well-structured |
| **Implementation** | ★★★★☆ (4/5) | Complete protocol support |
| **Test Coverage** | ★★★★☆ (4/5) | Has unit tests |

**Overall: ★★★★☆ (4/5)**

---

# 5-6. router.rs & config.rs

**Status:** COMMENTED
**Priority:** Low

### router.rs (244 lines)
- Duplicates functionality in http_server.rs
- Depends on non-existent `op_http::router::ServiceRouter`
- Lower priority to fix

### config.rs (32 lines)
- Simple config loading using external `config` crate
- Trivial to replicate without external dependency
- Not worth adding dependency for 32 lines

---

# 7. tool_adapter.rs

**Status:** COMMENTED (Corrupted + Missing Dependencies)
**Location:** `crates/op-mcp/src/tool_adapter.rs`
**Lines:** 494

## 7.1 Critical Issue: File Corruption

The file has line numbers embedded in content:
```
  1 | //! Tool Adapter - Bridges op-tools and external MCPs
  2 | //!
```

**Must be cleaned before use.**

## 7.2 Intended Functionality

### Primary Purpose
Bridge between local op-tools registry, external MCP servers, with security filtering.

### Security Model
```rust
const BLOCKED_PATTERNS: &[&str] = &[
    // Shell/Execution - NEVER expose
    "shell_execute",
    "write_file",

    // Systemd mutations - web-only
    "systemd_start", "systemd_stop", "systemd_restart",
    "systemd_reload", "systemd_enable", "systemd_disable", "systemd_apply",

    // OVS mutations - web-only
    "ovs_create", "ovs_delete", "ovs_add", "ovs_set",

    // Plugin mutations
    "_apply",

    // BTRFS mutations
    "btrfs_create", "btrfs_delete", "btrfs_snapshot",
];
```

### Tool Filtering (MCP_TOOL_FILTER)
| Filter Value | Included Tools |
|--------------|----------------|
| `systemd` | `dbus_systemd1_*` |
| `login` | `dbus_login1_*` |
| `ovs` | `ovs_*` |
| `agents` | `agent_*`, `list_*`, `spawn_*`, `*agent*` |
| `core` | `dbus_DBus_*`, `dbus_login1_*`, `ovs_*`, `plugin_*` |
| `skills` | `skill_*`, `workstack_*`, `workflow_*` |

## 7.3 Missing Dependencies

```rust
use op_execution_tracker::{ExecutionContext, ExecutionResult, ExecutionStatus, ExecutionTracker};
use op_dynamic_loader::{ExecutionAwareLoader, SmartLoadingStrategy};
```

These crates don't exist.

## 7.4 Ratings

| Criteria | Rating | Justification |
|----------|--------|---------------|
| **Usefulness** | ★★★★☆ (4/5) | Good security model |
| **Code Quality** | ★★☆☆☆ (2/5) | File is corrupted |
| **Implementation** | ★★★★☆ (4/5) | Well-designed when working |

**Overall: ★★★☆☆ (3/5) - Needs cleanup**

---

# 8. tool_adapter_orchestrated.rs

**Status:** COMMENTED (Missing Dependencies)
**Location:** `crates/op-mcp/src/tool_adapter_orchestrated.rs`
**Lines:** 314

## 8.1 Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      OrchestratedToolAdapter                                 │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │  tool_registry: Arc<ToolRegistry>                                       │ │
│  │  orchestrated_executor: Arc<OrchestratedExecutor>                       │ │
│  │  execution_tracker: Arc<ExecutionTracker>                               │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Execution Modes:                                                            │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                                                                         │ │
│  │  DIRECT                                                                 │ │
│  │  ├── Single tool execution                                             │ │
│  │  └── tool_name → result                                                │ │
│  │                                                                         │ │
│  │  WORKSTACK (Multi-Agent Collaboration)                                  │ │
│  │  ├── Multiple agents work together on one task                         │ │
│  │  ├── Shared context between agents                                     │ │
│  │  ├── Task decomposition                                                │ │
│  │  └── workstack_id → agents collaborate → aggregated result            │ │
│  │                                                                         │ │
│  │  SKILL (Enhanced Execution)                                             │ │
│  │  ├── Apply skill to tool execution                                     │ │
│  │  └── skill_name + tool → enhanced result                               │ │
│  │                                                                         │ │
│  │  MULTI_AGENT (Explicit Dispatch)                                        │ │
│  │  ├── Explicitly dispatch to multiple agents                            │ │
│  │  └── [agent1, agent2, ...] → parallel execution → results             │ │
│  │                                                                         │ │
│  │  WORKFLOW (Node-Based Pipeline)                                         │ │
│  │  ├── Tools/services as nodes in a graph                                │ │
│  │  ├── Outputs flow to inputs                                            │ │
│  │  └── workflow_id → node1 → node2 → ... → final result                 │ │
│  │                                                                         │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘

Workstack Example (Multi-Agent Collaboration):
┌─────────────────────────────────────────────────────────────────────────────┐
│  Task: "Build new API endpoint for user authentication"                     │
│                                                                              │
│  ┌───────────────────┐     ┌───────────────────┐     ┌───────────────────┐ │
│  │   Python Agent    │     │   Database Agent  │     │   Security Agent  │ │
│  │   ─────────────   │     │   ──────────────  │     │   ──────────────  │ │
│  │   Writes endpoint │────►│   Designs schema  │────►│   Reviews for     │ │
│  │   handler code    │     │   and migrations  │     │   vulnerabilities │ │
│  └───────────────────┘     └───────────────────┘     └───────────────────┘ │
│           │                         │                         │             │
│           └─────────────────────────┴─────────────────────────┘             │
│                                     │                                        │
│                                     ▼                                        │
│                         ┌───────────────────────┐                           │
│                         │   Aggregated Result   │                           │
│                         │   - Code files        │                           │
│                         │   - Migration SQL     │                           │
│                         │   - Security report   │                           │
│                         └───────────────────────┘                           │
└─────────────────────────────────────────────────────────────────────────────┘

Workflow Example (Node-Based Pipeline):
┌─────────────────────────────────────────────────────────────────────────────┐
│  Workflow: "data_processing_pipeline"                                        │
│                                                                              │
│  ┌──────────┐    ┌───────────┐    ┌──────────┐    ┌─────────┐              │
│  │ fetch_   │    │ transform │    │ validate │    │  store  │              │
│  │ data     │───►│ _data     │───►│ _schema  │───►│ _result │              │
│  │          │    │           │    │          │    │         │              │
│  │ INPUT:   │    │ INPUT:    │    │ INPUT:   │    │ INPUT:  │              │
│  │  url     │    │  raw_data │    │  data    │    │  valid  │              │
│  │          │    │           │    │          │    │  _data  │              │
│  │ OUTPUT:  │    │ OUTPUT:   │    │ OUTPUT:  │    │ OUTPUT: │              │
│  │  raw_data│    │  data     │    │  valid_  │    │  id     │              │
│  │          │    │           │    │  data    │    │         │              │
│  └──────────┘    └───────────┘    └──────────┘    └─────────┘              │
│                                                                              │
│  Each node is a tool. Outputs connect to next node's inputs.                │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 8.2 Intended Functionality

### Primary Purpose
Enable advanced execution patterns beyond single tool calls.

### Capabilities
| Mode | Description |
|------|-------------|
| **Workstacks** | Multiple agents collaborate on one task with shared context |
| **Workflows** | Node-based pipelines connecting tools/services |
| **Skills** | Enhancement layers applied to tool execution |
| **Multi-Agent** | Explicit parallel dispatch to multiple agents |

## 8.3 Missing Dependencies

```rust
use op_chat::{
    ExecutionMode,           // Enum: Direct, Workstack, Skill, MultiAgent, Workflow
    OrchestratedExecutor,    // Main executor struct
    OrchestratedResult,      // Result with mode, trace, metrics
    Workflow,                // Workflow definition
    WorkflowStep,            // Step in workflow
};
use op_core::ExecutionTracker;
```

## 8.4 Ratings

| Criteria | Rating | Justification |
|----------|--------|---------------|
| **Usefulness** | ★★★★★ (5/5) | Advanced orchestration capabilities |
| **Code Quality** | ★★★★★ (5/5) | Clean, well-designed |
| **Implementation** | ★★★★★ (5/5) | Sophisticated execution model |
| **Integration Effort** | ★★☆☆☆ (2/5) | Requires significant op_chat additions |

**Overall: ★★★★★ (5/5) - High value when dependencies added**

---

# 9-15. Introspection Modules

**Status:** INTEGRATED (All Compile Successfully)
**Location:** `crates/op-mcp-old/src/`

## Summary Table

| Module | Lines | Purpose | Rating |
|--------|-------|---------|--------|
| **hybrid_scanner.rs** | 450 | Unified system scan (D-Bus + FS + processes + hardware) | ★★★★★ |
| **consolidated_introspection.rs** | 800 | Unified API with SQLite caching | ★★★★☆ |
| **comprehensive_introspection.rs** | 176 | D-Bus system/session bus introspection | ★★★☆☆ |
| **native_introspection.rs** | 2,700 | Low-level D-Bus XML parsing | ★★★★☆ |
| **system_introspection.rs** | 550 | System service discovery | ★★★☆☆ |
| **json_introspection.rs** | 160 | JSON conversion utilities | ★★★☆☆ |
| **workflow_plugin_introspection.rs** | 425 | Workflow/plugin discovery | ★★★☆☆ |

## 9.1 hybrid_scanner.rs - Recommended for Use

### Data Structures
```rust
pub struct HybridSystemScan {
    pub dbus_services: Vec<DiscoveredService>,
    pub filesystem_resources: Vec<FilesystemResource>,
    pub processes: Vec<ProcessInfo>,
    pub hardware: Vec<HardwareDevice>,
    pub network_interfaces: Vec<NetworkInterface>,
    pub system_config: Vec<ConfigFile>,
    pub timestamp: i64,
}
```

### Capabilities
- Scans D-Bus services
- Discovers filesystem resources (configs, sockets, devices)
- Lists processes with memory/CPU stats
- Enumerates hardware (via /sys)
- Lists network interfaces
- Finds configuration files

---

# Summary: Integration Priorities

## Priority 1: Immediate Value (Already Working)
- `external_client.rs` - Use now
- `http_server.rs` - Use now
- `hybrid_scanner.rs` - Use now

## Priority 2: High Value, Medium Effort
- `lazy_tools.rs` - Add op-tools discovery/builtin APIs
- `server.rs` - Automatically enabled when lazy_tools works

## Priority 3: High Value, High Effort
- `tool_adapter_orchestrated.rs` - Add op_chat orchestration types
- `tool_adapter.rs` - Clean file + add missing deps

## Priority 4: Low Priority
- `router.rs` - Duplicates http_server.rs
- `config.rs` - Trivial, not needed

---

# Appendix: Dependency Graph

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            DEPENDENCY GRAPH                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  WORKING NOW:                                                                │
│  ┌──────────────────┐     ┌──────────────────┐                             │
│  │ external_client  │     │   http_server    │                             │
│  │                  │     │                  │                             │
│  │  Deps: tokio,    │     │  Deps: axum,     │                             │
│  │  serde, anyhow   │     │  op_agents       │                             │
│  └──────────────────┘     └──────────────────┘                             │
│                                                                              │
│  BLOCKED - Need op_tools APIs:                                              │
│  ┌──────────────────┐                                                       │
│  │   lazy_tools     │◄───── Needs: op_tools::{builtin, discovery}          │
│  └────────┬─────────┘                                                       │
│           │                                                                  │
│           ▼                                                                  │
│  ┌──────────────────┐                                                       │
│  │     server       │◄───── Blocked by lazy_tools                          │
│  └──────────────────┘                                                       │
│                                                                              │
│  BLOCKED - Need op_chat APIs:                                               │
│  ┌──────────────────┐                                                       │
│  │ tool_adapter_    │◄───── Needs: op_chat::{ExecutionMode,                │
│  │ orchestrated     │       OrchestratedExecutor, Workflow, ...}           │
│  └──────────────────┘                                                       │
│                                                                              │
│  BLOCKED - Multiple Issues:                                                  │
│  ┌──────────────────┐                                                       │
│  │  tool_adapter    │◄───── 1. File format corrupted                       │
│  │                  │       2. Needs: op_execution_tracker                  │
│  │                  │       3. Needs: op_dynamic_loader                     │
│  └──────────────────┘                                                       │
│                                                                              │
│  LOW PRIORITY:                                                               │
│  ┌──────────────────┐     ┌──────────────────┐                             │
│  │     router       │     │     config       │                             │
│  │ (duplicates      │     │ (trivial, needs  │                             │
│  │  http_server)    │     │  config crate)   │                             │
│  └──────────────────┘     └──────────────────┘                             │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```
