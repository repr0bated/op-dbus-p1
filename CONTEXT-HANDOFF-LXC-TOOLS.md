# Context Handoff - LXC Tools Implementation

**Date:** Dec 31, 2024
**Goal:** Enable chatbot to configure full system topology (self-building for marketing)

---

## Current State

```
✅ op-web service running (137 tools, 75 agents)
✅ Gemini 3 preview working
✅ OVS tools: ovs_create_bridge, ovs_add_port, ovs_list_*
✅ OpenFlow tools: openflow_add_flow, openflow_create_privacy_socket
✅ rtnetlink tools: rtnetlink_list_interfaces, rtnetlink_add_address
✅ Self-dev tools: self_build, self_read_file, self_write_file, self_git_*
❌ LXC container tools NOT exposed to chatbot
```

## The Problem

The system prompt (`crates/op-chat/src/system_prompt.rs`) defines a topology with:
- 3 LXC containers (CT 100, 101, 102) for privacy router
- Socket networking (priv_wg, priv_warp, priv_xray)
- Dynamic container sockets (sock_*)

**But there are NO chatbot tools to create/manage LXC containers.**

## The Solution

### 1. LXC Plugin EXISTS but not exposed

Location: `crates/op-plugins/src/state_plugins/lxc.rs`

Already has:
```rust
pub async fn create_container(container: &ContainerInfo) -> Result<()>
pub async fn start_container(ct_id: &str) -> Result<()>
async fn discover_from_ovs(&self) -> Result<Vec<ContainerInfo>>
```

### 2. Need to create tools wrapper

Create: `crates/op-tools/src/builtin/lxc_tools.rs`

```rust
// Tools to implement:
lxc_list_containers {}           // List all LXC containers
lxc_create_container {           // Create container
  id: "100",
  template: "debian-13-standard",
  bridge: "ovs-br0",
  network_type: "socket",        // "socket" or "veth"
  resources: { vcpus: 1, memory_mb: 512, storage_gb: 4 }
}
lxc_start_container { id: "100" }
lxc_stop_container { id: "100" }
lxc_delete_container { id: "100" }
```

### 3. Register in mod.rs

File: `crates/op-tools/src/builtin/mod.rs`

Add:
```rust
mod lxc_tools;
// In register_response_tools():
lxc_tools::register_lxc_tools(registry).await?;
```

## Key Files

| File | Purpose |
|------|---------|
| `crates/op-chat/src/system_prompt.rs` | Target topology spec (909 lines) |
| `crates/op-tools/src/builtin/mod.rs` | Tool registration |
| `crates/op-plugins/src/state_plugins/lxc.rs` | LXC plugin with create/start |
| `COMPREHENSIVE-DOC-ANALYSIS.md` | Full documentation review |

## Target Topology (from system prompt)

```
ovs-br0 (single bridge)
├── mgmt0 (internal, host management)
├── nm0 (netmaker mesh, enslaved)
├── {uplink} (physical NIC, introspected)
├── priv_wg (CT 100 - WireGuard gateway)
├── priv_warp (CT 101 - Cloudflare WARP)
├── priv_xray (CT 102 - XRay client)
└── sock_* (dynamic container sockets)
```

## Build & Deploy

```bash
cd /home/jeremy/git/op-dbus-v2
cargo build --release -p op-web
sudo cp target/release/op-web-server /usr/local/sbin/
sudo systemctl restart op-web
curl http://localhost:8080/api/tools | jq '.tools | length'
```

## Environment

```
OP_SELF_REPO_PATH=/home/jeremy/git/op-dbus-v2
Service: /etc/systemd/system/op-web.service
Binary: /usr/local/sbin/op-web-server
Config: /etc/op-dbus/environment
```

## Test Command

Once LXC tools are added, test with chatbot:
```
"Configure the privacy router topology with 3 containers"
```

Should execute:
1. `ovs_create_bridge {"name": "ovs-br0"}`
2. `lxc_create_container {"id": "100", ...}`
3. `ovs_add_port {"bridge": "ovs-br0", "port": "priv_wg", "type": "internal"}`
4. ... continue for all 3 containers

---

## Quick Start for New Context

```
Read this file, then:
1. Create crates/op-tools/src/builtin/lxc_tools.rs
2. Reference crates/op-plugins/src/state_plugins/lxc.rs for implementation
3. Register tools in crates/op-tools/src/builtin/mod.rs
4. Build and deploy
5. Test via chatbot
```
