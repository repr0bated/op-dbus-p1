# gRPC Integration for Chatbot

## Overview

This document describes how gRPC is integrated into the chatbot (op-chat) for high-performance internal communication with agents.

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│                         EXTERNAL LAYER                                    │
│                    (unchanged - HTTP/WebSocket)                           │
├──────────────────────────────────────────────────────────────────────────┤
│   Browser ────WebSocket────►  op-web ──HTTP/SSE──► MCP Clients           │
│                                  │                                        │
│                                  │                                        │
│                            ┌─────▼─────┐                                  │
│                            │ Orchestrator│                                │
│                            └─────┬─────┘                                  │
└──────────────────────────────────┼────────────────────────────────────────┘
                                   │
┌──────────────────────────────────┼────────────────────────────────────────┐
│                        INTERNAL LAYER (NEW)                               │
│                            (gRPC)                                         │
├──────────────────────────────────┼────────────────────────────────────────┤
│                                  ▼                                        │
│                     ┌────────────────────────┐                            │
│                     │      ChatActor         │                            │
│                     │  (GrpcAgentClient)     │                            │
│                     └───────────┬────────────┘                            │
│                                 │                                         │
│            ┌────────────────────┼────────────────────┐                    │
│            │    gRPC Streaming  │                    │                    │
│            ▼                    ▼                    ▼                    │
│   ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐          │
│   │    rust_pro     │  │     memory      │  │  seq_thinking   │          │
│   │    (cargo)      │  │   (key-value)   │  │   (reasoning)   │          │
│   │    :50051       │  │     :50052      │  │     :50053      │          │
│   └─────────────────┘  └─────────────────┘  └─────────────────┘          │
│            │                    │                    │                    │
│            ▼                    ▼                    ▼                    │
│   ┌─────────────────┐  ┌─────────────────┐                                │
│   │ backend_architect│ │ context_manager │                                │
│   │     :50054      │  │     :50055      │                                │
│   └─────────────────┘  └─────────────────┘                                │
└───────────────────────────────────────────────────────────────────────────┘
```

## Benefits

### 1. Streaming Support

**Before (current):**
```rust
// In process.rs - heartbeat hack
let heartbeat_handle = tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        tx.send(OrchestratorEvent::Thinking).await;
    }
});
```

**After (with gRPC):**
```rust
// Natural streaming
let mut stream = agent_client.cargo_build(&session_id, ".", true, |chunk| {
    event_tx.send(OrchestratorEvent::ToolOutput {
        content: chunk.content,
        stream: chunk.stream_type,
    }).await;
}).await?;
```

### 2. Run-on-Connection Agents

```rust
// On user connect
let agents = client.start_session(&session_id, "cursor").await?;
// Returns: ["rust_pro", "memory", "sequential_thinking", "context_manager", "backend_architect"]

// All agents are now warm and ready
// No cold start on first tool call
```

### 3. Batch Operations

```rust
// Execute multiple operations in one roundtrip
let results = client.batch_execute(&session_id, vec![
    ("memory", "remember", json!({"key": "project", "value": "op-dbus-v2"})),
    ("memory", "remember", json!({"key": "language", "value": "rust"})),
    ("context_manager", "load", json!({"name": "session-context"})),
], true).await?;
```

### 4. High-Frequency Operations

| Agent | Operation | D-Bus Latency | gRPC Latency | Improvement |
|-------|-----------|---------------|--------------|-------------|
| memory | remember | ~5ms | ~0.5ms | 10x |
| memory | recall | ~5ms | ~0.5ms | 10x |
| sequential_thinking | think | ~8ms | ~1ms | 8x |
| rust_pro | check | ~10ms + wait | streaming | Real-time output |

## Implementation

### Step 1: Proto Compilation

Add to `crates/op-chat/build.rs`:
```rust
fn main() {
    tonic_build::compile_protos("proto/agents.proto")
        .expect("Failed to compile agent protos");
}
```

Add to `crates/op-chat/Cargo.toml`:
```toml
[dependencies]
tonic = "0.11"
prost = "0.12"

[build-dependencies]
tonic-build = "0.11"
```

### Step 2: Replace D-Bus Calls in ChatActor

```rust
// Before
let result = dbus_agent_executor.execute(&agent_id, &op, args).await?;

// After
let result = grpc_client.execute(&session_id, &agent_id, &op, args).await?;
```

### Step 3: Add Session Lifecycle

```rust
impl ChatActor {
    pub async fn on_connect(&self, session_id: &str, client_name: &str) {
        // Start run-on-connection agents
        let agents = self.grpc_client.start_session(session_id, client_name).await?;
        info!("Started {} agents for session {}", agents.len(), session_id);
    }
    
    pub async fn on_disconnect(&self, session_id: &str) {
        self.grpc_client.end_session(session_id).await?;
    }
}
```

### Step 4: Streaming Tool Output

```rust
impl UnifiedOrchestrator {
    async fn execute_streaming_tool(
        &self,
        name: &str,
        args: Value,
        event_tx: &mpsc::Sender<OrchestratorEvent>,
    ) -> Result<ToolResult> {
        // Use streaming for rust_pro operations
        if name.starts_with("rust_pro_") {
            return self.grpc_client.execute_stream(
                &self.session_id,
                "rust_pro",
                &name[9..], // Strip prefix
                args,
                |chunk| {
                    let _ = event_tx.blocking_send(OrchestratorEvent::ToolOutput {
                        name: name.to_string(),
                        content: chunk.content,
                        is_stderr: chunk.stream_type == StreamType::Stderr,
                        is_final: chunk.is_final,
                    });
                },
            ).await;
        }
        
        // Non-streaming tools use regular execute
        self.grpc_client.execute(&self.session_id, agent_id, op, args).await
    }
}
```

## Configuration

```toml
# /etc/op-dbus/grpc.toml

[client]
address = "http://127.0.0.1:50051"
connect_timeout_ms = 5000
request_timeout_ms = 30000
max_retries = 3
pool_connections = true

[agents]
# Run-on-connection agents (started immediately on user connect)
run_on_connection = [
    "rust_pro",
    "backend_architect",
    "sequential_thinking",
    "memory",
    "context_manager",
]

# On-demand agents (started on first call)
on_demand = [
    "mem0",
    "search_specialist",
    "python_pro",
    "debugger",
    "deployment",
    "prompt_engineer",
]
```

## Migration Path

### Phase 1: Add gRPC Client (Non-Breaking)
1. Add `GrpcAgentClient` to `op-chat`
2. Keep D-Bus executor as fallback
3. Route specific agents through gRPC

### Phase 2: Session Lifecycle
1. Add session start/end hooks
2. Start run-on-connection agents on connect
3. Track session state

### Phase 3: Streaming
1. Convert long-running tools to streaming
2. Update WebSocket events for streaming
3. Remove heartbeat hack

### Phase 4: Full Migration
1. Route all agent calls through gRPC
2. Deprecate D-Bus agent executor
3. Measure performance improvements

## Testing

```bash
# Unit tests
cargo test -p op-chat --features grpc

# Integration test (requires agent service running)
cargo test -p op-chat --features grpc -- --ignored grpc_integration

# Performance comparison
cargo bench -p op-chat --features grpc
```

## Monitoring

```rust
// Metrics exposed by GrpcAgentClient
struct AgentMetrics {
    requests_total: Counter,
    request_duration_seconds: Histogram,
    active_sessions: Gauge,
    streaming_chunks_total: Counter,
    errors_total: Counter,
}
```
