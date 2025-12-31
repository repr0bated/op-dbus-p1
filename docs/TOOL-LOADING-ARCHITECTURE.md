# Tool Loading Architecture

> **op-dbus-v2** provides three complementary strategies for managing tools under client limits.

## URLs

| Endpoint | Purpose |
|----------|---------|
| `/groups-admin` | **Domain-based group management** (new) |
| `/mcp-picker` | Individual tool picker (legacy) |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Tool Loading Strategies                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐ │
│  │  Groups Admin   │    │  Compact Mode   │    │ Context-Aware   │ │
│  │  (Web UI)       │    │  (4 meta-tools) │    │ (Auto-suggest)  │ │
│  ├─────────────────┤    ├─────────────────┤    ├─────────────────┤ │
│  │ Pre-selects     │    │ Defers tool     │    │ Auto-suggests   │ │
│  │ WHICH tools     │    │ discovery to    │    │ based on        │ │
│  │ are available   │    │ runtime (lazy)  │    │ conversation    │ │
│  └────────┬────────┘    └────────┬────────┘    └────────┬────────┘ │
│           │                      │                      │          │
│           └──────────────────────┴──────────────────────┘          │
│                                  │                                  │
│                           COMBINED USAGE                            │
└─────────────────────────────────────────────────────────────────────┘
```

## Strategy Comparison

| Mode | When Tools Load | Cursor Limit | Best For |
|------|----------------|--------------|----------|
| **Groups Admin** | Upfront (manual) | Must stay ≤40 | Fine control of tool set |
| **Compact Mode** | On-demand (lazy) | Bypasses entirely | Maximum flexibility |
| **Context-Aware** | Dynamic (smart) | Respects limit | Automatic adaptation |

---

## 1. Groups Admin (Web UI)

**URL:** `http://localhost:8080/groups-admin`

Organize tools into domain-based groups (~5 tools each):

### Domains

| Domain | Groups | Description |
|--------|--------|-------------|
| **core** | respond, info, help | Essential tools |
| **files** | read, write, file-manage, search | File operations |
| **shell** | shell-safe, shell-exec, shell-root | Command execution |
| **systemd** | services, service-control, service-config, journals | Service management |
| **network** | network-info, network-diag, network-config, firewall | Network operations |
| **dbus** | dbus-intro, dbus-call, dbus-monitor | D-Bus operations |
| **monitoring** | monitoring, processes, process-control, logs | System monitoring |
| **git** | git-read, git-write, git-remote | Version control |
| **devops** | containers, container-control, deploy, k8s-read | DevOps tools |
| **security** | auth, sso, secrets, audit, crypto | Security operations |
| **database** | db-read, db-write, db-admin | Database operations |
| **ovs** | ovs-info, ovs-config | Open vSwitch |
| **system** | system-power, system-config, disk-format, user-admin | Restricted admin |

### Presets

| Preset | Groups | Tools | Use Case |
|--------|--------|-------|----------|
| `minimal` | respond | ~3 | Chat only |
| `safe` | respond, info, read, search | ~18 | Read-only operations |
| `developer` | + write, shell-safe, git-read | ~28 | Development workflow |
| `sysadmin` | + services, network-info, logs, monitoring | ~32 | System administration |
| `architect` | + dbus-intro, architect-view | ~26 | Architecture analysis |
| `security` | + auth, audit | ~24 | Security operations |
| `full-admin` | All including restricted | ~40 | Full access (localhost only) |

### Security Levels

| Level | Access Zone | Description |
|-------|-------------|-------------|
| `public` | Any IP | Safe, read-only tools |
| `standard` | Any IP | Normal operations |
| `elevated` | Private/Mesh | System modifications |
| `restricted` | Localhost/Mesh only | Dangerous commands |

### IP-Based Access Zones

| Zone | IP Ranges | Access |
|------|-----------|--------|
| `Localhost` | 127.0.0.1, ::1 | Full access |
| `TrustedMesh` | Tailscale (100.64-127.x.x), Netmaker (10.101-103.x.x), etc. | Full access |
| `PrivateNetwork` | 192.168.x.x, 10.x.x.x, 172.16-31.x.x | Up to elevated |
| `Public` | Everything else | Public/standard only |

---

## 2. Compact Mode (Lazy Loading)

Exposes only 4-5 meta-tools instead of all tools directly:

```
┌─────────────────────────────────────────────────────────────────┐
│  Compact Mode Meta-Tools                                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  list_tools      → Browse available tools by category           │
│  search_tools    → Find tools by keyword                        │
│  get_tool_schema → Get input schema before executing            │
│  execute_tool    → Execute any tool by name                     │
│  batch_execute   → Run multiple tools in sequence (optional)    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Benefits

- **~95% context token savings** - Only 4 tool definitions instead of 750+
- **Bypasses 40-tool limit entirely** - All tools accessible via `execute_tool`
- **Better LLM reasoning** - Fewer choices = more focused decisions
- **Works with any client** - Cursor, Gemini CLI, Claude Desktop

### Workflow

```
1. LLM calls list_tools(category: "systemd")
   → Returns: [{name: "systemd_restart", description: "..."}]

2. LLM calls get_tool_schema(tool_name: "systemd_restart")  
   → Returns: {input_schema: {properties: {unit: {type: "string"}}}}

3. LLM calls execute_tool(tool_name: "systemd_restart", arguments: {unit: "nginx"})
   → Returns: {success: true, result: "Service restarted"}
```

### Client Auto-Detection

Compact mode is auto-enabled for:
- Gemini CLI (`gemini`, `aistudio`, `google-genai`)
- Any client with "compact" in name

Full mode for:
- Cursor (needs direct tools)
- Claude Desktop

---

## 3. Context-Aware Loading

Automatically suggests and enables tool groups based on conversation.

### Context Signals

| Signal Type | Example | Detected Groups |
|-------------|---------|-----------------|
| **File extension** | `/etc/nginx/nginx.conf` | read, services |
| **File extension** | `myapp.service` | services, service-control |
| **Keyword** | "docker container" | containers |
| **Keyword** | "git commit" | git-read, git-write |
| **Intent** | "restart the service" | service-control |
| **Intent** | "debug the issue" | logs, monitoring |
| **Explicit** | "I'm working on networking" | network-* groups |

### Usage

```rust
use op_mcp_aggregator::{ContextAwareTools, ToolGroups};

// Create context tracker
let mut ctx_tools = ContextAwareTools::new(40);

// Observe user message
ctx_tools.observe_message("I need to restart the nginx service and check logs");

// Get suggestions
let suggestions = ctx_tools.suggest_groups(&groups);
// → [{group: "services", confidence: 75}, {group: "service-control", confidence: 70}]

// Auto-enable high-confidence groups
let enabled = ctx_tools.auto_enable(&mut groups);
// → ["services", "service-control", "logs"]
```

### Confidence Scoring

| Signal | Points |
|--------|--------|
| Explicit domain mention | +50 |
| File extension match | +30 |
| Keyword match | +25 |
| Intent match | +20 |
| **Auto-enable threshold** | **≥70** |

---

## Combined Workflow

```
User Message
     │
     ▼
┌─────────────────┐
│ Context Analysis│ ← Extract files, keywords, intent
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Suggest Groups  │ ← Match to tool groups
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Auto-Enable     │ ← Enable high-confidence groups
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Execute Tool    │ ← Via direct call or execute_tool meta-tool
└─────────────────┘
```

---

## Configuration

### Environment Variables

```bash
# Trusted network prefixes (comma-separated)
OP_TRUSTED_NETWORKS=10.50.,10.99.

# Static files directory
OP_WEB_STATIC_DIR=/var/www/op-dbus

# Server port
PORT=8080
```

### MCP Client Configuration

After saving a profile in Groups Admin, copy the generated config:

**Cursor** (`~/.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "op-dbus-myprofile": {
      "url": "http://localhost:8080/mcp/groups/myprofile",
      "transport": "sse"
    }
  }
}
```

**Gemini CLI**:
```json
{
  "mcpServers": {
    "op-dbus-myprofile": {
      "url": "http://localhost:8080/mcp/groups/myprofile",
      "transport": "sse"
    }
  }
}
```

---

## Files

| File | Purpose |
|------|---------|
| `crates/op-mcp-aggregator/src/groups.rs` | Tool groups, security levels, access zones |
| `crates/op-mcp-aggregator/src/compact.rs` | Compact mode meta-tools |
| `crates/op-mcp-aggregator/src/context.rs` | Context-aware suggestions |
| `crates/op-web/src/groups_admin.rs` | Web UI for group management |
| `crates/op-web/src/mcp_picker.rs` | Legacy tool picker |
