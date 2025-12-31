# Context Handoff - Tool Groups & Repo Consolidation

> **Date:** Dec 31, 2024
> **For:** Claude Code continuation

---

## Current Task Status

### ✅ Completed: Tool Groups Admin UI

Built a comprehensive tool management system for MCP:

| Feature | Status | Location |
|---------|--------|----------|
| Groups Admin UI | ✅ Done | `/groups-admin` endpoint |
| Compact Mode (4 meta-tools) | ✅ Done | `op-mcp-aggregator/src/compact.rs` |
| Context-Aware Loading | ✅ Done | `op-mcp-aggregator/src/context.rs` |
| IP-Based Access Control | ✅ Done | `op-mcp-aggregator/src/groups.rs` |
| Documentation | ✅ Done | `docs/TOOL-LOADING-ARCHITECTURE.md` |

**To test:** 
```bash
cd /home/jeremy/git/op-dbus-v2
./target/release/op-web-server
# Open http://localhost:8080/groups-admin
```

---

## 🚨 Pending: Repo Consolidation

### The Problem

Two separate repos exist with **NO common git history**:

| Repo | URL | Commits | Key Unique Files |
|------|-----|---------|------------------|
| `op-dbus-v2` | github.com/repr0bated/op-dbus-v2 | 1 | lazy_tools.rs, server.rs, router.rs |
| `op-dbus-v2.1` | github.com/repr0bated/op-dbus-v2.1 | 11 | op-mcp-aggregator/, groups_admin.rs |

**Confusingly:** Local dir `/home/jeremy/git/op-dbus-v2` tracks `op-dbus-v2.1` remote!

### Files ONLY in op-dbus-v2 (need to merge):
```
crates/op-mcp/src/lazy_tools.rs      ← lazy loading implementation
crates/op-mcp/src/server.rs
crates/op-mcp/src/router.rs
crates/op-mcp/src/config.rs
crates/op-mcp/src/tool_adapter.rs
crates/op-mcp/src/tool_adapter_orchestrated.rs
crates/op-mcp/src/external_client.rs
crates/op-mcp/src/http_server.rs
crates/op-mcp/SETUP.md
crates/op-mcp/docs/
crates/op-mcp-old/src/hybrid_scanner.rs
crates/op-mcp-old/src/introspection_*.rs
```

### Files ONLY in op-dbus-v2.1 (current work):
```
crates/op-mcp-aggregator/           ← NEW: tool groups, compact mode
crates/op-web/src/groups_admin.rs   ← NEW: admin UI
crates/op-web/src/mcp_picker.rs
crates/op-mcp/src/protocol.rs
crates/op-mcp/src/sse.rs
crates/op-tools/src/security.rs
crates/op-tools/src/builtin/openflow_tools.rs
crates/op-tools/src/builtin/rtnetlink_tools.rs
crates/op-plugins/src/state_plugins/full_system.rs
docs/TOOL-LOADING-ARCHITECTURE.md
```

### Recommended Action

```bash
# 1. Clone v2 for comparison
cd /tmp
git clone https://github.com/repr0bated/op-dbus-v2.git op-dbus-v2-source

# 2. Copy unique files from v2 to v2.1
cd /home/jeremy/git/op-dbus-v2
cp /tmp/op-dbus-v2-source/crates/op-mcp/src/lazy_tools.rs crates/op-mcp/src/
cp /tmp/op-dbus-v2-source/crates/op-mcp/src/server.rs crates/op-mcp/src/
# ... etc

# 3. Commit and push to v2.1
git add -A
git commit -m "feat: merge unique files from op-dbus-v2"
git push origin master
```

---

## Architecture: Tool Loading (What We Built)

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
│  └─────────────────┘    └─────────────────┘    └─────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### Key New Crate: op-mcp-aggregator

```
crates/op-mcp-aggregator/
├── src/
│   ├── lib.rs           # Main exports
│   ├── groups.rs        # Tool groups, security levels, IP access zones
│   ├── compact.rs       # 4 meta-tools (list_tools, execute_tool, etc.)
│   ├── context.rs       # Context-aware suggestions
│   ├── aggregator.rs    # Main orchestrator
│   ├── config.rs        # Configuration
│   ├── client.rs        # MCP client
│   ├── cache.rs         # Tool caching
│   └── profile.rs       # Profile management
├── Cargo.toml
└── README.md
```

### Security Levels & IP Zones

| Security Level | Who Can Access |
|----------------|----------------|
| `public` | Any IP |
| `standard` | Any IP |
| `elevated` | Localhost, Mesh VPN, Private Network |
| `restricted` | Localhost, Mesh VPN only |

| Access Zone | IP Ranges |
|-------------|-----------|
| `Localhost` | 127.0.0.1, ::1 |
| `TrustedMesh` | Tailscale 100.64-127.x.x, Netmaker 10.101-103.x.x |
| `PrivateNetwork` | 192.168.x.x, 10.x.x.x, 172.16-31.x.x |
| `Public` | Everything else |

---

## Commands Reference

```bash
# Build
cd /home/jeremy/git/op-dbus-v2
cargo build --release -p op-web

# Run server
./target/release/op-web-server

# Check compilation
cargo check --workspace

# Push changes
git add -A && git commit -m "message" && git push origin master
```

---

## URLs When Running

| URL | Purpose |
|-----|---------|
| http://localhost:8080/groups-admin | **Tool Groups Admin UI** |
| http://localhost:8080/mcp-picker | Legacy tool picker |
| http://localhost:8080/api/health | Health check |
| http://localhost:8080/api/tools | List tools (JSON) |

---

## Quick Task for Claude Code (Sonnet is fine)

```bash
# 1. Clone op-dbus-v2 source
git clone https://github.com/repr0bated/op-dbus-v2.git /tmp/v2-source

# 2. Copy unique files from v2 to current repo
cd /home/jeremy/git/op-dbus-v2

# MCP files
cp /tmp/v2-source/crates/op-mcp/src/lazy_tools.rs crates/op-mcp/src/
cp /tmp/v2-source/crates/op-mcp/src/server.rs crates/op-mcp/src/
cp /tmp/v2-source/crates/op-mcp/src/router.rs crates/op-mcp/src/
cp /tmp/v2-source/crates/op-mcp/src/config.rs crates/op-mcp/src/
cp /tmp/v2-source/crates/op-mcp/src/tool_adapter.rs crates/op-mcp/src/
cp /tmp/v2-source/crates/op-mcp/src/tool_adapter_orchestrated.rs crates/op-mcp/src/
cp /tmp/v2-source/crates/op-mcp/src/external_client.rs crates/op-mcp/src/
cp /tmp/v2-source/crates/op-mcp/src/http_server.rs crates/op-mcp/src/
cp -r /tmp/v2-source/crates/op-mcp/docs crates/op-mcp/
cp /tmp/v2-source/crates/op-mcp/SETUP.md crates/op-mcp/

# MCP-old introspection files
cp /tmp/v2-source/crates/op-mcp-old/src/hybrid_scanner.rs crates/op-mcp-old/src/
cp /tmp/v2-source/crates/op-mcp-old/src/introspection_cache.rs crates/op-mcp-old/src/
cp /tmp/v2-source/crates/op-mcp-old/src/introspection_parser.rs crates/op-mcp-old/src/
cp /tmp/v2-source/crates/op-mcp-old/src/comprehensive_introspection.rs crates/op-mcp-old/src/
cp /tmp/v2-source/crates/op-mcp-old/src/consolidated_introspection.rs crates/op-mcp-old/src/

# 3. Check if it compiles (may need to update mod.rs files)
cargo check --workspace

# 4. Commit and push
git add -A
git commit -m "feat: merge unique files from op-dbus-v2 repo"
git push origin master
```

**Note:** After copying, you may need to add `mod` declarations in `lib.rs` files. Check for compilation errors.

---

## Questions to Answer

1. Should we consolidate repos into one? Which name to keep?
2. Should `lazy_tools.rs` from v2 be merged into `op-mcp-aggregator`?
3. Is the user on "xray server" a separate machine from localhost?

---

## Git Status

```
Local: /home/jeremy/git/op-dbus-v2
Remote: https://github.com/repr0bated/op-dbus-v2.1.git (origin)
Branch: master
Last commit: 5118556 chore: sync all pending changes
```

All changes pushed as of this handoff.
