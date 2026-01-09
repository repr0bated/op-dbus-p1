#!/bin/bash
# Clean up duplicate UI implementations
# Establishes crates/op-web as the authoritative UI

set -e

echo "🧹 UI Cleanup - Establishing Authoritative UI"
echo ""

# 1. Stop and disable npm-based UI service
echo "1️⃣ Disabling op-chat-ui.service..."
if systemctl is-enabled op-chat-ui.service 2>/dev/null; then
    sudo systemctl stop op-chat-ui.service 2>/dev/null || true
    sudo systemctl disable op-chat-ui.service 2>/dev/null || true
    echo "   ✅ Disabled op-chat-ui.service"
else
    echo "   ⏭️  op-chat-ui.service not enabled"
fi

# 2. Remove the systemd unit file
if [ -f "/etc/systemd/system/op-chat-ui.service" ]; then
    sudo rm /etc/systemd/system/op-chat-ui.service
    echo "   ✅ Removed service file"
fi

sudo systemctl daemon-reload
echo ""

# 3. Find and archive duplicate UI directories
echo "2️⃣ Archiving duplicate UI directories..."
REPO=""
for p in /home/jeremy/git/op-dbus /home/jeremy/op-dbus /opt/op-dbus; do
    if [ -d "$p/crates" ]; then
        REPO="$p"
        break
    fi
done

if [ -n "$REPO" ]; then
    cd "$REPO"
    
    # Archive op-chat-ui if it exists
    if [ -d "op-chat-ui" ] && [ ! -d "archived/op-chat-ui" ]; then
        mkdir -p archived
        mv op-chat-ui archived/
        echo "   ✅ Moved op-chat-ui to archived/"
    fi
    
    # Archive chat-ui if it exists
    if [ -d "chat-ui" ] && [ ! -d "archived/chat-ui" ]; then
        mkdir -p archived
        mv chat-ui archived/
        echo "   ✅ Moved chat-ui to archived/"
    fi
else
    echo "   ⚠️ Repository not found"
fi
echo ""

# 4. Verify authoritative UI exists
echo "3️⃣ Verifying authoritative UI..."
if [ -n "$REPO" ]; then
    if [ -d "$REPO/crates/op-web/static" ]; then
        echo "   ✅ crates/op-web/static/ exists"
        ls -la "$REPO/crates/op-web/static/" | head -10
    else
        echo "   ⚠️ crates/op-web/static/ not found - UI may need setup"
    fi
fi
echo ""

# 5. Check if op-web is serving UI
echo "4️⃣ Testing op-web UI..."
if curl -s -o /dev/null -w '%{http_code}' http://localhost:8080/ | grep -q '200\|302'; then
    echo "   ✅ op-web is serving content at /"
else
    echo "   ⚠️ op-web may not be serving UI (check http://localhost:8080/)"
fi
echo ""

echo "📋 Summary"
echo "=========="
echo "Authoritative UI: crates/op-web/static/"
echo "Served by: op-web-server on port 8080"
echo "No npm/node required."
echo ""
echo "To modify UI:"
echo "  1. Edit files in crates/op-web/static/"
echo "  2. cargo build --release -p op-web"
echo "  3. sudo systemctl restart op-web"
