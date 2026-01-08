# Agents Reference

## Run-on-Connection Agents

These agents start automatically when a client connects to the Agents MCP server.

---

### rust_pro (Priority: 100)

**Description:** Rust development environment with cargo, clippy, and rustfmt.

**Operations:**

| Operation | Description | Required Args |
|-----------|-------------|---------------|
| `check` | Run cargo check | path (optional) |
| `build` | Run cargo build | path, release, features |
| `test` | Run cargo test | path, filter, features |
| `clippy` | Run clippy lints | path, fix |
| `format` | Check/apply rustfmt | path, fix |
| `run` | Run cargo run | path, release, features |
| `doc` | Generate docs | path |
| `bench` | Run benchmarks | path |

**Example:**
```json
{
  "name": "rust_pro_check",
  "arguments": {
    "path": ".",
    "features": "full"
  }
}
```

---

### backend_architect (Priority: 99)

**Description:** System design, architecture review, and pattern suggestions.

**Operations:**

| Operation | Description | Required Args |
|-----------|-------------|---------------|
| `analyze` | Analyze codebase structure | path, scope |
| `design` | Create design documents | context, constraints |
| `review` | Review design decisions | design, criteria |
| `suggest` | Suggest improvements | context, constraints |
| `document` | Generate documentation | path, format |

**Example:**
```json
{
  "name": "backend_architect_analyze",
  "arguments": {
    "path": "crates/op-mcp/src",
    "scope": "crate"
  }
}
```

---

### sequential_thinking (Priority: 98)

**Description:** Step-by-step reasoning and problem decomposition.

**Operations:**

| Operation | Description | Required Args |
|-----------|-------------|---------------|
| `think` | Record a thought step | thought, step, total_steps |
| `plan` | Create execution plan | thought, step, total_steps |
| `analyze` | Analyze a problem | thought, step, total_steps |
| `conclude` | Draw conclusions | thought, step, total_steps |
| `reflect` | Reflect on progress | thought, step, total_steps |

**Example:**
```json
{
  "name": "sequential_thinking_think",
  "arguments": {
    "thought": "First, I need to understand the MCP protocol structure",
    "step": 1,
    "total_steps": 5
  }
}
```

---

### memory (Priority: 97)

**Description:** Key-value memory for session state.

**Operations:**

| Operation | Description | Required Args |
|-----------|-------------|---------------|
| `remember` | Store a value | key, value |
| `recall` | Retrieve a value | key |
| `forget` | Delete a value | key |
| `list` | List all keys | pattern (optional) |
| `search` | Search values | query |

**Example:**
```json
{
  "name": "memory_remember",
  "arguments": {
    "key": "current_task",
    "value": "Implementing MCP server with run-on-connection agents"
  }
}
```

---

### context_manager (Priority: 96)

**Description:** Persist context across sessions.

**Operations:**

| Operation | Description | Required Args |
|-----------|-------------|---------------|
| `save` | Save context | name, content, tags |
| `load` | Load context | name |
| `list` | List contexts | tag (optional) |
| `delete` | Delete context | name |
| `export` | Export to file | path, format |
| `import` | Import from file | path, format |
| `clear` | Clear all contexts | - |

**Example:**
```json
{
  "name": "context_manager_save",
  "arguments": {
    "name": "mcp-project-context",
    "content": "Working on op-mcp crate, implementing agents server",
    "tags": ["rust", "mcp", "in-progress"]
  }
}
```

---

## On-Demand Agents

These agents start on first call (lazy loading).

### mem0 (Priority: 80)
Semantic vector memory with similarity search.

### search_specialist (Priority: 75)
Search code, documentation, and web resources.

### python_pro (Priority: 70)
Python code analysis, execution, and formatting.

### debugger (Priority: 70)
Error analysis and debugging assistance.

### deployment (Priority: 60)
Service deployment and management.

### prompt_engineer (Priority: 50)
Generate and optimize prompts.

---

## Tool Naming Convention

Agent tools follow the pattern: `{agent_id}_{operation}`

Examples:
- `rust_pro_check`
- `rust_pro_build`
- `memory_remember`
- `memory_recall`
- `sequential_thinking_think`
- `context_manager_save`

---

## D-Bus Service Names

| Agent | D-Bus Service |
|-------|---------------|
| rust_pro | org.dbusmcp.Agent.RustPro |
| backend_architect | org.dbusmcp.Agent.BackendArchitect |
| sequential_thinking | org.dbusmcp.Agent.SequentialThinking |
| memory | org.dbusmcp.Agent.Memory |
| context_manager | org.dbusmcp.Agent.ContextManager |
| mem0 | org.dbusmcp.Agent.Mem0 |
| python_pro | org.dbusmcp.Agent.PythonPro |
