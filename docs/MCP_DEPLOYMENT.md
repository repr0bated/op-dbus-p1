# MCP Server Deployment Guide

## Quick Start

### 1. Build

```bash
cd /path/to/op-dbus-v2
cargo build --release -p op-mcp
```

### 2. Install Binary

```bash
sudo cp target/release/op-mcp-server /usr/local/sbin/
sudo chmod 755 /usr/local/sbin/op-mcp-server
```

### 3. Create Systemd Services

#### Compact MCP Service (Port 3001)

```bash
sudo tee /etc/systemd/system/op-mcp-compact.service << 'EOF'
[Unit]
Description=OP MCP Compact Server
After=network.target dbus.service
Wants=dbus.service

[Service]
Type=simple
User=root
EnvironmentFile=-/etc/op-dbus/environment
ExecStart=/usr/local/sbin/op-mcp-server --mode compact --http 0.0.0.0:3001 --log-level info
Restart=always
RestartSec=5

NoNewPrivileges=true
ProtectSystem=strict
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
```

#### Agents MCP Service (Port 3002)

```bash
sudo tee /etc/systemd/system/op-mcp-agents.service << 'EOF'
[Unit]
Description=OP MCP Agents Server (Run-on-Connection)
After=network.target dbus.service
Wants=dbus.service

[Service]
Type=simple
User=root
EnvironmentFile=-/etc/op-dbus/environment
ExecStart=/usr/local/sbin/op-mcp-server --mode agents --http 0.0.0.0:3002 --log-level info
Restart=always
RestartSec=5

NoNewPrivileges=true
ProtectSystem=strict
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
```

### 4. Enable and Start

```bash
sudo systemctl daemon-reload
sudo systemctl enable op-mcp-compact op-mcp-agents
sudo systemctl start op-mcp-compact op-mcp-agents
```

### 5. Verify

```bash
# Check services
systemctl status op-mcp-compact op-mcp-agents

# Test compact endpoint
curl http://localhost:3001/health

# Test agents endpoint  
curl http://localhost:3002/health

# List tools via compact
curl -X POST http://localhost:3001/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

## Nginx Reverse Proxy

```nginx
# /etc/nginx/sites-available/op-mcp

# Compact MCP (tool discovery)
location /mcp/compact {
    proxy_pass http://127.0.0.1:3001/;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_buffering off;
    proxy_cache off;
    proxy_read_timeout 86400s;
}

location /mcp/compact/sse {
    proxy_pass http://127.0.0.1:3001/sse;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_buffering off;
    proxy_cache off;
    proxy_read_timeout 86400s;
    chunked_transfer_encoding off;
}

# Agents MCP (run-on-connection)
location /mcp/agents {
    proxy_pass http://127.0.0.1:3002/;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_buffering off;
    proxy_cache off;
    proxy_read_timeout 86400s;
}

location /mcp/agents/sse {
    proxy_pass http://127.0.0.1:3002/sse;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_buffering off;
    proxy_cache off;
    proxy_read_timeout 86400s;
    chunked_transfer_encoding off;
}
```

## Client Configuration

### Cursor MCP Config

```json
{
  "mcpServers": {
    "op-compact": {
      "serverUrl": "https://your-domain.com/mcp/compact",
      "transport": "sse"
    },
    "op-agents": {
      "serverUrl": "https://your-domain.com/mcp/agents",
      "transport": "sse"
    }
  }
}
```

### Claude Desktop Config

```json
{
  "mcpServers": {
    "op-agents": {
      "command": "op-mcp-server",
      "args": ["--mode", "agents"]
    }
  }
}
```

## Recommended Workflow

### For Rust Development

1. **Connect to Agents MCP** (starts rust_pro, memory, etc.)
2. Use `rust_pro_check` to verify compilation
3. Use `sequential_thinking_plan` for complex tasks
4. Use `memory_remember` to store important context
5. Use `context_manager_save` to persist across sessions

### For Tool Discovery

1. **Connect to Compact MCP**
2. Use `list_tools` with category filter
3. Use `search_tools` for specific capabilities
4. Use `get_tool_schema` before calling
5. Use `execute_tool` to run discovered tools

## Troubleshooting

### Service Won't Start

```bash
# Check logs
journalctl -u op-mcp-agents -n 50 -f

# Test binary directly
/usr/local/sbin/op-mcp-server --mode agents --http 0.0.0.0:3002 --log-level debug
```

### Agents Not Starting

```bash
# Check D-Bus connectivity
busctl --system list | grep dbusmcp

# Check agent services
systemctl status 'dbus-agent@*'
```

### SSE Connection Issues

```bash
# Test SSE directly
curl -N http://localhost:3002/sse

# Check nginx buffering is off
nginx -T | grep -A5 'proxy_buffering'
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | info | Log level |
| `MCP_CONFIG_FILE` | - | Path to MCP config JSON |
| `OP_SELF_REPO_PATH` | - | Self-repository for self_* tools |
