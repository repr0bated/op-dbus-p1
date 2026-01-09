# Implementation Checklist

## Current Implementation Guides

| File | Status | Description |
|------|--------|-------------|
| `SKILL-IMPLEMENTATION.md` | ✅ Created | Skills system with constraints, transformations |
| `MCP-GRPC-FIX.md` | ✅ Created | mcp-proxy for daemon connectivity |
| `GOOGLE-LOGIN.md` | ✅ Created | OAuth 2.0 web authentication |
| `REMOVE-LAZY.md` | ✅ Created | Replace lazy_static/once_cell patterns |
| `ANTIGRAVITY-INTEGRATION.md` | ✅ Created | Google auth + agentic capabilities |

## Additional Guides Needed?

| Task | Need .md? | Notes |
|------|-----------|-------|
| Agent Capabilities Array | ❓ | Was in early `IMPLEMENTATION.md` - regenerate? |
| Workstack Caching | ❓ | Was in early `IMPLEMENTATION.md` |
| Pattern Tracker | ❓ | Was in early `IMPLEMENTATION.md` |
| gRPC Proto Definitions | ❓ | Was in early `IMPLEMENTATION.md` |
| D-Bus Agent Discovery | ❌ | Already implemented in `dbus_service.rs` |
| MCP Aggregator | ❌ | Already implemented |
| Chat Actor/Brain | ❌ | Already implemented |

## Priority Order

1. **REMOVE-LAZY.md** - Clean up technical debt first
2. **ANTIGRAVITY-INTEGRATION.md** - Enable enterprise auth
3. **SKILL-IMPLEMENTATION.md** - Add domain knowledge augmentation
4. **MCP-GRPC-FIX.md** - Fix MCP client connectivity
5. **GOOGLE-LOGIN.md** - Web-based auth alternative

## Files with Lazy Patterns to Fix

Based on source files provided:

```bash
# Search for lazy patterns
rg "lazy_static|once_cell|OnceCell|OnceLock|Lazy::new" --type rust

# Known locations:
crates/op-core/src/self_identity.rs  # OnceLock (std library - acceptable)
crates/op-mcp/src/lazy_tools.rs       # Likely has lazy patterns
```

## What Your IDE Claude Needs

Give it the relevant `.md` file and say:

> "Implement the code from this spec. Create the actual source files."

The specs contain:
- Full file paths
- Complete code implementations
- Step-by-step instructions
- Usage examples
