# Antigravity Bridge - IDE-Based LLM Access

## ⚠️ CRITICAL: NO API KEYS, NO VERTEX AI

> **THIS IS NOT ABOUT API KEYS.**
>
> **THIS IS NOT ABOUT VERTEX AI.**
>
> **THIS IS NOT ABOUT GOOGLE CLOUD CREDENTIALS.**

## What Antigravity Actually Does

**Antigravity uses your IDE's Google login** to access LLM capabilities:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         HOW ANTIGRAVITY WORKS                               │
│                                                                             │
│  1. You open Cursor/Windsurf/VSCode                                        │
│  2. IDE prompts: "Sign in with Google"                                     │
│  3. You authenticate with your Google account (OAuth)                      │
│  4. IDE now has authenticated session to Google's LLM services             │
│  5. Antigravity Bridge exposes this session via HTTP on localhost          │
│  6. op-dbus connects to the bridge → uses YOUR authenticated session       │
│                                                                             │
│  NO API KEYS INVOLVED. The IDE handles everything.                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

## What NOT To Do

❌ **DO NOT** set `GEMINI_API_KEY`
❌ **DO NOT** set `ANTHROPIC_API_KEY`  
❌ **DO NOT** set `OPENAI_API_KEY`
❌ **DO NOT** configure `GOOGLE_APPLICATION_CREDENTIALS`
❌ **DO NOT** run `gcloud auth application-default login`
❌ **DO NOT** enable Vertex AI in Google Cloud Console
❌ **DO NOT** set up a GCP billing account for this

**If you're configuring API keys, you're doing it wrong.** The whole point is to use your IDE's existing authenticated session.

## What TO Do

✅ **DO** sign into Google in your IDE (Cursor, Windsurf, VSCode + Copilot)
✅ **DO** install the Antigravity Bridge extension
✅ **DO** let the extension expose localhost:3333
✅ **DO** set `ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333` in op-dbus
✅ **DO** use SSH tunnels for headless servers

---

## Architecture: Desktop (IDE Running)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Your Desktop with IDE                                    │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │                    Cursor / Windsurf / VSCode                         │ │
│  │                                                                       │ │
│  │  ┌─────────────────────────────────────────────────────────┐         │ │
│  │  │  Google OAuth Session                                    │         │ │
│  │  │  (You logged in with your Google account)               │         │ │
│  │  │                                                         │         │ │
│  │  │  This gives the IDE access to:                          │         │ │
│  │  │  - Gemini models (via your subscription)                │         │ │
│  │  │  - Claude models (if Cursor Pro)                        │         │ │
│  │  │  - GPT models (if Copilot subscription)                 │         │ │
│  │  └─────────────────────────────────────────────────────────┘         │ │
│  │                              │                                        │ │
│  │                              │ Internal IDE API (vscode.lm)           │ │
│  │                              ▼                                        │ │
│  │  ┌─────────────────────────────────────────────────────────┐         │ │
│  │  │  Antigravity Bridge Extension                           │         │ │
│  │  │                                                         │         │ │
│  │  │  - HTTP server on localhost:3333                       │         │ │
│  │  │  - Proxies requests through IDE's authenticated session │         │ │
│  │  │  - NO API KEYS - uses Google OAuth session             │         │ │
│  │  └─────────────────────────────────────────────────────────┘         │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                              │                                              │
│                              │ HTTP localhost:3333                          │
│                              ▼                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │                    op-dbus (local)                                    │ │
│  │                                                                       │ │
│  │  ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333                        │ │
│  │  LLM_PROVIDER=antigravity                                             │ │
│  │                                                                       │ │
│  │  NO API KEYS CONFIGURED                                               │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Headless Server Setup

Headless servers have no GUI, so they can't run the IDE. You need to **tunnel** from the headless server to a machine that's running the IDE.

### Architecture: Headless Server

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Your Workstation (has GUI)                               │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  Cursor with Google Login + Antigravity Bridge                       │   │
│  │  Listening: localhost:3333                                           │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                              │                                              │
│                              │ localhost:3333                               │
│                              ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  SSH Tunnel Endpoint (reverse or local)                              │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                              │
                              │ SSH Tunnel (encrypted)
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Headless Server (no GUI)                                 │
│                                                                             │
│  SSH tunnel makes workstation:3333 available as localhost:3333              │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  op-dbus                                                            │   │
│  │                                                                     │   │
│  │  ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333                      │   │
│  │  LLM_PROVIDER=antigravity                                           │   │
│  │                                                                     │   │
│  │  Thinks it's connecting locally, actually tunneled to workstation  │   │
│  │  NO API KEYS CONFIGURED                                            │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Option 1: SSH Reverse Tunnel (from workstation)

Run this **on your workstation** (where IDE runs):

```bash
# Makes localhost:3333 on headless server point to your workstation's localhost:3333
ssh -R 3333:127.0.0.1:3333 user@headless-server
```

Now on the headless server, `localhost:3333` reaches your IDE's bridge.

### Option 2: SSH Local Tunnel (from server)

Run this **on the headless server**:

```bash
# Makes local port 3333 connect to workstation's localhost:3333
ssh -L 3333:127.0.0.1:3333 user@your-workstation
```

### Option 3: Persistent Tunnel (systemd)

On the **headless server**, create `/etc/systemd/system/antigravity-tunnel.service`:

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
# Setup SSH key for op-dbus user
sudo -u op-dbus ssh-keygen -t ed25519 -N '' -f /var/lib/op-dbus/.ssh/id_ed25519

# Add to workstation's authorized_keys
cat /var/lib/op-dbus/.ssh/id_ed25519.pub >> ~/.ssh/authorized_keys  # on workstation

# Enable tunnel
sudo systemctl enable --now antigravity-tunnel
```

### Option 4: Netmaker Mesh (if you have it)

If using your Netmaker mesh (nm0 interface):

```bash
# Get workstation mesh IP
ip addr show nm0  # e.g., 10.50.0.2

# On headless server, configure op-dbus
ANTIGRAVITY_BRIDGE_URL=http://10.50.0.2:3333
```

No tunnel needed - mesh handles routing.

---

## Headless Server Configuration

On the headless server, edit `/etc/op-dbus/environment`:

```bash
# Use Antigravity (IDE bridge)
LLM_PROVIDER=antigravity

# Bridge URL (tunneled to workstation)
ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333

# Model to use (via IDE subscription)
LLM_MODEL=claude-3-5-sonnet

# ════════════════════════════════════════════════════════
# DO NOT SET ANY OF THESE - THEY ARE NOT USED
# ════════════════════════════════════════════════════════
# GEMINI_API_KEY=xxx           # NO!
# ANTHROPIC_API_KEY=xxx        # NO!
# OPENAI_API_KEY=xxx           # NO!
# GOOGLE_APPLICATION_CREDENTIALS=xxx  # NO!
# GOOGLE_GENAI_USE_VERTEXAI=true      # NO!
```

---

## Extension Implementation

The extension is simple - it just proxies HTTP requests through the IDE's internal LLM API.

### package.json

```json
{
  "name": "antigravity-bridge",
  "displayName": "Antigravity Bridge",
  "description": "Bridge IDE Google login to op-dbus (NO API KEYS)",
  "version": "1.0.0",
  "engines": { "vscode": "^1.85.0" },
  "activationEvents": ["onStartupFinished"],
  "main": "./out/extension.js",
  "contributes": {
    "commands": [
      { "command": "antigravity.start", "title": "Antigravity: Start Bridge" },
      { "command": "antigravity.stop", "title": "Antigravity: Stop Bridge" },
      { "command": "antigravity.status", "title": "Antigravity: Show Status" }
    ],
    "configuration": {
      "properties": {
        "antigravity.port": {
          "type": "number",
          "default": 3333,
          "description": "Bridge HTTP port"
        },
        "antigravity.autoStart": {
          "type": "boolean",
          "default": true
        }
      }
    }
  }
}
```

### Core Extension Logic

```typescript
import * as vscode from 'vscode';
import * as http from 'http';

let server: http.Server | null = null;

export function activate(context: vscode.ExtensionContext) {
    // Auto-start bridge
    startBridge();
    
    context.subscriptions.push(
        vscode.commands.registerCommand('antigravity.start', startBridge),
        vscode.commands.registerCommand('antigravity.stop', stopBridge),
        vscode.commands.registerCommand('antigravity.status', showStatus)
    );
}

function startBridge() {
    if (server) return;
    
    const port = vscode.workspace.getConfiguration('antigravity').get('port', 3333);
    
    server = http.createServer(async (req, res) => {
        res.setHeader('Access-Control-Allow-Origin', '*');
        res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
        res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
        
        if (req.method === 'OPTIONS') {
            res.writeHead(204);
            res.end();
            return;
        }
        
        // Health check
        if (req.method === 'GET' && req.url === '/health') {
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                status: 'ok',
                auth: 'Google OAuth via IDE (NO API KEYS)',
                message: 'Using IDE Google login session'
            }));
            return;
        }
        
        // Chat completions
        if (req.method === 'POST' && req.url === '/v1/chat/completions') {
            try {
                const body = await readBody(req);
                const request = JSON.parse(body);
                const response = await chat(request);
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify(response));
            } catch (e) {
                res.writeHead(500, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: String(e) }));
            }
            return;
        }
        
        res.writeHead(404);
        res.end();
    });
    
    server.listen(port, '127.0.0.1', () => {
        vscode.window.showInformationMessage(
            `Antigravity Bridge on localhost:${port} (using Google login)`
        );
    });
}

async function chat(request: any): Promise<any> {
    // This uses the IDE's internal LLM API
    // The IDE is logged into Google - no API keys needed
    
    // Try vscode.lm API (VSCode 1.90+ with Copilot)
    if (vscode.lm) {
        const models = await vscode.lm.selectChatModels({});
        if (models.length > 0) {
            const model = models[0];
            const messages = request.messages.map((m: any) =>
                m.role === 'user'
                    ? vscode.LanguageModelChatMessage.User(m.content)
                    : vscode.LanguageModelChatMessage.Assistant(m.content)
            );
            
            const response = await model.sendRequest(
                messages,
                {},
                new vscode.CancellationTokenSource().token
            );
            
            let content = '';
            for await (const chunk of response.text) {
                content += chunk;
            }
            
            return {
                id: `bridge-${Date.now()}`,
                object: 'chat.completion',
                model: model.name,
                choices: [{
                    index: 0,
                    message: { role: 'assistant', content },
                    finish_reason: 'stop'
                }]
            };
        }
    }
    
    // Try Cursor's internal API
    try {
        const result = await vscode.commands.executeCommand(
            'cursor.chat.sendMessage',
            request.messages?.[request.messages.length - 1]?.content || ''
        );
        if (result) {
            return {
                id: `cursor-${Date.now()}`,
                object: 'chat.completion',
                model: 'cursor',
                choices: [{
                    index: 0,
                    message: { role: 'assistant', content: String(result) },
                    finish_reason: 'stop'
                }]
            };
        }
    } catch {}
    
    throw new Error(
        'No LLM available. Make sure you are logged into Google in your IDE. ' +
        'This extension uses your IDE Google login - NO API KEYS.'
    );
}

function stopBridge() {
    if (server) {
        server.close();
        server = null;
        vscode.window.showInformationMessage('Antigravity Bridge stopped');
    }
}

function showStatus() {
    vscode.window.showInformationMessage(
        `Antigravity Bridge: ${server ? 'Running' : 'Stopped'}\n` +
        `Auth: Google OAuth via IDE (NO API KEYS)`
    );
}

function readBody(req: http.IncomingMessage): Promise<string> {
    return new Promise((resolve, reject) => {
        let body = '';
        req.on('data', chunk => body += chunk);
        req.on('end', () => resolve(body));
        req.on('error', reject);
    });
}

export function deactivate() {
    stopBridge();
}
```

---

## Testing

### On Workstation (IDE running)

```bash
# Health check
curl http://localhost:3333/health
# Expected: {"status":"ok","auth":"Google OAuth via IDE (NO API KEYS)"}

# Test chat
curl -X POST http://localhost:3333/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}]}'
```

### On Headless Server (via tunnel)

```bash
# Verify tunnel
curl http://localhost:3333/health

# Test op-dbus
curl -X POST http://localhost:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"Hello","user_id":"test"}'
```

---

## Troubleshooting

### "No LLM available"

1. **Check IDE Google login**: Settings → Account → Ensure signed in with Google
2. **Check subscription**: Cursor Pro, Windsurf Pro, or Copilot active?
3. **Restart IDE**: Sometimes session needs refresh

**DO NOT add API keys** - that's not the solution.

### Tunnel not working

```bash
# On headless server
ss -tlnp | grep 3333
curl -v http://127.0.0.1:3333/health

# Check systemd tunnel
systemctl status antigravity-tunnel
journalctl -u antigravity-tunnel -f
```

### "Connection refused" on headless

1. Is IDE running on workstation?
2. Is Antigravity extension started?
3. Is SSH tunnel established?

```bash
# From headless server, test tunnel
ncurl -w '%{http_code}' -o /dev/null http://127.0.0.1:3333/health
```

---

## Security

| Concern | Mitigation |
|---------|------------|
| **Token exposure** | Bridge only on localhost (127.0.0.1) |
| **Network exposure** | SSH tunnel encrypts all traffic |
| **Credential storage** | None - uses IDE session |
| **Binding to 0.0.0.0** | **NEVER** - always use tunnels |

---

## Summary

| What | How |
|------|-----|
| **Authentication** | Google OAuth in IDE (you sign in) |
| **API Keys** | **NONE** - not used |
| **Vertex AI** | **NONE** - not used |
| **Bridge** | Extension exposes IDE session on localhost:3333 |
| **Headless** | SSH tunnel from server to workstation |
| **op-dbus config** | `ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333` |
