# Quick Fix: Get LLM Working Now

## The Problem

```
Ollama API error 404 Not Found: {"error":"model 'gemini-1.5-flash' not found"}
```

The service is:
1. Ignoring `LLM_PROVIDER=antigravity` (provider doesn't exist in code)
2. Falling back to Ollama
3. Trying to use `gemini-1.5-flash` which isn't an Ollama model

## Quick Fix Options

### Option A: Use Ollama with a Real Model (Works Now)

```bash
# SSH to your server
ssh root@op-dbus.ghostbridge.tech

# Check what models you actually have in Ollama
curl http://localhost:11434/api/tags | jq '.models[].name'

# Update environment to use a model you have
sudo sed -i 's/LLM_MODEL=.*/LLM_MODEL=llama3.2:latest/' /etc/op-dbus/environment

# Also in systemd
sudo sed -i 's/gemini-1.5-flash/llama3.2:latest/g' /etc/systemd/system/op-web.service

# Reload and restart
sudo systemctl daemon-reload
sudo systemctl restart op-web

# Test
curl -X POST https://op-dbus.ghostbridge.tech/api/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello", "user_id": "test"}'
```

### Option B: Use Gemini API Directly (Free Tier)

If you have a Gemini API key:

```bash
# Set provider to gemini (not antigravity - that doesn't exist yet)
sudo tee -a /etc/op-dbus/environment << 'EOF'
LLM_PROVIDER=gemini
GEMINI_API_KEY=your-api-key-here
LLM_MODEL=gemini-2.0-flash
EOF

# Update systemd service
sudo tee /etc/systemd/system/op-web.service.d/override.conf << 'EOF'
[Service]
Environment="LLM_PROVIDER=gemini"
Environment="GEMINI_API_KEY=your-api-key-here"
Environment="LLM_MODEL=gemini-2.0-flash"
EOF

sudo systemctl daemon-reload
sudo systemctl restart op-web
```

## Check What's Actually Configured

```bash
# See current environment
cat /etc/op-dbus/environment | grep -E "(LLM|MODEL|GEMINI|ANTIGRAVITY)"

# See systemd environment
sudo systemctl show op-web -p Environment

# See available providers/models
curl -s http://localhost:11434/api/tags | jq -r '.models[].name'

# Check service logs
sudo journalctl -u op-web -n 20 --no-pager | grep -i "llm\|model\|provider"
```

---

## Why Antigravity Doesn't Work

The `ANTIGRAVITY-INTEGRATION.md` file is a **spec** — the actual Rust code hasn't been written yet.

To implement it, your IDE Claude needs to:

1. Create `crates/op-llm/src/antigravity.rs` from the spec
2. Add `pub mod antigravity;` to `crates/op-llm/src/lib.rs`
3. Add `Antigravity` variant to `ProviderType` enum
4. Handle `"antigravity"` in the provider factory
5. Rebuild and deploy

---

## Recommended: Just Use Gemini API Key

Easiest path:

1. Get free API key from https://aistudio.google.com/
2. Set `LLM_PROVIDER=gemini` and `GEMINI_API_KEY=xxx`
3. Your existing `gemini.rs` provider should work

```bash
# One-liner fix (replace YOUR_KEY)
sudo bash -c 'cat >> /etc/op-dbus/environment << EOF
LLM_PROVIDER=gemini
GEMINI_API_KEY=YOUR_KEY_HERE
LLM_MODEL=gemini-2.0-flash
EOF'

sudo systemctl restart op-web
```
