# Orchestration Architecture

## Overview

op-dbus uses a three-tier orchestration system:

| Tier | Component | Contains | Use Case |
|------|-----------|----------|----------|
| **Tools** | ToolRegistry | Atomic operations | "Read file", "Call D-Bus" |
| **Agents** | AgentCatalog | Operations on tools | "Run cargo test", "Analyze code" |
| **Orchestration** | Workstacks/Workflows | Multi-step plans | "Build feature X" |

## Key Concepts

### Tools
- Atomic operations (file_read, ovs_list_bridges, dbus_call)
- No state, no memory
- Fast, single-purpose

### Agents
- Grouped operations with expertise
- May have memory (via gRPC connection)
- Examples: rust_pro, python_pro, backend_architect

### Skills
- Knowledge injection (markdown content)
- Activated per-tool-call
- Examples: architecture_patterns, microservices_patterns

### Workstacks
- Multi-phase execution plans
- Combine agents + tools
- Phase dependencies and rollback

### Workflows
- Data-flow graphs (nodes + edges)
- Plugins/services as nodes
- Used for infrastructure automation

## Execution Flow

```
User Request: "Build a new REST endpoint for users"
        │
        ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      OrchestratedExecutor                                │
│                                                                          │
│  1. Parse request                                                        │
│  2. Determine execution mode:                                           │
│     - workstack_full_stack_feature → WorkstackExecutor                  │
│     - workflow_deploy → WorkflowEngine                                  │
│     - skill_architecture → SkillRegistry + tool                         │
│     - agent_rust_pro → GrpcAgentPool                                    │
│     - direct tool → TrackedToolExecutor                                 │
└─────────────────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      WorkstackExecutor                                   │
│                                                                          │
│  Phase: analyze                                                          │
│    ├─ Tool: file_read (requirements.md)                                 │
│    └─ Agent: backend_architect (via gRPC)                               │
│                                                                          │
│  Phase: design                                                           │
│    └─ Agent: sequential_thinking (via gRPC streaming)                   │
│                                                                          │
│  Phase: implement                                                        │
│    └─ Agent: rust_pro (via gRPC streaming)                              │
│                                                                          │
│  Phase: test                                                             │
│    └─ Agent: rust_pro.test (via gRPC streaming)                         │
│                                                                          │
│  Phase: save_context                                                     │
│    └─ Agent: context_manager (via gRPC)                                 │
└─────────────────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      GrpcAgentPool                                       │
│                                                                          │
│  Run-on-connection (persistent):                                        │
│  ┌──────────────┐ ┌────────────────────┐ ┌─────────────────────────┐    │
│  │  rust_pro    │ │ backend_architect  │ │ sequential_thinking     │    │
│  │  :50051      │ │ :50052             │ │ :50053                  │    │
│  │              │ │                    │ │                         │    │
│  │ check        │ │ analyze            │ │ think (streaming)       │    │
│  │ build        │ │ design             │ │ plan                    │    │
│  │ test         │ │ review             │ │ analyze                 │    │
│  │ clippy       │ │ suggest            │ │ conclude                │    │
│  └──────────────┘ └────────────────────┘ └─────────────────────────┘    │
│                                                                          │
│  ┌──────────────┐ ┌────────────────────┐                                │
│  │   memory     │ │  context_manager   │                                │
│  │   :50054     │ │  :50055            │                                │
│  │              │ │                    │                                │
│  │ remember     │ │ save               │                                │
│  │ recall       │ │ load               │                                │
│  │ forget       │ │ list               │                                │
│  │ search       │ │ export/import      │                                │
│  └──────────────┘ └────────────────────┘                                │
│                                                                          │
│  On-demand (lazy-connect):                                              │
│  python_pro | debugger | mem0 | search_specialist | deployment          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Workstack vs Workflow

| Aspect | Workstack | Workflow |
|--------|-----------|----------|
| **Purpose** | LLM-orchestrated tasks | Infrastructure automation |
| **Contains** | Agents + Tools | Nodes (plugins/services) |
| **Structure** | Phases with dependencies | Graph with edges |
| **Execution** | Phase-by-phase | Data-flow |
| **Rollback** | Per-phase rollback | Transaction-based |
| **Example** | "Build feature X" | "Deploy to production" |

### Workstack Example

```yaml
workstack: full_stack_feature
description: Develop a full-stack feature

phases:
  - id: analyze
    name: Analyze Requirements
    agents: [backend_architect]
    tools:
      - tool: file_read
        args: { path: "${requirements_file}" }
        store_as: requirements
    depends_on: []

  - id: design 
    name: Design Solution
    agents: [sequential_thinking]
    depends_on: [analyze]

  - id: implement
    name: Implement Feature
    agents: [rust_pro]
    depends_on: [design]

  - id: test
    name: Test Implementation
    agents: [rust_pro]  # rust_pro_test
    depends_on: [implement]
    continue_on_failure: true

  - id: review
    name: Code Review
    agents: [backend_architect, security_auditor]
    depends_on: [test]
```

### Workflow Example

```yaml
workflow: deploy_production
description: Deploy to production with CI/CD

nodes:
  - id: build
    type: cargo_build
    inputs: { path: ".", release: true }
    
  - id: test
    type: cargo_test
    inputs: { path: "." }
    depends_on: [build]
    
  - id: docker_build
    type: docker_build
    inputs: { tag: "${version}" }
    depends_on: [test]
    
  - id: push
    type: docker_push
    inputs: { registry: "ghcr.io" }
    depends_on: [docker_build]
    
  - id: deploy
    type: kubectl_apply
    inputs: { manifest: "k8s/deployment.yaml" }
    depends_on: [push]
```

## gRPC Integration

### Benefits

| Feature | D-Bus (current) | gRPC (new) |
|---------|-----------------|------------|
| Connection | Per-call | Persistent |
| Streaming | ❌ | ✅ |
| Batching | ❌ | ✅ |
| Latency | ~5-10ms | ~0.5-2ms |
| Session state | None | Built-in |

### Run-on-Connection Agents

These agents start when the user connects and stay running:

1. **rust_pro** - Primary development agent (cargo operations)
2. **backend_architect** - Architecture guidance
3. **sequential_thinking** - Reasoning chains
4. **memory** - Session key-value store
5. **context_manager** - Persistent context

### Usage in WorkstackExecutor

```rust
async fn execute_phase(&self, phase: &WorkstackPhase) -> Result<PhaseResult> {
    // Execute tools first
    for tool_call in &phase.tools {
        let result = self.tool_executor.execute(&tool_call.tool, &tool_call.arguments).await?;
        // Store result...
    }
    
    // Execute agents via gRPC pool
    for agent_id in &phase.agents {
        if is_streaming_agent(agent_id) {
            // Use streaming for rust_pro, sequential_thinking
            let result = self.grpc_pool.execute_streaming(
                agent_id,
                &phase.operation,
                arguments,
                |chunk| self.emit_progress(chunk),
            ).await?;
        } else {
            // Non-streaming for others
            let result = self.grpc_pool.execute(agent_id, &phase.operation, arguments).await?;
        }
    }
    
    Ok(PhaseResult { ... })
}
```

## Configuration

### Agent Pool Config

```toml
# /etc/op-dbus/agents.toml

[pool]
base_address = "http://127.0.0.1"
connect_timeout_ms = 5000
request_timeout_ms = 30000

[run_on_connection]
agents = [
    "rust_pro",
    "backend_architect",
    "sequential_thinking",
    "memory",
    "context_manager",
]

[ports]
rust_pro = 50051
backend_architect = 50052
sequential_thinking = 50053
memory = 50054
context_manager = 50055
python_pro = 50056
debugger = 50057
```

### Workstack Registry

```toml
# /etc/op-dbus/workstacks.toml

[workstacks.full_stack_feature]
enabled = true
default_timeout = 600

[workstacks.code_review]
enabled = true
parallel_phases = ["security_review", "arch_review", "performance_review"]
```
