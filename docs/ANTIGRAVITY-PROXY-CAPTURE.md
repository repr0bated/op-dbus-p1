# Antigravity IDE Proxy Capture

## The Problem

Antigravity IDE (and similar IDEs like Cursor) send special headers that identify them as IDE clients:

```
Authorization: Bearer ya29.xxx...
X-Goog-Api-Client: antigravity/1.0.0 gl-js/1.2.3
User-Agent: Antigravity/1.0.0
X-Client-Version: 1.0.0
```

These headers are required to access **Code Assist** features (enterprise subscription).

CLI tools don't get these benefits because they don't send the IDE headers.

## The Solution

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         CAPTURE FLOW                                        │
│                                                                              │
│  ┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐  │
│  │  Antigravity IDE │ ──▶  │   mitmproxy      │ ──▶  │  Google APIs     │  │
│  │                  │      │   (intercept)    │      │                  │  │
│  └──────────────────┘      └────────┬─────────┘      └──────────────────┘  │
│                                     │                                       │
│                                     │ Capture                               │
│                                     ▼                                       │
│                            ┌──────────────────┐                            │
│                            │ session.json     │                            │
│                            │ - OAuth token    │                            │
│                            │ - IDE headers    │                            │
│                            │ - API endpoints  │                            │
│                            └──────────────────┘                            │
│                                     │                                       │
│                                     │ Replay                                │
│                                     ▼                                       │
│  ┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐  │
│  │     op-dbus      │ ──▶  │ AntigravityReplay│ ──▶  │  Google APIs     │  │
│  │   (chatbot)      │      │    Provider      │      │  (thinks it's    │  │
│  │                  │      │  (same headers)  │      │   the IDE)       │  │
│  └──────────────────┘      └──────────────────┘      └──────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Quick Start

### Step 1: Install mitmproxy

```bash
pip install mitmproxy
# or
apt install mitmproxy
```

### Step 2: Run Capture Script

```bash
# With display (workstation)
./scripts/antigravity-proxy-capture.sh

# Headless (server with virtual Wayland)
./scripts/antigravity-proxy-capture.sh --headless
```

### Step 3: Sign In to Antigravity

When Antigravity IDE opens:
1. Sign in with your Google account (Code Assist subscription)
2. Make a request (ask it something)
3. The script captures the OAuth token + headers

### Step 4: Use Captured Session

```bash
# Configure op-dbus
export ANTIGRAVITY_SESSION_FILE=~/.config/antigravity/captured/session.json
export LLM_PROVIDER=antigravity_replay

# Restart service
sudo systemctl restart op-web
```

## What Gets Captured

| Data | Purpose |
|------|----------|
| **OAuth Token** | Authentication with Google APIs |
| **IDE Headers** | Identify as Antigravity IDE (for Code Assist) |
| **API Endpoints** | Know which endpoints the IDE uses |
| **User-Agent** | Mimic IDE's identity |

### Example Captured Headers

```json
{
  "Authorization": "Bearer ya29.xxx...",
  "X-Goog-Api-Client": "antigravity/1.0.0 gl-js/1.2.3 grpc-js/1.8.0",
  "User-Agent": "Antigravity/1.0.0 (Linux; x64)",
  "X-Client-Version": "1.0.0",
  "X-Goog-Request-Reason": "code-assist"
}
```

## Files Created

```
~/.config/antigravity/captured/
├── session.json     # Full captured session
├── token.json       # Latest OAuth token
├── headers.json     # Captured IDE headers
├── proxy.log        # mitmproxy log
└── capture_addon.py # mitmproxy addon
```

## Rust Provider Usage

```rust
use op_llm::antigravity_replay::AntigravityReplayProvider;

// From environment
let provider = AntigravityReplayProvider::from_env()?;

// Or with explicit path
let config = AntigravityReplayConfig {
    session_file: PathBuf::from("/path/to/session.json"),
    default_model: "gemini-2.0-flash".to_string(),
    auto_routing: true,
};
let provider = AntigravityReplayProvider::new(config)?;

// Use like any other provider
let response = provider.chat("auto", vec![
    ChatMessage::user("Hello!"),
]).await?;
```

## Python Client Usage

```python
from antigravity_replay_client import AntigravityReplayClient

# From captured session
client = AntigravityReplayClient.from_captured_session()

# Chat
response = await client.chat("Hello!")
print(response["text"])

# See captured headers
print(client.get_captured_headers())
```

## Token Refresh

OAuth tokens expire after ~1 hour. When you get a 401 error:

```bash
# Re-run the capture script
./scripts/antigravity-proxy-capture.sh

# Sign in again in Antigravity
# New token will be captured
```

For long-running deployments, consider:
1. Capturing the refresh token (if available)
2. Setting up periodic re-capture via cron
3. Using API key instead (free tier, no IDE benefits)

## Security Notes

1. **Session files contain secrets** - chmod 600, don't commit to git
2. **mitmproxy sees all traffic** - only use for capture, not always-on
3. **Token expiration** - OAuth tokens expire, need periodic refresh
4. **Terms of Service** - Check your subscription agreement

## Troubleshooting

### mitmproxy certificate errors

```bash
# Install mitmproxy CA certificate
mitmproxy-ca-file install
```

### Antigravity not using proxy

Electron apps sometimes ignore system proxy settings:

```bash
# Set explicitly for Electron
export ELECTRON_GET_USE_PROXY=1
export NODE_TLS_REJECT_UNAUTHORIZED=0  # Dev only!
```

### No tokens captured

Check if Antigravity is making API requests:

```bash
# Watch proxy log
tail -f ~/.config/antigravity/captured/proxy.log
```

### 401 errors after capture

Token expired. Re-run capture script.

## Alternative: Direct OAuth + Header Spoofing

If you know the exact headers Antigravity sends, you could:

1. Get OAuth token via device code flow
2. Manually add the IDE headers

```bash
# But you need to know the exact headers...
export ANTIGRAVITY_EXTRA_HEADERS='{"X-Goog-Api-Client": "antigravity/1.0.0"}'
```

The proxy capture approach is more reliable because it captures the exact headers the IDE uses.
