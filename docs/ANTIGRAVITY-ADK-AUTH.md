# Antigravity ADK Authentication

## What We Know from Docs

Antigravity is Google's AI-powered IDE (VS Code fork) with:
- Built-in Agent system
- MCP integration via MCP Store
- CLI interface with `antigravity chat` subcommand

## Authentication Flow

Antigravity likely uses **Google OAuth** internally, handled by the ADK (Agent Development Kit).

### Token Location (Probable)

Based on typical Google tool patterns:

```bash
# Check these locations
ls -la ~/.config/antigravity/
ls -la ~/.antigravity/
ls -la ~/.config/google-antigravity/
ls -la ~/.local/share/antigravity/

# Also check gcloud ADC (might share auth)
cat ~/.config/gcloud/application_default_credentials.json
```

### MCP Integration Approach

Instead of extracting tokens, we can use Antigravity's MCP support to connect to our op-dbus server:

```json
// In Antigravity MCP config (~/.config/antigravity/mcp_config.json or via MCP Store)
{
  "servers": {
    "op-dbus": {
      "url": "https://op-dbus.ghostbridge.tech/mcp/agents",
      "transport": "sse"
    }
  }
}
```

### CLI Chat Mode

From the docs:
```bash
antigravity chat  # Pass prompt to run in chat session
```

This might work headless and expose the auth token.

## Investigation Steps

1. **Find where Antigravity stores auth:**
   ```bash
   # Watch file access during auth
   strace -f -e openat antigravity 2>&1 | grep -i "config\|token\|cred\|auth"
   ```

2. **Check MCP config location:**
   ```bash
   find ~ -name "mcp*" -o -name "*antigravity*" 2>/dev/null | head -20
   ```

3. **Try CLI chat mode:**
   ```bash
   # See if it works headless
   echo "Hello" | antigravity chat
   ```

## Connecting op-dbus to Antigravity

### Option 1: Antigravity Connects to op-dbus (Recommended)

Add our MCP server to Antigravity's MCP Store:

1. Open Antigravity
2. Go to MCP Store ("..." dropdown)
3. Click "Manage MCP Servers" → "View raw config"
4. Add:

```json
{
  "op-dbus-agents": {
    "serverUrl": "https://op-dbus.ghostbridge.tech/mcp/agents"
  }
}
```

Now Antigravity can use our agents (memory, context_manager, sequential_thinking, etc.)

### Option 2: op-dbus Uses Antigravity's Auth

If we find where Antigravity stores tokens:

```bash
# Set token file for op-dbus
export GOOGLE_AUTH_TOKEN_FILE=~/.config/antigravity/oauth_token.json
export LLM_PROVIDER=antigravity
```

## Summary

The cleanest approach is **Option 1**: Let Antigravity connect to op-dbus via MCP. Antigravity handles its own auth, and our agents become available as tools in the IDE.

This avoids:
- Token extraction complexity
- Auth token management
- Potential TOS issues with token reuse
