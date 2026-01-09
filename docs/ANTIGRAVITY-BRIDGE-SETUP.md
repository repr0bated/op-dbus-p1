# Antigravity Bridge Setup Guide

This guide explains how to use your **Antigravity IDE subscription** as the LLM backend for op-dbus, with **zero additional API charges**.

## How It Works

```
op-dbus chatbot
     │
     │ HTTP POST to localhost:7788/v1/chat/completions
     ▼
┌─────────────────────────────────────┐
│  Antigravity Bridge Extension       │
│  (runs inside Antigravity IDE)      │
│                                     │
│  Uses vscode.lm API to access       │
│  IDE's authenticated LLM session    │
└─────────────────────────────────────┘
     │
     │ (Antigravity's authenticated session)
     ▼
   Claude / GPT / Gemini
   (Your enterprise subscription)
```

## Step 1: Build the Extension

```bash
cd /path/to/op-dbus
./scripts/build-antigravity-extension.sh
```

This creates `extensions/antigravity-bridge/antigravity-bridge-0.1.0.vsix`

## Step 2: Install in Antigravity IDE

1. Open Antigravity IDE
2. Press `Ctrl+Shift+P`
3. Type "Extensions: Install from VSIX"
4. Select the `.vsix` file

## Step 3: Verify Bridge is Running

1. Look for `AG:7788` in the status bar (bottom right)
2. Press `Ctrl+Shift+P` → "Antigravity Bridge: Test LLM Access"
3. Check the output panel for detected APIs

## Step 4: Configure op-dbus

```bash
# On the server running op-dbus
export ANTIGRAVITY_ENABLED=true
export ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:7788
export LLM_PROVIDER=antigravity

# Or add to systemd service
sudo tee /etc/systemd/system/op-web.service.d/antigravity.conf << 'EOF'
[Service]
Environment="ANTIGRAVITY_ENABLED=true"
Environment="ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:7788"
Environment="LLM_PROVIDER=antigravity"
EOF

sudo systemctl daemon-reload
sudo systemctl restart op-web
```

## Step 5: Test

```bash
# Test the bridge directly
curl http://127.0.0.1:7788/health

# Test chat completion
curl -X POST http://127.0.0.1:7788/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

## Troubleshooting

### Bridge not starting
- Check VS Code version (requires 1.90+)
- Check extension is activated: `Ctrl+Shift+P` → "Antigravity Bridge: Status"

### "No LLM API detected"
- Run "Antigravity Bridge: Test LLM Access" to see available APIs
- Antigravity may use a different API than `vscode.lm`
- Check if you need to enable LLM features in Antigravity settings

### Connection refused on port 7788
- Ensure Antigravity IDE is running
- Ensure the extension is installed and activated
- Check for port conflicts: `lsof -i :7788`

### Remote server can't reach localhost
If op-dbus runs on a remote server and Antigravity runs locally:

```bash
# Option 1: SSH tunnel from local to remote
ssh -R 7788:localhost:7788 user@remote-server

# Option 2: Run a reverse proxy on the remote server
# This exposes your local bridge to the remote server
```

## Security Notes

- The bridge only listens on `127.0.0.1` (localhost)
- No external network access
- Uses your existing Antigravity authentication
- No API keys stored or transmitted

## Benefits

- **Zero API charges** - Uses your existing subscription
- **No API keys needed** - Uses IDE's authentication  
- **Works with any model** - Whatever Antigravity provides
- **Enterprise compliance** - All requests go through your org's account
