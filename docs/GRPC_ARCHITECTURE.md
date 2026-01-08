# gRPC Architecture for Internal Agent Communication

## Overview

gRPC is recommended for **internal** communication between:
- MCP Gateway ↔ Run-on-connection agents
- Agents ↔ Tool Registry
- Agents ↔ Other agents

**External clients** (Cursor, Claude Desktop, browsers) continue to use HTTP/SSE/stdio.

## Why gRPC Internally?

| Benefit | Impact |
|---------|--------|
| **Binary protocol** | ~10x smaller messages than JSON |
| **HTTP/2 multiplexing** | Multiple concurrent requests on one connection |
| **Bidirectional streaming** | Agents can push status updates |
| **Connection pooling** | Persistent connections for run-on-connect agents |
| **Type safety** | Protobuf catches errors at compile time |
| **Code generation** | Less boilerplate, consistent API |

## Why NOT gRPC for External?

| Challenge | Impact |
|-----------|--------|
| Browser support | Requires grpc-web proxy |
| MCP spec | Expects JSON-RPC |
| Debugging | Binary is harder to inspect |
| Cursor/Claude | Expect stdio or HTTP |

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                     EXTERNAL (JSON/HTTP)                        │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│   Cursor ─────────────┐                                         │
│   Claude Desktop ─────┼──── stdio / SSE ────┐                   │
│   Browser ────────────┤                     │                   │
│   Gemini CLI ─────────┘                     ▼                   │
│                                    ┌────────────────┐           │
│                                    │  MCP Gateway   │           │
│                                    │  (op-mcp)      │           │
│                                    └───────┬────────┘           │
└────────────────────────────────────────────┼────────────────────┘
                                             │
┌────────────────────────────────────────────┼────────────────────┐
│                     INTERNAL (gRPC)        │                    │
├────────────────────────────────────────────┼────────────────────┤
│                                            │                    │
│           ┌────────────────────────────────┼──────────┐         │
│           │                                │          │         │
│           ▼                                ▼          ▼         │
│   ┌───────────────┐              ┌─────────────┐  ┌────────┐   │
│   │  rust_pro     │◄────gRPC────►│   memory    │  │ tools  │   │
│   │  :50051       │              │   :50052    │  │ :50053 │   │
│   └───────────────┘              └─────────────┘  └────────┘   │
│           │                              │                      │
│           │          ┌───────────────────┘                      │
│           │          │                                          │
│           ▼          ▼                                          │
│   ┌───────────────────────┐     ┌──────────────────┐           │
│   │  sequential_thinking  │     │  context_manager │           │
│   │  :50054              │     │  :50055          │           │
│   └───────────────────────┘     └──────────────────┘           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Service Definitions

### AgentLifecycle Service
Manages agent startup/shutdown for run-on-connection:

```protobuf
service AgentLifecycle {
    rpc Start(StartRequest) returns (StartResponse);
    rpc Stop(StopRequest) returns (StopResponse);
    rpc Health(HealthRequest) returns (HealthResponse);
    rpc WatchStatus(WatchRequest) returns (stream AgentStatus);
}
```

### AgentExecution Service
High-performance tool execution:

```protobuf
service AgentExecution {
    rpc Execute(ExecuteRequest) returns (ExecuteResponse);
    rpc BatchExecute(stream ExecuteRequest) returns (stream ExecuteResponse);
    rpc StreamExecute(ExecuteRequest) returns (stream ExecuteChunk);
}
```

### Dedicated Services
Frequently-used agents get dedicated services:

- `MemoryService` - key-value operations
- `SequentialThinkingService` - reasoning chains
- `ContextManagerService` - persistent context
- `RustProService` - cargo operations with streaming output

## Implementation Plan

### Phase 1: Core Infrastructure
1. Add `tonic` to op-mcp Cargo.toml
2. Compile proto files with `tonic-build`
3. Create `GrpcAgentClient` for MCP gateway

### Phase 2: Agent Services
1. Implement `AgentLifecycle` server in each agent
2. Replace D-Bus calls with gRPC in `DbusAgentExecutor`
3. Add connection pooling

### Phase 3: Streaming
1. Add streaming cargo output for `rust_pro`
2. Add streaming thoughts for `sequential_thinking`
3. Add bulk import/export for `context_manager`

## Configuration

```toml
# /etc/op-dbus/grpc.toml

[server]
enable = true
bind = "127.0.0.1"  # Internal only!

[agents.rust_pro]
port = 50051
max_connections = 10

[agents.memory]
port = 50052
max_connections = 50  # High frequency

[agents.sequential_thinking]
port = 50054
max_connections = 10

[agents.context_manager]
port = 50055
max_connections = 10

[tls]
enable = false  # Internal network, use mTLS in production
```

## Benefits for Run-on-Connection

### Current (D-Bus)
```
Client connects → MCP starts agents via D-Bus → Each call is new D-Bus message
```

### With gRPC
```
Client connects → MCP establishes gRPC streams → Persistent bidirectional channels
```

**Advantages:**
1. Agents stay warm (no cold starts)
2. Status updates pushed to MCP
3. Batched operations in one roundtrip
4. Proper backpressure handling

## Benchmarks (Expected)

| Operation | D-Bus | gRPC | Improvement |
|-----------|-------|------|-------------|
| memory_remember | ~5ms | ~0.5ms | 10x |
| tool execute | ~10ms | ~2ms | 5x |
| batch 10 tools | ~100ms | ~10ms | 10x |
| streaming cargo | N/A | ✓ | New capability |
