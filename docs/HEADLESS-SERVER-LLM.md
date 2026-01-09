# LLM Access for Headless Servers

## The Problem

You have:
- A **headless server** (no GUI) running op-dbus
- A **workstation** with Cursor/VSCode and an enterprise LLM subscription
- You want the server to use LLM capabilities WITHOUT:
  - Managing API keys
  - Configuring Vertex AI
  - Setting up Google Cloud credentials

## The Solution

Use the **Antigravity Bridge** architecture:

```
┌────────────────────────────────────────────────────────────────┐
│              Your Workstation (GUI, IDE)                       │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Cursor / VSCode with Enterprise Subscription           │   │
│  │                                                         │   │
│  │  Antigravity Bridge Extension                           │   │
│  │  └─ HTTP Server on 127.0.0.1:3333                      │   │
│  │  └─ Uses IDE's existing LLM session                    │   │
│  │  └─ NO API KEYS                                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              │ localhost:3333                   │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  SSH Reverse Tunnel Endpoint                            │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
                              │
                              │ SSH Tunnel (encrypted)
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│              Headless Server (no GUI)                          │
│                                                                 │
│  SSH tunnel makes workstation:3333 available as localhost:3333 │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  op-dbus                                                │   │
│  │                                                         │   │
│  │  /etc/op-dbus/environment:                              │   │
│  │    LLM_PROVIDER=antigravity                             │   │
│  │    ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333        │   │
│  │    # NO API KEYS NEEDED                                 │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
```

## Quick Setup

### 1. On Your Workstation

```bash
# Install the Antigravity Bridge extension in your IDE
# (Cursor, VSCode, or Windsurf)

# Start the bridge (from IDE command palette)
# Command: "Antigravity: Start Bridge"

# Verify it's running
curl http://localhost:3333/health
```

### 2. Create SSH Tunnel

**Option A: From workstation (reverse tunnel)**
```bash
ssh -R 3333:127.0.0.1:3333 user@headless-server
```

**Option B: From server (local tunnel)**
```bash
ssh -L 3333:127.0.0.1:3333 user@workstation
```

### 3. On Headless Server

```bash
# Configure op-dbus
echo 'LLM_PROVIDER=antigravity' | sudo tee -a /etc/op-dbus/environment
echo 'ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333' | sudo tee -a /etc/op-dbus/environment

# Restart op-dbus
sudo systemctl restart op-web

# Test
curl -X POST http://localhost:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"Hello","user_id":"test"}'
```

## Persistent Tunnel with systemd

Create `/etc/systemd/system/antigravity-tunnel.service` on the **headless server**:

```ini
[Unit]
Description=Antigravity Bridge SSH Tunnel
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=op-dbus
ExecStart=/usr/bin/ssh -N -L 3333:127.0.0.1:3333 \
    -o ServerAliveInterval=30 \
    -o ServerAliveCountMax=3 \
    -o ExitOnForwardFailure=yes \
    -o StrictHostKeyChecking=accept-new \
    -i /var/lib/op-dbus/.ssh/id_ed25519 \
    user@workstation.local
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
# Generate SSH key for op-dbus user (on server)
sudo -u op-dbus ssh-keygen -t ed25519 -N '' -f /var/lib/op-dbus/.ssh/id_ed25519

# Add public key to workstation's authorized_keys
cat /var/lib/op-dbus/.ssh/id_ed25519.pub
# Copy to: ~/.ssh/authorized_keys on workstation

# Enable and start tunnel
sudo systemctl enable antigravity-tunnel
sudo systemctl start antigravity-tunnel

# Check status
sudo systemctl status antigravity-tunnel
```

## Why Not Just Use API Keys?

If you have API keys, use them directly! Configure:
- `GEMINI_API_KEY` for Gemini
- `ANTHROPIC_API_KEY` for Claude
- `OPENAI_API_KEY` for GPT

The Antigravity Bridge is specifically for when:
1. Your enterprise subscription doesn't provide API keys
2. You want to avoid managing API keys
3. You want to leverage your IDE's existing authenticated session
4. Your organization requires using the enterprise proxy/quota

## Troubleshooting

### Bridge not responding on server

```bash
# Check tunnel is running
ss -tlnp | grep 3333

# Check SSH tunnel status
systemctl status antigravity-tunnel

# Test direct connection
curl -v http://127.0.0.1:3333/health
```

### "Connection refused"

1. Ensure Antigravity Bridge is running in IDE on workstation
2. Ensure SSH tunnel is established
3. Check firewall on both machines

### "No LLM backend available"

1. Check your IDE subscription is active
2. Ensure Copilot/Cursor is authenticated
3. This is NOT an API key issue - check IDE, not credentials

## Related Documents

- [VSCODE-EXTENSION-AUTH.md](./VSCODE-EXTENSION-AUTH.md) - Full extension implementation
- [ANTIGRAVITY-AUTH.md](../ANTIGRAVITY-AUTH.md) - Architecture overview
