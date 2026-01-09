# Headless Gemini Token Capture for op-dbus

## The Goal

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         HEADLESS SERVER                                     │
│                                                                              │
│  1. Run Gemini CLI (or trigger Google OAuth)                                │
│  2. Gemini CLI obtains OAuth token                                          │
│  3. Token saved to ~/.gemini/oauth_token.json (or similar)                  │
│  4. We capture that token                                                    │
│  5. Save it for op-dbus: ~/.config/antigravity/token.json                   │
│  6. op-dbus uses token for LLM calls                                        │
│                                                                              │
│  ┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐  │
│  │   Gemini CLI     │ ──▶  │  OAuth Token     │ ──▶  │    op-dbus       │  │
│  │   (auth flow)    │      │  (captured)      │      │  (LLM provider)  │  │
│  └──────────────────┘      └──────────────────┘      └──────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Why This Works

1. **Gemini CLI handles OAuth** - It knows how to authenticate with Google
2. **Token is stored locally** - After auth, the token is saved to disk
3. **We piggyback** - Capture that token for our own use
4. **Same API** - Both Gemini CLI and op-dbus use the same Gemini API

## Token Storage Locations

Different Google tools store tokens in different places:

| Tool | Token Location |
|------|----------------|
| Gemini CLI | `~/.gemini/oauth_token.json` |
| Gemini CLI (alt) | `~/.config/gemini/credentials.json` |
| Google AI SDK | `~/.config/google-generativeai/credentials.json` |
| gcloud | `~/.config/gcloud/application_default_credentials.json` |
| Firebase | `~/.config/firebase/tokens.json` |

## Quick Start

### Method 1: Automatic Capture

```bash
# Run the capture script
./scripts/capture-gemini-oauth.sh

# This will:
# 1. Check for existing tokens
# 2. Try running Gemini CLI to trigger auth
# 3. Watch for token file creation
# 4. Save token for op-dbus
```

### Method 2: Manual (Run Gemini CLI yourself)

```bash
# Terminal 1: Start token watcher
./scripts/antigravity-token-extractor.py --watch

# Terminal 2: Run Gemini CLI (triggers OAuth)
gemini "Hello"
# Complete OAuth flow (device code or browser)

# Token will be captured automatically
```

### Method 3: If Display Needed

If Gemini CLI insists on opening a browser:

```bash
# Force virtual display mode
./scripts/capture-gemini-oauth.sh --force-display

# This spins up a headless Wayland compositor,
# runs the auth flow there, captures the token
```

## What Happens During Auth

### Scenario A: Device Code Flow (Ideal)

Gemini CLI might support device code flow:

```
$ gemini "Hello"

╔══════════════════════════════════════════════════════════╗
║  To sign in, visit: https://google.com/device            ║
║  Enter code: ABCD-1234                                    ║
╚══════════════════════════════════════════════════════════╝

Waiting for authentication...
```

You enter the code on your phone → Gemini CLI gets the token → We capture it.

### Scenario B: Browser OAuth

If Gemini CLI opens a browser:

1. **With display**: Browser opens, you complete auth
2. **Headless**: Use virtual display (cage/weston) to run headless browser
3. **Capture**: Token is saved after auth completes

## Token Format

Captured token is normalized to:

```json
{
    "access_token": "ya29.xxx...",
    "refresh_token": "1//xxx...",
    "token_type": "Bearer",
    "expires_at": 1234567890,
    "scope": "openid email profile",
    "saved_at": 1234567890,
    "source": "/home/user/.gemini/oauth_token.json"
}
```

## op-dbus Integration

Once captured, configure op-dbus:

```bash
# Environment variables
export GOOGLE_AUTH_TOKEN_FILE=~/.config/antigravity/token.json
export LLM_PROVIDER=antigravity
export LLM_MODEL=gemini-2.0-flash

# Or in /etc/op-dbus/environment
GOOGLE_AUTH_TOKEN_FILE=/home/user/.config/antigravity/token.json
LLM_PROVIDER=antigravity
LLM_MODEL=gemini-2.0-flash
```

## Automatic Token Refresh

op-dbus's `HeadlessOAuthProvider` handles refresh:

1. Loads token from file
2. Checks expiry (with 5-minute buffer)
3. Refreshes using `refresh_token` if expired
4. Saves refreshed token back to file

```rust
// From crates/op-llm/src/headless_oauth.rs
pub async fn get_token(&self) -> Result<String> {
    let token = self.load_token().await?;
    
    if token.is_expired() {
        let new_token = self.refresh_token(&token.refresh_token).await?;
        self.save_token(&new_token).await?;
        return Ok(new_token.access_token);
    }
    
    Ok(token.access_token)
}
```

## Systemd Timer for Refresh

Keep token fresh automatically:

```ini
# ~/.config/systemd/user/antigravity-refresh.timer
[Unit]
Description=Refresh Antigravity OAuth Token

[Timer]
OnBootSec=5min
OnUnitActiveSec=30min

[Install]
WantedBy=timers.target
```

```ini
# ~/.config/systemd/user/antigravity-refresh.service
[Unit]
Description=Refresh Antigravity Token

[Service]
Type=oneshot
ExecStart=/path/to/antigravity-token-extractor.py --watch --timeout 10
```

## Troubleshooting

### "Gemini CLI not found"

Install Gemini CLI:
```bash
# npm
npm install -g @google/gemini-cli

# or pip
pip install google-generativeai
```

### Token not being captured

1. Check where Gemini CLI stores tokens:
   ```bash
   find ~ -name "*gemini*" -name "*.json" 2>/dev/null
   strace -f -e openat gemini --help 2>&1 | grep -i "config\|token\|cred"
   ```

2. Add the path to `TOKEN_PATHS` in the script

### "No refresh_token"

Some OAuth flows don't return a refresh token. Ensure:
- Using `access_type=offline` in OAuth request
- Using `prompt=consent` to force consent screen

### Token expires immediately

The access token expires in ~1 hour. Make sure you have:
- `refresh_token` in the captured token
- `client_id` and `client_secret` for refresh calls

## Security Notes

1. **Token permissions**: Files are chmod 600 (owner read/write only)
2. **Refresh token is sensitive**: Treat like a password
3. **Don't commit tokens**: Add to `.gitignore`
4. **Scope matters**: Only request scopes you need

## Alternative: Just Use API Key

If this is too complex, get a free Gemini API key:

```bash
# Go to https://aistudio.google.com/
# Create API key
export GEMINI_API_KEY="AIza..."
export LLM_PROVIDER=antigravity
# Done!
```

The token capture approach is for when:
- You want to use Gemini CLI's auth instead of managing API keys
- You're using an enterprise Google account without API key access
- You want to leverage existing gcloud ADC credentials
