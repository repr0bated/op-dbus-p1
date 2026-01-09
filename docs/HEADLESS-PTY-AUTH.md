# Headless PTY Authentication Bridge

## The Problem

Many CLI tools have interactive authentication flows:
- **Gemini CLI**: Opens browser for Google OAuth
- **GitHub CLI**: Device code flow or browser OAuth
- **gcloud**: Browser-based OAuth
- **aws-vault**: Prompts for MFA codes

On headless servers, these flows fail because there's no browser or interactive terminal.

## Solution: PTY Wrapper with Auth Bridge

Wrap CLI tools in a PTY (pseudo-terminal) that:
1. **Detects auth prompts** and forwards them
2. **Captures auth URLs** and makes them accessible
3. **Persists sessions** in a secure store
4. **Provides a web UI** for completing auth flows remotely

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Headless Server                                     │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                     PTY Auth Bridge                                  │   │
│  │                                                                      │   │
│  │  ┌────────────┐    ┌────────────┐    ┌────────────┐                │   │
│  │  │ gemini CLI │    │ gh CLI     │    │ gcloud     │                │   │
│  │  │ (wrapped)  │    │ (wrapped)  │    │ (wrapped)  │                │   │
│  │  └─────┬──────┘    └─────┬──────┘    └─────┬──────┘                │   │
│  │        │                 │                 │                        │   │
│  │        ▼                 ▼                 ▼                        │   │
│  │  ┌─────────────────────────────────────────────────────────────┐   │   │
│  │  │                   Auth Detector                             │   │   │
│  │  │  - Parses PTY output for auth URLs                         │   │   │
│  │  │  - Detects device codes                                    │   │   │
│  │  │  - Captures "Press Enter to continue" prompts              │   │   │
│  │  └─────────────────────────────────────────────────────────────┘   │   │
│  │                            │                                        │   │
│  │                            ▼                                        │   │
│  │  ┌─────────────────────────────────────────────────────────────┐   │   │
│  │  │                Auth Notification System                     │   │   │
│  │  │  - Web UI on port 3334 showing pending auths               │   │   │
│  │  │  - Webhook to external notification service                │   │   │
│  │  │  - Email/SMS notification option                           │   │   │
│  │  │  - D-Bus signal for local consumers                        │   │   │
│  │  └─────────────────────────────────────────────────────────────┘   │   │
│  │                            │                                        │   │
│  │                            ▼                                        │   │
│  │  ┌─────────────────────────────────────────────────────────────┐   │   │
│  │  │                Session Store (encrypted)                    │   │   │
│  │  │  - ~/.config/pty-auth-bridge/sessions/                     │   │   │
│  │  │  - Tokens encrypted at rest                                │   │   │
│  │  │  - Auto-refresh before expiry                              │   │   │
│  │  └─────────────────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  op-dbus uses wrapped CLIs transparently                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                              │
                              │ HTTPS (auth notification)
                              ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Your Phone/Workstation                              │
│                                                                             │
│  1. Receive notification: "Auth required for Gemini CLI"                   │
│  2. Click link, complete OAuth in browser                                   │
│  3. PTY bridge detects completion, continues execution                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Implementation

### Core Components

1. **PTY Wrapper** - Spawns CLI in pseudo-terminal, monitors output
2. **Auth Detector** - Pattern matching for auth prompts/URLs
3. **Notification System** - Web UI + webhooks for remote auth completion
4. **Session Store** - Encrypted credential storage with auto-refresh

### Supported Auth Patterns

| Pattern | Detection | Action |
|---------|-----------|--------|
| Browser URL | `https://...auth...` | Display in web UI |
| Device Code | `Enter code: XXXX-XXXX` | Show code + URL |
| MFA Prompt | `Enter MFA code:` | Request via webhook |
| Press Enter | `Press Enter to continue` | Auto-respond after remote confirmation |
| Password | `Password:` | Securely request via web UI |

## Alternative: Device Authorization Flow

For OAuth-capable services, use **RFC 8628 Device Authorization Grant**:

```
┌──────────────────┐                              ┌──────────────────┐
│  Headless Server │                              │   Your Phone     │
│                  │                              │                  │
│  1. Request      │                              │                  │
│     device code  │──────────────────────────────│                  │
│                  │                              │                  │
│  2. Display:     │                              │                  │
│     "Go to       │                              │                  │
│     xyz.com/code │                              │                  │
│     Enter: ABC1"│                              │  3. User visits  │
│                  │                              │     xyz.com/code │
│                  │                              │     enters ABC1  │
│  4. Poll for     │                              │                  │
│     completion   │◄─────────────────────────────│  5. User grants  │
│                  │                              │     access       │
│  6. Token        │                              │                  │
│     received!    │                              │                  │
└──────────────────┘                              └──────────────────┘
```

This is how:
- **GitHub CLI** does `gh auth login --web`
- **Azure CLI** does device code flow
- **Google Cloud** supports device code for headless

## Integration with op-dbus

### Option 1: Wrapper Binary

Create wrapper scripts that use the PTY bridge:

```bash
# /usr/local/bin/gemini-headless
#!/bin/bash
exec pty-auth-bridge gemini "$@"
```

### Option 2: LLM Provider Integration

The `op-llm` crate can use wrapped CLIs:

```rust
// In op-llm/src/gemini_cli.rs
pub struct GeminiCliProvider {
    wrapper: PtyAuthBridge,
}

impl GeminiCliProvider {
    pub async fn chat(&self, messages: Vec<Message>) -> Result<Response> {
        // Use PTY wrapper to run gemini CLI
        // Auth is handled transparently by the bridge
        self.wrapper.execute(&["gemini", "chat", "--json"], input).await
    }
}
```

### Option 3: D-Bus Service

Run PTY wrapper as a D-Bus service:

```
org.dbusmcp.AuthBridge
  ├── Methods:
  │   ├── Execute(command: s, args: as) -> (output: s)
  │   ├── GetPendingAuths() -> (auths: a{ss})
  │   ├── CompleteAuth(auth_id: s, response: s) -> (success: b)
  │   └── GetSessionStatus(tool: s) -> (authenticated: b, expires: x)
  │
  └── Signals:
      ├── AuthRequired(auth_id: s, url: s, message: s)
      └── AuthCompleted(auth_id: s, success: b)
```
