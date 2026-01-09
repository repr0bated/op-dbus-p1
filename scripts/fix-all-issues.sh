#!/bin/bash
# Fix all remaining issues from the journal errors

set -e

echo "🔧 Fixing All Remaining Issues"
echo ""

REPO=""
for p in /home/jeremy/git/op-dbus /home/jeremy/op-dbus /opt/op-dbus; do
    if [ -d "$p/crates" ]; then
        REPO="$p"
        break
    fi
done

if [ -z "$REPO" ]; then
    echo "❌ Cannot find op-dbus repository"
    exit 1
fi

echo "📁 Repository: $REPO"
cd "$REPO"
echo ""

# 1. Build op-agents (with the new create_agent function)
echo "1️⃣ Building op-agents..."
cargo build --release -p op-agents 2>&1 | tail -20 || {
    echo "   ❌ op-agents build failed"
    echo "   Make sure the agent source files are in place"
}
echo ""

# 2. Build op-tools
echo "2️⃣ Building op-tools..."
cargo build --release -p op-tools 2>&1 | tail -20 || {
    echo "   ❌ op-tools build failed"
}
echo ""

# 3. Build op-web
echo "3️⃣ Building op-web..."
cargo build --release -p op-web 2>&1 | tail -20 || {
    echo "   ❌ op-web build failed"
}
echo ""

# 4. Install binaries
echo "4️⃣ Installing binaries..."
if [ -f "target/release/op-web-server" ]; then
    sudo cp target/release/op-web-server /usr/local/sbin/
    echo "   ✅ Installed op-web-server"
fi

if [ -f "target/release/dbus-agent-manager" ]; then
    sudo cp target/release/dbus-agent-manager /usr/local/sbin/
    echo "   ✅ Installed dbus-agent-manager"
fi
echo ""

# 5. Clean up UI
echo "5️⃣ Cleaning up duplicate UIs..."
sudo systemctl stop op-chat-ui.service 2>/dev/null || true
sudo systemctl disable op-chat-ui.service 2>/dev/null || true
sudo rm -f /etc/systemd/system/op-chat-ui.service
sudo systemctl daemon-reload
echo "   ✅ Disabled op-chat-ui.service"
echo ""

# 6. Restart services
echo "6️⃣ Restarting services..."
sudo systemctl restart op-web.service 2>/dev/null || echo "   ⚠️ op-web restart failed"
echo "   ✅ Services restarted"
echo ""

# 7. Verify
echo "7️⃣ Verification..."
sleep 3

if systemctl is-active --quiet op-web; then
    echo "   ✅ op-web is running"
else
    echo "   ❌ op-web is NOT running"
    echo "   Check: sudo journalctl -u op-web -n 50"
fi

# Check for D-Bus agents
AGENTS=$(busctl --system list 2>/dev/null | grep -c "org.dbusmcp.Agent" || echo "0")
echo "   📊 D-Bus agents registered: $AGENTS"
echo ""

echo "✅ Done!"
echo ""
echo "Next steps if issues remain:"
echo "  1. Check logs: sudo journalctl -u op-web -f"
echo "  2. Test API: curl http://localhost:8080/api/health"
echo "  3. Test D-Bus: busctl --system list | grep dbusmcp"
