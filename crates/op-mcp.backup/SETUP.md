# MCP Server Setup Guide

## Your API Keys (from ~/.bashrc)

You already have these API keys configured:

```bash
# GitHub
GH_TOKEN='your-github-token-here'

# Hugging Face
HF_TOKEN="your-huggingface-token-here"

# Cloudflare
CF_DNS_ZONE_TOKEN='your-cloudflare-dns-zone-token-here'
CF_ACCOUNT_ID='your-cloudflare-account-id-here'
CF_GLOBAL_API_KEY='your-cloudflare-global-api-key-here'

# Pinecone
PINECONE_API_KEY='your-pinecone-api-key-here'

# Paperspace
PAPERSPACE_API_KEY='your-paperspace-api-key-here'

# Hostkey
HOSTKEY_API_KEY='your-hostkey-api-key-here'

# Zen Coder
ZEN_CODER_CLIENT_ID='your-zen-coder-client-id-here'
ZEN_CODER_SECRET_KEY="your-zen-coder-secret-key-here"
```

## Installation

### 1. Build op-mcp-server

```bash
cd /home/jeremy/op-dbus-v2
cargo build --release -p op-mcp
```

### 2. Reload bashrc

```bash
source ~/.bashrc
```

### 3. Test the server

```bash
# Test standalone
/home/jeremy/op-dbus-v2/target/release/op-mcp-server

# Or with MCP config
MCP_CONFIG_FILE=/home/jeremy/op-dbus-v2/crates/op-mcp/mcp-config.json \
  /home/jeremy/op-dbus-v2/target/release/op-mcp-server
```

## Available MCP Servers

### Configured in mcp-config.json:

1. **github** - GitHub operations (using your GH_TOKEN)
2. **filesystem** - File system access to /home/jeremy
3. **brave-search** - Web search (needs Brave API key)
4. **postgres** - PostgreSQL database access
5. **sequential-thinking** - Enhanced reasoning
6. **memory** - Persistent memory
7. **fetch** - HTTP requests
8. **puppeteer** - Browser automation

### Claude Desktop Integration

Your MCP servers are also configured for Claude Desktop in:
`~/.claude/mcp.json`

## Adding More MCP Servers

### Popular MCP Servers You Can Add:

#### Google Drive
```bash
npm install -g @modelcontextprotocol/server-gdrive
```

Add to mcp-config.json:
```json
{
  "name": "gdrive",
  "command": "mcp-server-gdrive",
  "args": [],
  "auth_method": "none"
}
```

#### Slack
```bash
npm install -g @modelcontextprotocol/server-slack
```

#### Git
```bash
npm install -g @modelcontextprotocol/server-git
```

#### Everything
```bash
npm install -g @modelcontextprotocol/server-everything
```

## Testing Tools

### List available tools:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | \
  MCP_CONFIG_FILE=/home/jeremy/op-dbus-v2/crates/op-mcp/mcp-config.json \
  /home/jeremy/op-dbus-v2/target/release/op-mcp-server
```
