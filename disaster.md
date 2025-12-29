# op-dbus topography (canonical, end-to-end)

This is a system topology spec grounded in the current repo layout and wiring.
All component names and flows below are backed by concrete code paths.

---

## 1) Boundary diagram

### Control plane (decision + orchestration)

* **Entry points**
  - **D-Bus service**: `org.op_dbus.Service` exports Chat + State interfaces (`op-dbus-service/src/main.rs`).
  - **HTTP API**: unified Axum server mounts `/api/tools`, `/api/agents`, and `/health` (`op-dbus-service/src/main.rs`, `crates/op-http/src/lib.rs`, `crates/op-tools/src/router.rs`, `crates/op-agents/src/router.rs`).
  - **MCP adapter** (optional): `op-mcp-server` bridges MCP JSON-RPC over stdio to `op-chat` (`crates/op-mcp/README.md`, `crates/op-mcp/src/main.rs`).

* **Chat orchestration**
  - **ChatActor** handles requests, tool listing, tool execution, and routing (`crates/op-chat/src/actor.rs`).
  - **TrackedToolExecutor** executes tools with per-call tracking (`crates/op-chat/src/tool_executor.rs`).

* **State orchestration**
  - **StateManager** coordinates plugin state queries, diffs, checkpoints, and apply (`crates/op-state/src/manager.rs`).
  - **StatePlugin** trait defines the contract for state domains (`crates/op-state/src/plugin.rs`).

### Data plane (side effects + observations)

* **D-Bus**: system services invoked by tools (systemd operations via zbus) (`crates/op-tools/src/builtin/dbus.rs`).
* **Filesystem + procfs/sysfs**: read/write tools (`crates/op-tools/src/builtin/file.rs`, `crates/op-tools/src/builtin/procfs.rs`).
* **External MCP tools**: optional tool calls via `mcptools` CLI (`crates/op-tools/src/mcptools.rs`).

---

## 2) Runtime topology (who talks to whom)

```
Clients
  |                  (D-Bus: org.op_dbus.Service)
  |                  /org/op_dbus/Chat  -> org.op_dbus.Chat
  |                  /org/op_dbus/State -> org.op_dbus.State
  |                         |
  |                         v
  |                     ChatActor
  |                         |
HTTP / MCP  ----------------|----------------------------------------
  |                         v
  |                    TrackedToolExecutor
  |                         |
  |                         v
  |                    ToolRegistry (LRU + lazy)
  |                         |
  |                         v
  |          Tool implementations (D-Bus, file, procfs, MCP)
  |                         |
  |                         v
  |                     External systems
  |
  |                         (State flow)
  |                         v
  |                    StateManager (op-state)
  |                         |
  |                         v
  |                   StatePlugin implementations
```

**Concrete wiring in the binary** (`op-dbus-service/src/main.rs`):

* Initializes a shared `ToolRegistry` and registers built-ins (`crates/op-tools/src/lib.rs`).
* Starts `ChatActor` with the shared registry (`crates/op-chat/src/actor.rs`).
* Exports D-Bus interfaces:
  - `org.op_dbus.Chat` at `/org/op_dbus/Chat` (`op-dbus-service/src/chat.rs`).
  - `org.op_dbus.State` at `/org/op_dbus/State` (`op-dbus-service/src/state.rs`).
* Starts an HTTP server via `op-http`, mounting `/api/tools` and `/api/agents` (`op-dbus-service/src/main.rs`).

---

## 3) Execution flow (tool calls)

### 3.1 D-Bus Chat interface

* Method: `chat(message, session_id)` → returns string or JSON (`op-dbus-service/src/chat.rs`).
* Method: `list_tools()` → returns tool definitions (`op-dbus-service/src/chat.rs`).

Flow:
1. D-Bus client calls `org.op_dbus.Chat.chat`.
2. `ChatActor` receives an RPC request and executes a tool or returns an error (`crates/op-chat/src/actor.rs`).
3. `TrackedToolExecutor` resolves the tool in `ToolRegistry` and executes it (`crates/op-chat/src/tool_executor.rs`).
4. `ExecutionTracker` records the execution context/result (`crates/op-core/src/lib.rs`, `crates/op-execution-tracker/src/execution_tracker.rs`).

### 3.2 HTTP tools API

* `GET /api/tools` → list tools (`crates/op-tools/src/router.rs`).
* `GET /api/tools/:name` → tool definition (`crates/op-tools/src/router.rs`).
* `POST /api/tools/:name/execute` → direct tool execution (`crates/op-tools/src/router.rs`).

Flow:
1. HTTP client calls `/api/tools/:name/execute`.
2. Handler pulls tool from `ToolRegistry` and executes it directly.
3. Result is returned as JSON (no ChatActor involved in this path).

### 3.3 MCP adapter (stdio)

* MCP methods map to `op-chat` calls (`crates/op-mcp/README.md`, `crates/op-mcp/src/main.rs`).
* Flow: stdin JSON-RPC → ChatActorHandle → stdout JSON-RPC.

---

## 4) State model (desired vs observed)

### 4.1 Desired and current state structures

* **DesiredState**: `{ version, plugins: { name: <json> } }` (`crates/op-state/src/manager.rs`).
* **CurrentState**: `{ plugins: { name: <json> } }` (`crates/op-state/src/manager.rs`).

### 4.2 StatePlugin contract

Each plugin must implement:

* `query_current_state()`
* `calculate_diff(current, desired)` → `StateDiff` (with `StateAction`s)
* `apply_state(diff)` → `ApplyResult`
* `create_checkpoint()` / `rollback()`
* `verify_state()` (optional)

Source: `crates/op-state/src/plugin.rs`.

### 4.3 Apply pipeline (current behavior)

`StateManager::apply_state` executes four phases (`crates/op-state/src/manager.rs`):

1. **Checkpoints**: call `create_checkpoint()` for each plugin in desired state.
2. **Diff**: compute `StateDiff` for each plugin.
3. **Apply**: call `apply_state()` for each diff.
4. **Verify**: currently **disabled** (explicitly skipped in code).

Important details:

* Rollback is **disabled** for failures in the bulk `apply_state` path.
* Verification is **disabled** (commented out, with a warning).

These are real runtime semantics today, not aspirational.

---

## 5) Built-in tool surface (what actually executes)

### 5.1 Registered by default

`op-tools` registers the following at startup (`crates/op-tools/src/lib.rs`, `crates/op-tools/src/builtin/mod.rs`):

* **Filesystem tools**: `file_read`, `file_write`, `file_list`, `file_exists`, `file_stat`.
* **procfs/sysfs tools**: `procfs_read`, `procfs_write`, `sysfs_read`, `sysfs_write`.
* **D-Bus systemd tools**:
  - `dbus_systemd_start_unit`
  - `dbus_systemd_stop_unit`
  - `dbus_systemd_restart_unit`
  - `dbus_systemd_get_unit_status`
  - `dbus_systemd_list_units`
  (all in `crates/op-tools/src/builtin/dbus.rs`)
* **D-Bus introspection tools**: registered in `crates/op-tools/src/builtin/dbus_introspection.rs`.

### 5.2 MCP tool injection (optional)

If MCP tool servers are configured, the registry lazily creates tools that proxy through `mcptools` (`crates/op-tools/src/mcptools.rs`).

Configuration inputs:

* `OP_MCPTOOLS_CONFIG` (default `mcptools.json`)
* `OP_MCPTOOLS_SERVERS`
* `OP_MCPTOOLS_SERVER` / `OP_MCPTOOLS_SERVER_NAME`

---

## 6) D-Bus service topology

### 6.1 Exported name + objects

* **Bus name**: `org.op_dbus.Service` (`op-dbus-service/src/main.rs`).
* **Objects**:
  - `/org/op_dbus/Chat` implements `org.op_dbus.Chat` (`op-dbus-service/src/chat.rs`).
  - `/org/op_dbus/State` implements `org.op_dbus.State` (`op-dbus-service/src/state.rs`).

### 6.2 State interface methods

* `get_state(plugin_name)`
* `get_all_state()`
* `set_state(plugin_name, state_json)`
* `set_all_state(state_json)`
* `apply_from_file(path)`
* `apply_plugin_from_file(plugin_name, path)`

Source: `op-dbus-service/src/state.rs`.

---

## 7) Execution tracking and telemetry

* `TrackedToolExecutor` uses `ExecutionTracker` from `op-core`, which re-exports `op-execution-tracker` (`crates/op-chat/src/tool_executor.rs`, `crates/op-core/src/lib.rs`).
* Metrics + telemetry objects are wired in `ChatActor::with_registry` (`crates/op-chat/src/actor.rs`).
* A D-Bus execution tracker interface exists but is **commented out** in `op-dbus-service/src/main.rs`.

---

## 8) Optional / not-wired-yet components

These exist in the repo but are not currently wired into the `op-dbus-service` runtime:

* **D-Bus introspection service** in `op-chat` (currently returns “disabled”).
* **Execution tracker D-Bus interface** (commented in `op-dbus-service/src/main.rs`).
* **op-state-store** (SQLite/Redis job ledger) is present but not connected to the service.
* **op-plugins** provides plugin registry/dynamic loading, but `op-dbus-service` uses `op-state` directly.

---

## 9) End-to-end flow examples

### 9.1 Systemd restart via D-Bus tool

```
Client -> D-Bus org.op_dbus.Chat.chat
  -> ChatActor.execute_tool
    -> ToolRegistry.get("dbus_systemd_restart_unit")
      -> D-Bus call to systemd Manager.RestartUnit
```

Relevant files:

* `op-dbus-service/src/chat.rs`
* `crates/op-chat/src/actor.rs`
* `crates/op-chat/src/tool_executor.rs`
* `crates/op-tools/src/builtin/dbus.rs`

### 9.2 Desired state apply

```
Client -> D-Bus org.op_dbus.State.set_all_state
  -> StateManager.apply_state
    -> create_checkpoint per plugin
    -> calculate_diff per plugin
    -> apply_state per plugin
    -> verification skipped
```

Relevant files:

* `op-dbus-service/src/state.rs`
* `crates/op-state/src/manager.rs`
* `crates/op-state/src/plugin.rs`

---

## 10) Canonical truth summary

* **Single D-Bus service**: `org.op_dbus.Service` exposes Chat + State.
* **Single HTTP server**: `op-http` composes routers; only `/api/tools` and `/api/agents` are mounted by default.
* **Tool execution** is centralized via `ToolRegistry` and `TrackedToolExecutor` with execution tracking.
* **State management** is plugin-based via `op-state` with diff/apply/checkpoint flow.
* **Verification and rollback are currently disabled** in bulk apply; this is an explicit runtime behavior.
