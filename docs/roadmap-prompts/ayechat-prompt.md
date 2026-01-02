# Aye Chat Prompt - Get op-dbus-v2 Chatbot Working

Copy everything below the line to Aye Chat:

---

I need help fixing my Rust chatbot project. It's a system administration chatbot that uses tools to manage Linux systems via D-Bus, OVS, rtnetlink, and LXC.

## Project Location
`/home/jeremy/git/op-dbus-v2` (Rust workspace)

## Current Status
- Server runs, 145 tools registered
- Multi-turn execution works (LLM makes multiple tool calls)
- **PROBLEM**: Tools fail with "Tool not found" due to LRU cache eviction

## Bug Already Fixed
I increased `max_loaded_tools` from 100 to 500 in `crates/op-tools/src/registry.rs` line 101.

Need to rebuild and test:
```bash
cd /home/jeremy/git/op-dbus-v2
cargo build --release -p op-web
sudo systemctl restart op-web
```

## Remaining Issues to Fix

### 1. OVS Auto-Install
Tools like `ovs_list_bridges` should auto-install OVS if not available.

**Current behavior:** Fails with "Failed to connect to OVSDB socket"

**Desired behavior:** 
1. Check if OVS is installed/running
2. If not, use the `dbus_packagekit_install_packages` tool to install `openvswitch-switch`
3. Start the service with `dbus_systemd_start_unit`
4. Then retry the OVS operation

**Implementation:** In `crates/op-tools/src/builtin/ovs_tools.rs`, modify the `execute` method to:
```rust
async fn execute(&self, input: Value) -> Result<Value> {
    // Try OVS operation
    match self.try_ovs_operation().await {
        Ok(result) => Ok(result),
        Err(e) if e.to_string().contains("connection failed") || e.to_string().contains("not found") => {
            // OVS not available - try to install it
            info!("OVS not available, attempting to install...");
            
            // Use PackageKit to install
            let pkg_tool = crate::builtin::dbus::PackageKitInstallTool;
            pkg_tool.execute(json!({"packages": ["openvswitch-switch"]})).await?;
            
            // Start the service
            let systemd_tool = crate::builtin::dbus::SystemdStartUnitTool;
            systemd_tool.execute(json!({"unit": "openvswitch-switch.service"})).await?;
            
            // Wait for socket
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Retry
            self.try_ovs_operation().await
        }
        Err(e) => Err(e),
    }
}
```

File: `crates/op-tools/src/builtin/ovs_tools.rs`

### 2. Rtnetlink Works But Has Warnings
```
WARN netlink_packet_route::link::buffer_tool: Specified IFLA_INET6_CONF NLA attribute holds more data
```
This is just a warning from newer kernel, can be ignored.

### 3. Agent Tools Fail (D-Bus service not found)
```
ERROR: The name org.dbusmcp.Agent.BashPro was not provided by any .service files
```
The `agent_*` tools try to call external D-Bus services that don't exist. Either:
- Remove agent tools from registration
- Or make them return "Agent not available" instead of failing

File: `crates/op-tools/src/builtin/agent_tool.rs`

## Key Files

- `crates/op-tools/src/registry.rs` - Tool registry (eviction fixed)
- `crates/op-tools/src/builtin/ovs_tools.rs` - OVS tools
- `crates/op-tools/src/builtin/rtnetlink_tools.rs` - Network tools
- `crates/op-web/src/orchestrator.rs` - Multi-turn chat orchestration
- `/etc/systemd/system/op-web.service` - Systemd service (runs as root)

## Test Commands

After fixes, test with:
```bash
# Check logs
sudo journalctl -u op-web -f

# In browser: http://localhost:8080
# Ask: "List all systemd services"
# Ask: "Show network interfaces"
```

## What Success Looks Like

1. `dbus_systemd_list_units` returns list of services (not "tool not found")
2. `rtnetlink_list_interfaces` returns network interfaces  
3. `ovs_*` tools auto-install OVS if not present, then work
4. No more "Evicted tool" in logs during conversation
5. Chatbot can execute multi-step tasks (create bridge, add ports, etc.)

## Available Tools for Auto-Install

The chatbot already has these tools registered:
- `dbus_packagekit_install_packages` - Install packages via D-Bus PackageKit
- `dbus_systemd_start_unit` - Start systemd services
- `dbus_systemd_restart_unit` - Restart systemd services

So OVS tools can use these internally to self-heal.

## Architecture Summary

```
User → WebSocket → UnifiedOrchestrator → LLM (Gemini)
                         ↓
                   ToolRegistry → Execute Tool → Return Result
                         ↓
                   Multi-turn loop (up to 10 turns)
```

The orchestrator sends tool definitions to Gemini, Gemini returns tool calls, orchestrator executes them, sends results back to Gemini, repeat until Gemini responds without tool calls.

---

End of prompt. Build and test after making changes:
```bash
cargo build --release -p op-web && sudo systemctl restart op-web
```

