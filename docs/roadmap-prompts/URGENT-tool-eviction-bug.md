# URGENT: Tool Registry Eviction Bug

## Copy this entire prompt to Aye Chat:

---

I have a critical bug in my Rust project op-dbus-v2. The tool registry is evicting tools during execution, causing "Tool not found" errors.

## The Bug

In `crates/op-tools/src/registry.rs`, there's an LRU cache or eviction mechanism that removes tools while the chatbot is actively trying to use them.

**Evidence from logs:**
```
INFO op_tools::registry: Evicted tool: ovs_list_bridges
INFO op_tools::registry: Evicted tool: ovs_create_bridge
INFO op_tools::registry: Evicted tool: dbus_systemd_get_unit_status
...
ERROR op_web_server::orchestrator: Tool not found: ovs_list_bridges
```

The tools are registered at startup (145 tools successfully), but during multi-turn execution they get evicted and become unavailable.

## What I Need

1. Find the eviction logic in `crates/op-tools/src/registry.rs`
2. Either:
   - **Remove the eviction entirely** (tools should stay registered for the lifetime of the server)
   - OR **Increase the cache size** to hold all 145+ tools
   - OR **Change eviction to only apply to dynamic/temporary tools**, not built-in tools

## File to Fix

`crates/op-tools/src/registry.rs`

**The exact problem is on line 101:**
```rust
impl Default for LruConfig {
    fn default() -> Self {
        Self {
            max_loaded_tools: 100,  // <-- THIS IS THE BUG! We have 145 tools!
            min_idle_time: Duration::from_secs(300),
            hot_threshold: 10,
            eviction_check_interval: 10,
        }
    }
}
```

## The Fix

Change line 101 from:
```rust
max_loaded_tools: 100,
```
to:
```rust
max_loaded_tools: 500,
```

Or better yet, disable eviction entirely by setting it very high:
```rust
max_loaded_tools: 10000,
```

## Expected Behavior

Once a tool is registered, it should NEVER be evicted during server operation. The registry should hold all tools permanently.

## Build & Test

```bash
cd /home/jeremy/git/op-dbus-v2
cargo build --release -p op-web
sudo systemctl restart op-web
sudo journalctl -u op-web -f
```

Then test by asking the chatbot: "List all OVS bridges"

The tool `ovs_list_bridges` should execute without "Tool not found" error.

---

## Quick Context

This is a Rust workspace. The tool registry is used by a chatbot that executes system administration tools. Multi-turn execution is working (LLM calls tools over multiple rounds), but the eviction bug breaks tool availability mid-conversation.

The fix should be simple - just disable or increase the cache eviction threshold.

