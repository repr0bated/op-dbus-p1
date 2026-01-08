# MCP Server Implementation Guide

## Overview

This document describes the authoritative MCP server implementation with:
- Multiple transports (stdio, HTTP, SSE, WebSocket, gRPC)
- Three server modes (Compact, Agents, Full)
- Run-on-connection agents

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     MCP Server Factory                          │
│  Creates servers based on mode: Compact | Agents | Full        │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│ CompactServer │     │ AgentsServer  │     │  FullServer   │
│ 4 meta-tools  │     │ Always-on 5+  │     │ All 148+      │
│ /mcp/compact  │     │ /mcp/agents   │     │ /mcp/sse      │
└───────────────┘     └───────────────┘     └───────────────┘
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Transport Layer                              │
│  Stdio | HTTP | SSE | HTTP+SSE | WebSocket | gRPC              │
└─────────────────────────────────────────────────────────────────┘
```

## Server Modes

### 1. Compact Mode (Default for LLMs)

Exposes only 4 meta-tools to discover 148+ actual tools:

| Tool | Purpose |
|------|--------|
| `list_tools` | Browse tools with category filter |
| `search_tools` | Search by keyword |
| `get_tool_schema` | Get input schema before execution |
| `execute_tool` | Execute any tool by name |

**Benefits:**
- Saves ~95% context tokens
- Works within Cursor's 40-tool limit
- LLM discovers tools as needed

### 2. Agents Mode (Run-on-Connection)

Cognitive agents that start immediately when client connects:

| Agent | Priority | Operations | Purpose |
|-------|----------|------------|---------|
| `rust_pro` | 100 | check, build, test, clippy, format, run, doc, bench | Rust development |
| `backend_architect` | 99 | analyze, design, review, suggest, document | System design |
| `sequential_thinking` | 98 | think, plan, analyze, conclude, reflect | Step-by-step reasoning |
| `memory` | 97 | remember, recall, forget, list, search | Session state |
| `context_manager` | 96 | save, load, list, delete, export, import, clear | Persistent context |

**On-Demand Agents (lazy-loaded):**
- `mem0` - Semantic vector memory
- `search_specialist` - Code/docs/web search
- `python_pro` - Python development
- `debugger` - Error analysis
- `deployment` - Service deployment
- `prompt_engineer` - Prompt optimization

### 3. Full Mode

All 148+ tools directly exposed. May hit client tool limits.

## Run-on-Connection Behavior

When a client connects to Agents mode:

1. Client sends `initialize` request
2. Server starts all run-on-connection agents via D-Bus
3. Server returns initialize response with `_meta.startedAgents`
4. Agents remain running for session duration
5. On disconnect, agents are stopped

```json
{
  "protocolVersion": "2024-11-05",
  "serverInfo": { "name": "op-mcp-agents" },
  "_meta": {
    "startedAgents": [
      "rust_pro",
      "backend_architect", 
      "sequential_thinking",
      "memory",
      "context_manager"
    ]
  }
}
```

## Transport Options

| Transport | Use Case | Endpoint |
|-----------|----------|----------|
| Stdio | CLI tools, local MCP clients | stdin/stdout |
| HTTP | REST API access | POST /mcp |
| SSE | Server-sent events streaming | GET /sse |
| HTTP+SSE | Bidirectional (recommended) | GET /sse + POST /mcp |
| WebSocket | Full duplex | ws:///ws |
| gRPC | High-performance (optional) | :50051 |

## CLI Usage

```bash
# Compact mode (default), stdio transport
op-mcp-server

# Agents mode with HTTP
op-mcp-server --mode agents --http 0.0.0.0:3002

# Full mode with all transports
op-mcp-server --mode full --all

# Multiple servers on different ports
op-mcp-server --mode compact --http 0.0.0.0:3001 &
op-mcp-server --mode agents --http 0.0.0.0:3002 &
```

## File Structure

```
crates/op-mcp/
├── src/
│   ├── lib.rs              # Public exports, ServerMode enum
│   ├── main.rs             # CLI entry point
│   ├── protocol.rs         # JSON-RPC types
│   ├── server.rs           # Full/Compact McpServer
│   ├── agents_server.rs    # AgentsServer with run-on-connection
│   ├── compact.rs          # CompactServer (4 meta-tools)
│   ├── resources.rs        # MCP resources
│   └── transport/
│       ├── mod.rs          # Transport trait
│       ├── stdio.rs        # Stdio transport
│       ├── http.rs         # HTTP/SSE transports
│       └── websocket.rs    # WebSocket transport
├── proto/
│   └── mcp.proto           # gRPC service definition
└── Cargo.toml
```
