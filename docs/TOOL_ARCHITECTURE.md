# Tool Architecture - op-dbus

## Overview

op-dbus uses **per-request tool loading** with **compact mode** to efficiently serve 100+ tools.

## Key Concepts

### Per-Request Loading (NOT Session-Based)

```
REQUEST START
    │
    ├── Create RequestContext
    ├── Load ALL tools (54+)
    ├── Initialize turn counter (0/75)
    │
    ▼
PROCESSING
    │
    ├── Execute tool 1 (turn 1/75)
    ├── Execute tool 2 (turn 2/75)
    ├── ... up to 75 tool calls
    │
    ▼
REQUEST END
    │
    ├── DROP RequestContext
    ├── Unload ALL tools
    └── Free memory
```

**This is NOT session-based:**
- Each HTTP request gets fresh tools
- Tools are NOT shared between requests
- Memory is freed after every request
- max_turns is per REQUEST, not per session

### Why Per-Request?

| Approach | Pros | Cons |
|----------|------|------|
| ❌ Session-based | Faster subsequent calls | Memory bloat, eviction bugs |
| ❌ Lazy loading | Lower initial memory | Eviction during request = bugs |
| ✅ Per-request | Clean, no eviction | Small startup cost per request |

The per-request approach:
1. **No eviction bugs** - Tools can't disappear mid-request
2. **Clean memory** - Freed after every request
3. **Isolation** - Each request is independent
4. **Predictable** - Same behavior every time

## Compact Mode

### What LLM Sees (5 meta-tools)

```json
[
  {"name": "execute_tool", "description": "Execute any tool by name"},
  {"name": "list_tools", "description": "List available tools"},
  {"name": "search_tools", "description": "Search tools by keyword"},
  {"name": "get_tool_schema", "description": "Get tool parameters"},
  {"name": "respond", "description": "Send response to user"}
]
```

### What's Actually Loaded (54+ tools)

| Category | Count | Examples |
|----------|-------|----------|
| Response | 3 | respond_to_user, cannot_perform, request_clarification |
| Filesystem | 3 | read_file, write_file, list_directory |
| Shell | 1 | shell_execute |
| System | 2 | procfs_read, list_network_interfaces |
| Systemd | 8 | unit_status, list_units, start/stop/restart/enable/disable, reload |
| OVS | 10 | list_bridges, show_bridge, list_ports, etc. |
| Plugin | 27 | 9 plugins × 3 ops (query/diff/apply) |
| **Total** | **54** | All loaded per request |

## Turn Limit

```
max_turns = 75 per REQUEST
```

- After 75 tool calls, request fails
- Prevents runaway loops
- Counter resets on next request

## Request Lifecycle

```rust
// 1. Request comes in
let ctx = RequestContext::new(request_id, config);

// 2. Load ALL tools
ctx.load_tool(Arc::new(RespondToUserTool));
ctx.load_tool(Arc::new(ReadFileTool));
// ... all 54+ tools

// 3. Execute tools (turns 1..75)
for tool_call in request.tool_calls {
    ctx.increment_turn()?;  // Fails at 76
    ctx.execute_tool(name, args).await?;
}

// 4. Request completes, context dropped
drop(ctx);  // All tools freed, memory released
```

## Configuration

```rust
CompactServerConfig {
    max_turns: 75,  // Per REQUEST
    categories: vec!["systemd", "ovs", ...],
    page_size: 50,
}
```

## Comparison with Old (Broken) Approach

### Old: LRU Cache with Eviction (BROKEN)

```
max_loaded_tools: 100
min_idle_secs: 300
eviction_check_interval: 10

PROBLEM: Tools evicted during request!
  - Request starts, needs 145 tools
  - Only 100 can be loaded
  - LRU evicts older tools
  - Tool call fails: "Tool not found"
```

### New: Per-Request Loading (CORRECT)

```
max_turns: 75  // Per request limit

Request 1: Load all → Execute → Unload
Request 2: Load all → Execute → Unload
...

NO EVICTION, NO "Tool not found"
```

## Memory Profile

- **Per request**: ~5-10 MB for all tools
- **Between requests**: 0 MB (all freed)
- **No memory leaks**: Rust ownership ensures cleanup

## Files

| File | Purpose |
|------|--------|
| `request_context.rs` | Per-request tool holder |
| `request_handler.rs` | Loads tools, processes request |
| `compact.rs` | Compact mode configuration |
| `tools/*.rs` | Tool implementations |
