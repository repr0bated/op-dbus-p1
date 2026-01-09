# UI Cleanup - Authoritative Source

## The Problem

Multiple UI implementations exist:
1. `op-chat-ui/` - Requires npm, unclear if used
2. `chat-ui/` - May not exist
3. `crates/op-web/src/ui/` - Rust-based, embedded in op-web
4. `crates/op-web/static/` - Static files served by op-web

## Authoritative UI: `crates/op-web`

The **authoritative UI** is the one embedded in `op-web-server`:
- Located at `crates/op-web/src/ui/` (Rust templates)
- Static files at `crates/op-web/static/`
- Served directly by the Axum web server
- No npm/node required

## What to Remove/Disable

### 1. Disable op-chat-ui.service

```bash
sudo systemctl disable op-chat-ui.service
sudo systemctl stop op-chat-ui.service
sudo rm /etc/systemd/system/op-chat-ui.service
```

### 2. Remove or Archive Unused UI Directories

```bash
# Archive for reference
mv op-chat-ui/ archived-op-chat-ui/
mv chat-ui/ archived-chat-ui/ 2>/dev/null || true
```

### 3. Clean Up Systemd

```bash
sudo systemctl daemon-reload
sudo systemctl reset-failed
```

## UI Architecture

```
op-web-server (port 8080)
    │
    ├── /                   → Index page (from op-web/static/index.html)
    ├── /chat               → Chat interface (from op-web/static/chat.html)
    ├── /admin              → Admin interface (from op-web/src/ui/admin.rs)
    ├── /api/*              → REST API endpoints
    ├── /mcp/*              → MCP protocol endpoints
    └── /static/*           → Static assets
```

## Files in op-web

```
crates/op-web/
├── src/
│   ├── main.rs             # Server entry point
│   ├── routes/             # API routes
│   ├── ui/                 # UI handlers (admin, etc.)
│   └── handlers/           # Request handlers
├── static/
│   ├── index.html          # Main page
│   ├── chat.html           # Chat interface
│   ├── css/
│   └── js/
└── templates/              # HTML templates (if using templating)
```

## Why Not npm-based UI?

1. **Simplicity** - No build step required
2. **Deployment** - Single binary includes everything
3. **Security** - Fewer dependencies
4. **Reliability** - No npm/node version issues

## Adding New UI Features

Edit files in `crates/op-web/static/` and rebuild:

```bash
cargo build --release -p op-web
sudo cp target/release/op-web-server /usr/local/sbin/
sudo systemctl restart op-web
```
