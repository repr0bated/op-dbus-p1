#!/bin/bash
# Start D-Bus agents properly
#
# This registers agents as D-Bus services so introspection can find them.

set -e

echo "🚀 Starting D-Bus Agents"
echo ""

# 1. Install D-Bus policy if not present
DBUS_CONF="/etc/dbus-1/system.d/op-dbus.conf"
if [ ! -f "$DBUS_CONF" ]; then
    echo "📝 Installing D-Bus policy..."
    sudo cp deploy/dbus/org.dbusmcp.conf "$DBUS_CONF"
    sudo systemctl reload dbus
    echo "   ✅ D-Bus policy installed"
fi

# 2. Build the agent manager if needed
if [ ! -f "target/release/dbus-agent-manager" ]; then
    echo "📦 Building agent manager..."
    cargo build --release -p op-agents --bin dbus-agent-manager
fi

# 3. Check if already running
if systemctl is-active --quiet op-dbus-agents 2>/dev/null; then
    echo "⚠️  Agent service already running"
    echo "   Stop with: sudo systemctl stop op-dbus-agents"
    exit 0
fi

# 4. Start agents (foreground for debugging, or install service)
if [ "$1" = "--service" ]; then
    echo "📋 Installing systemd service..."
    sudo cp deploy/systemd/op-dbus-agents.service /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable op-dbus-agents
    sudo systemctl start op-dbus-agents
    echo "   ✅ Service installed and started"
    echo ""
    echo "Check status: sudo systemctl status op-dbus-agents"
    echo "View logs:    sudo journalctl -u op-dbus-agents -f"
else
    echo "🔧 Starting agents in foreground..."
    echo "   (Use --service to install as systemd service)"
    echo ""
    sudo ./target/release/dbus-agent-manager
fi
