# Antigravity: NO API Keys Required

## The Core Concept

**Antigravity leverages your IDE's Google login** - not API keys, not Vertex AI, not cloud credentials.

```
┌──────────────────────────────────────────────────────────────────┐
│                    HOW ANTIGRAVITY WORKS                         │
│                                                                  │
│  You (in IDE)                                                    │
│       │                                                          │
│       │ "Sign in with Google"                                    │
│       ▼                                                          │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │               Google OAuth                               │    │
│  │                                                         │    │
│  │  ● You authenticate with your Google account            │    │
│  │  ● IDE receives OAuth tokens                            │    │
│  │  ● These tokens grant access to Gemini/Claude/GPT       │    │
│  │  ● (Based on your subscription: Cursor Pro, etc.)       │    │
│  └─────────────────────────────────────────────────────────┘    │
│       │                                                          │
│       │ IDE now has authenticated session                        │
│       ▼                                                          │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │          Antigravity Bridge Extension                    │    │
│  │                                                         │    │
│  │  ● Exposes HTTP server on localhost:3333                │    │
│  │  ● Routes LLM requests through IDE's session            │    │
│  │  ● NO API KEYS in the extension                         │    │
│  │  ● NO credentials stored                                │    │
│  └─────────────────────────────────────────────────────────┘    │
│       │                                                          │
│       │ HTTP localhost:3333                                      │
│       ▼                                                          │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │               op-dbus                                    │    │
│  │                                                         │    │
│  │  ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333          │    │
│  │  LLM_PROVIDER=antigravity                               │    │
│  │                                                         │    │
│  │  NO API KEYS CONFIGURED HERE EITHER                     │    │
│  └─────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

## What You Need

1. **IDE with Google login**: Cursor, Windsurf, or VSCode with Copilot
2. **Active subscription**: Your IDE subscription gives you LLM access
3. **Antigravity Bridge extension**: Installed in IDE
4. **op-dbus configured**: `LLM_PROVIDER=antigravity`

## What You DON'T Need

| NOT Needed | Why |
|------------|-----|
| `GEMINI_API_KEY` | Uses IDE's Google OAuth instead |
| `ANTHROPIC_API_KEY` | Uses IDE's subscription instead |
| `OPENAI_API_KEY` | Uses IDE's subscription instead |
| `GOOGLE_APPLICATION_CREDENTIALS` | Not using service accounts |
| Vertex AI enabled | Not using Vertex AI |
| GCP billing account | Not using GCP services |
| `gcloud auth` anything | Not using gcloud |

## Headless Servers

Headless servers can't run the IDE, so you tunnel to a machine that can:

```bash
# From workstation (where IDE runs)
ssh -R 3333:127.0.0.1:3333 user@headless-server

# Now headless server's localhost:3333 reaches your IDE
```

Or use Netmaker mesh if available:

```bash
# On headless server
ANTIGRAVITY_BRIDGE_URL=http://10.50.0.2:3333  # Workstation's mesh IP
```

## Quick Start

### Desktop (IDE running locally)

1. Install Antigravity Bridge extension
2. Sign into Google in IDE
3. Configure op-dbus:

```bash
LLM_PROVIDER=antigravity
ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333
```

### Headless Server

1. Same as above, but add SSH tunnel:

```bash
# On workstation
ssh -R 3333:127.0.0.1:3333 user@server
```

2. Or make it persistent with systemd on server

## FAQ

**Q: Do I need to get a Gemini API key?**
A: NO. The IDE's Google login handles authentication.

**Q: What about Vertex AI?**
A: NOT USED. Antigravity uses the standard consumer Gemini API via IDE OAuth.

**Q: How does the IDE have access to Gemini?**
A: When you sign into Google in your IDE, you authorize it to use Google's AI services on your behalf. Your subscription (Cursor Pro, etc.) determines quotas.

**Q: What if I don't have an IDE subscription?**
A: You can still sign into Google - basic Gemini access is available. For better models/quotas, you need a subscription.

**Q: Can I use this on a server without GUI?**
A: Yes, via SSH tunnel to a workstation running the IDE.
