# Chatbot Authentication

## Overview

The op-dbus chatbot uses LLM providers for AI capabilities. Authentication is handled through:

1. **Antigravity Headless** (Recommended) - OAuth token from Antigravity IDE
2. **API Keys** (Fallback) - Direct API keys for Gemini, Anthropic, etc.

## Quick Start

### Option 1: Antigravity Headless (Enterprise, No API Charges)

```bash
# 1. Install dependencies
sudo apt install cage wayvnc

# 2. Install systemd services
sudo cp deploy/systemd/antigravity-*.service /etc/systemd/system/
sudo systemctl daemon-reload

# 3. Start Antigravity with virtual display
sudo systemctl start antigravity-display antigravity-vnc

# 4. Connect via VNC and log in with Google
vncviewer localhost:5900
# -> Sign in with your Google account in Antigravity IDE

# 5. Extract the OAuth token
./scripts/antigravity-extract-token.sh

# 6. Configure op-dbus
echo 'GOOGLE_AUTH_TOKEN_FILE=/home/jeremy/.config/antigravity/token.json' | sudo tee -a /etc/op-dbus/environment
echo 'LLM_PROVIDER=antigravity' | sudo tee -a /etc/op-dbus/environment

# 7. Restart chatbot
sudo systemctl restart op-web
```

### Option 2: API Key (Simple)

```bash
# Add to /etc/op-dbus/environment
GEMINI_API_KEY=your-api-key-here
LLM_PROVIDER=antigravity

# Restart
sudo systemctl restart op-web
```

## Authentication Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         HEADLESS SERVER                                     │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  cage (Wayland compositor)                                           │  │
│  │    └── antigravity (IDE)                                             │  │
│  │          └── User logs in with Google account (once)                 │  │
│  │          └── OAuth token stored locally                              │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                              │                                              │
│                              │ Token extracted                              │
│                              ▼                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  ~/.config/antigravity/token.json                                    │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                              │                                              │
│                              │ HeadlessOAuthProvider                        │
│                              ▼                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  op-web (chatbot)                                                    │  │
│  │    └── AntigravityProvider                                           │  │
│  │          └── Makes API calls with OAuth token                        │  │
│  │          └── Auto-refreshes when expired                             │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                              │                                              │
│                              │ API requests with Bearer token               │
│                              ▼                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  Gemini API                                                          │  │
│  │    └── Enterprise billing (Code Assist subscription)                 │  │
│  │    └── ZERO API charges                                              │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `GOOGLE_AUTH_TOKEN_FILE` | Path to OAuth token JSON | For OAuth |
| `GEMINI_API_KEY` | Gemini API key | For API key auth |
| `ANTHROPIC_API_KEY` | Anthropic API key | Optional |
| `LLM_PROVIDER` | Provider to use (`antigravity`, `gemini`, `anthropic`) | Optional |
| `LLM_MODEL` | Default model | Optional |

## Services

| Service | Description | Port |
|---------|-------------|------|
| `antigravity-display` | cage + antigravity | Internal Wayland |
| `antigravity-vnc` | wayvnc server | 5900/tcp |
| `op-web` | Chatbot web server | 8080/tcp |

## Commands

```bash
# Check status
sudo systemctl status antigravity-display antigravity-vnc op-web

# View logs
journalctl -u antigravity-display -f
journalctl -u op-web -f

# Restart everything
sudo systemctl restart antigravity-display antigravity-vnc op-web

# Check token
cat ~/.config/antigravity/token.json | jq .
```

## Troubleshooting

### "No LLM providers configured"

1. Check if token file exists:
   ```bash
   ls -la ~/.config/antigravity/token.json
   ```

2. If missing, extract from Antigravity:
   ```bash
   ./scripts/antigravity-extract-token.sh
   ```

3. Check environment:
   ```bash
   cat /etc/op-dbus/environment | grep -E 'GOOGLE_AUTH|LLM_'
   ```

### "Authentication failed (401)"

Token expired. Re-extract:

```bash
# Connect to Antigravity and ensure you're logged in
vncviewer localhost:5900

# Re-extract token
./scripts/antigravity-extract-token.sh

# Restart chatbot
sudo systemctl restart op-web
```

### VNC connection refused

```bash
# Check if services are running
sudo systemctl status antigravity-display antigravity-vnc

# Check Wayland socket
ls -la /run/user/1000/antigravity-0

# Restart services
sudo systemctl restart antigravity-display
sleep 5
sudo systemctl restart antigravity-vnc
```

## Security Notes

1. **Token file permissions**: Set to `chmod 600`
2. **Refresh token**: Long-lived, protect like a password
3. **Don't commit tokens**: Add to `.gitignore`
4. **VNC security**: Use SSH tunnel for remote access

```bash
# Secure VNC access via SSH tunnel
ssh -L 5900:localhost:5900 user@server
vncviewer localhost:5900
```
