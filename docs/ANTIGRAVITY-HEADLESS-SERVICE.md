# Antigravity Headless Service

## Overview

Run Antigravity IDE on a headless server with a virtual Wayland display.
Connect via VNC to log in once, then the auth persists.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         HEADLESS SERVER                                     │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  cage (Wayland compositor)                                           │  │
│  │    └── antigravity (IDE)                                             │  │
│  │          └── Logged in with Google account                           │  │
│  │          └── OAuth token stored locally                              │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                              │                                              │
│                              │ Wayland socket                               │
│                              ▼                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  wayvnc (VNC server)                                                 │  │
│  │    └── Listening on :5900                                            │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                              │                                              │
└──────────────────────────────│──────────────────────────────────────────────┘
                               │
                               │ VNC (port 5900)
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         YOUR WORKSTATION                                    │
│                                                                              │
│  $ vncviewer headless-server:5900                                           │
│    └── See Antigravity IDE                                                  │
│    └── Log in with Google (once)                                            │
│    └── Done!                                                                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Install on Headless Server

```bash
# Run as root
sudo ./scripts/setup-antigravity-headless.sh --user jeremy --vnc-port 5900
```

### 2. Connect via VNC

```bash
# From your workstation
vncviewer headless-server:5900

# Or with TigerVNC
vncviewer -SecurityTypes VeNCrypt,TLSPlain headless-server:5900

# Or with any VNC client (RealVNC, Remmina, etc.)
```

### 3. Log In Once

1. Antigravity opens in the VNC window
2. Click "Sign in with Google"
3. Complete authentication
4. **Done!** Token is now stored locally

### 4. Extract Token for op-dbus

```bash
./scripts/antigravity-extract-token.sh

# Configure op-dbus
export GOOGLE_AUTH_TOKEN_FILE=~/.config/antigravity/token.json
export LLM_PROVIDER=antigravity
```

## Services

| Service | Description | Port |
|---------|-------------|------|
| `antigravity-display` | cage + antigravity | Internal Wayland |
| `antigravity-vnc` | wayvnc server | 5900/tcp |

## Commands

```bash
# Control script
antigravity-ctl start     # Start services
antigravity-ctl stop      # Stop services
antigravity-ctl restart   # Restart
antigravity-ctl status    # Check status
antigravity-ctl logs      # View logs (follow mode)
antigravity-ctl connect   # Show connection info

# Direct systemctl
sudo systemctl status antigravity-display
sudo systemctl status antigravity-vnc
sudo journalctl -u antigravity-display -f
```

## Token Persistence

After logging in via VNC, Antigravity stores OAuth tokens locally:

```
~/.config/antigravity/
├── token.json           # Extracted for op-dbus
├── oauth_token.json     # Original Antigravity token (location varies)
└── ...
```

The token includes:
- `access_token` - Short-lived (~1 hour)
- `refresh_token` - Long-lived (use to refresh access_token)

## Automatic Token Refresh

The `HeadlessOAuthProvider` in op-dbus automatically refreshes tokens:

```rust
// op-llm/src/headless_oauth.rs
if token.is_expired() {
    let new_token = self.refresh_token(&token.refresh_token).await?;
    self.save_token(&new_token).await?;
}
```

## Security

### VNC Security

**Default**: VNC is unencrypted. For production:

1. **SSH Tunnel** (recommended):
   ```bash
   # On your workstation
   ssh -L 5900:localhost:5900 user@headless-server
   vncviewer localhost:5900
   ```

2. **VNC Password**:
   ```bash
   # Create password file
   wayvncctl set-password
   # Update service to use --password-file
   ```

3. **Firewall**:
   ```bash
   # Only allow from specific IPs
   ufw allow from 10.0.0.0/24 to any port 5900
   ```

### Token Security

- Token files are `chmod 600`
- Stored in user's home directory
- Don't commit to git (add to .gitignore)

## Troubleshooting

### "Cannot open display"

Wayland compositor not running:
```bash
sudo systemctl status antigravity-display
sudo journalctl -u antigravity-display -n 50
```

### VNC connection refused

Check wayvnc status:
```bash
sudo systemctl status antigravity-vnc
# Is the Wayland socket ready?
ls -la /run/user/$(id -u)/antigravity-0
```

### Black screen in VNC

Might need a short delay for Antigravity to start:
```bash
sudo systemctl restart antigravity-vnc
```

### "No OAuth token found"

You haven't logged in yet:
1. Connect via VNC
2. Log in with Google in Antigravity
3. Run `antigravity-extract-token.sh` again

### Antigravity crashes

Check logs:
```bash
journalctl -u antigravity-display -f
```

Common fixes:
- Add `--no-sandbox` flag
- Check GPU issues: `LIBGL_ALWAYS_SOFTWARE=1`
- Memory issues: check system resources

## Alternative: SSH Tunnel Only (No Public VNC)

If you don't want VNC exposed:

```bash
# 1. Disable VNC service
sudo systemctl disable antigravity-vnc
sudo systemctl stop antigravity-vnc

# 2. Always use SSH tunnel
ssh -L 5900:localhost:5900 user@server
vncviewer localhost:5900
```

## Integration with op-dbus

Once Antigravity is running and you've logged in:

```bash
# Extract token
./scripts/antigravity-extract-token.sh

# Configure op-dbus
cat >> /etc/op-dbus/environment << 'EOF'
GOOGLE_AUTH_TOKEN_FILE=/home/jeremy/.config/antigravity/token.json
LLM_PROVIDER=antigravity
LLM_MODEL=gemini-2.0-flash
EOF

# Restart op-web
sudo systemctl restart op-web
```

Now op-dbus uses the authenticated Antigravity session for LLM calls!
