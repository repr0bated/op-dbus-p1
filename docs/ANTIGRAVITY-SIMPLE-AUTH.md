# Antigravity Auth - Simple Approach

## TL;DR

Antigravity stores OAuth tokens after first login. Just:

1. **Log in once** (with display) → token persists
2. **Use the persisted token** for headless op-dbus

## Where Antigravity Stores Tokens

After logging in, check these locations:

```bash
# Most likely locations (VS Code fork pattern)
ls -la ~/.config/antigravity/
ls -la ~/.antigravity/
ls -la ~/.config/Code/User/globalStorage/

# Find any token/credential files
find ~/.config -name "*antigravity*" -o -name "*token*" -o -name "*credential*" 2>/dev/null
find ~/.antigravity -type f 2>/dev/null
```

## Simple Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  ONE-TIME SETUP (with display)                                              │
│                                                                              │
│  1. Launch Antigravity IDE                                                   │
│  2. Sign in with Google                                                      │
│  3. Token saved to ~/.config/antigravity/ (or similar)                      │
│  4. Close Antigravity                                                        │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     │ Token persisted
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  HEADLESS USAGE                                                             │
│                                                                              │
│  Option A: Run headless Antigravity (stays logged in)                       │
│            antigravity --disable-gpu --headless .                           │
│                                                                              │
│  Option B: Point op-dbus to the persisted token                             │
│            export GOOGLE_AUTH_TOKEN_FILE=~/.config/antigravity/token.json  │
│            export LLM_PROVIDER=antigravity                                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Option A: Keep Antigravity Running Headless

```bash
# Start Antigravity in headless mode (already logged in)
antigravity --disable-gpu . &

# It stays authenticated, op-dbus can use the bridge
export ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333
export LLM_PROVIDER=antigravity
```

## Option B: Extract and Reuse Token

```bash
# After logging in once, find where the token is stored
TOKEN_FILE=$(find ~/.config -name "*token*.json" -path "*antigravity*" 2>/dev/null | head -1)

if [ -n "$TOKEN_FILE" ]; then
    echo "Found token at: $TOKEN_FILE"
    
    # Copy to op-dbus location
    mkdir -p ~/.config/op-dbus
    cp "$TOKEN_FILE" ~/.config/op-dbus/antigravity-token.json
    
    # Configure op-dbus
    export GOOGLE_AUTH_TOKEN_FILE=~/.config/op-dbus/antigravity-token.json
    export LLM_PROVIDER=antigravity
fi
```

## Finding the Token (After Login)

```bash
#!/bin/bash
# find-antigravity-token.sh

echo "Searching for Antigravity OAuth tokens..."

# Common VS Code-based app storage patterns
locations=(
    "$HOME/.config/antigravity"
    "$HOME/.antigravity"
    "$HOME/.config/Code/User/globalStorage"
    "$HOME/.vscode/extensions"
    "$HOME/.local/share/antigravity"
)

for loc in "${locations[@]}"; do
    if [ -d "$loc" ]; then
        echo "Checking: $loc"
        find "$loc" -name "*.json" -exec grep -l "access_token\|refresh_token" {} \; 2>/dev/null
    fi
done

# Also check for gcloud ADC (might be shared)
if [ -f "$HOME/.config/gcloud/application_default_credentials.json" ]; then
    echo "Found gcloud ADC: $HOME/.config/gcloud/application_default_credentials.json"
fi
```

## Systemd Service (Keep Antigravity Running)

```ini
# /etc/systemd/user/antigravity-headless.service
[Unit]
Description=Antigravity IDE (Headless)
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/antigravity --disable-gpu --no-sandbox /home/jeremy/git/op-dbus-v2
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
```

```bash
# Enable
systemctl --user daemon-reload
systemctl --user enable antigravity-headless
systemctl --user start antigravity-headless
```

## Summary

| Approach | Complexity | When to Use |
|----------|------------|-------------|
| Keep Antigravity running | Simple | Development, always-on workstation |
| Extract persisted token | Medium | Headless server, no GUI |
| MCP bridge in extension | More work | Need IDE integration features |

**The simplest path**: Log in once with a display, then either keep Antigravity running headless or copy the token file.
