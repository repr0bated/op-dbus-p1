#!/bin/bash
# Setup Antigravity on a headless server
#
# This creates a persistent SSH tunnel to a workstation running the IDE.
# The IDE must have the Antigravity Bridge extension and be logged into Google.
#
# NO API KEYS NEEDED - uses IDE's Google OAuth session.

set -e

echo "🔐 Antigravity Headless Setup"
echo ""
echo "⚠️  This uses your IDE's Google login - NO API KEYS"
echo ""

# Get workstation address
read -p "Workstation address (where IDE runs): " WORKSTATION
if [ -z "$WORKSTATION" ]; then
    echo "❌ Workstation address required"
    exit 1
fi

# Get SSH user
read -p "SSH user on workstation [$(whoami)]: " SSH_USER
SSH_USER=${SSH_USER:-$(whoami)}

# Test SSH connectivity
echo ""
echo "🔍 Testing SSH connection..."
if ! ssh -o ConnectTimeout=5 "${SSH_USER}@${WORKSTATION}" echo "SSH OK" 2>/dev/null; then
    echo "❌ Cannot connect to ${SSH_USER}@${WORKSTATION}"
    echo "   Make sure SSH key authentication is set up"
    exit 1
fi
echo "✅ SSH connection works"

# Test if bridge is running on workstation
echo ""
echo "🔍 Checking Antigravity Bridge on workstation..."
if ssh "${SSH_USER}@${WORKSTATION}" "curl -s http://127.0.0.1:3333/health" 2>/dev/null | grep -q 'ok'; then
    echo "✅ Antigravity Bridge is running"
else
    echo "⚠️  Antigravity Bridge not detected on workstation"
    echo "   Make sure:"
    echo "   1. IDE (Cursor/VSCode) is running"
    echo "   2. Antigravity Bridge extension is installed and started"
    echo "   3. You are logged into Google in the IDE"
    echo ""
    read -p "Continue anyway? [y/N]: " CONTINUE
    if [ "$CONTINUE" != "y" ] && [ "$CONTINUE" != "Y" ]; then
        exit 1
    fi
fi

# Create systemd service for tunnel
echo ""
echo "📝 Creating systemd tunnel service..."

# Generate SSH key if needed
SSH_KEY_PATH="/var/lib/op-dbus/.ssh/id_ed25519"
if [ ! -f "$SSH_KEY_PATH" ]; then
    echo "   Generating SSH key for op-dbus user..."
    sudo mkdir -p /var/lib/op-dbus/.ssh
    sudo ssh-keygen -t ed25519 -N '' -f "$SSH_KEY_PATH" -C "op-dbus@$(hostname)"
    sudo chown -R op-dbus:op-dbus /var/lib/op-dbus/.ssh
    sudo chmod 700 /var/lib/op-dbus/.ssh
    sudo chmod 600 "$SSH_KEY_PATH"
    
    echo ""
    echo "   ⚠️  Add this key to ${WORKSTATION}:~/.ssh/authorized_keys:"
    echo ""
    sudo cat "${SSH_KEY_PATH}.pub"
    echo ""
    read -p "   Press Enter when done..."
fi

# Create service file
sudo tee /etc/systemd/system/antigravity-tunnel.service > /dev/null << EOF
[Unit]
Description=Antigravity Bridge SSH Tunnel (NO API KEYS - uses IDE Google login)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=op-dbus
ExecStart=/usr/bin/ssh -N -L 3333:127.0.0.1:3333 \\
    -o ServerAliveInterval=30 \\
    -o ServerAliveCountMax=3 \\
    -o ExitOnForwardFailure=yes \\
    -o StrictHostKeyChecking=accept-new \\
    -i ${SSH_KEY_PATH} \\
    ${SSH_USER}@${WORKSTATION}
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# Configure op-dbus
echo ""
echo "📝 Configuring op-dbus for Antigravity..."

if [ -f /etc/op-dbus/environment ]; then
    # Update or add settings
    sudo grep -q "^LLM_PROVIDER=" /etc/op-dbus/environment && \
        sudo sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=antigravity/' /etc/op-dbus/environment || \
        echo "LLM_PROVIDER=antigravity" | sudo tee -a /etc/op-dbus/environment > /dev/null
    
    sudo grep -q "^ANTIGRAVITY_BRIDGE_URL=" /etc/op-dbus/environment && \
        sudo sed -i 's|^ANTIGRAVITY_BRIDGE_URL=.*|ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333|' /etc/op-dbus/environment || \
        echo "ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333" | sudo tee -a /etc/op-dbus/environment > /dev/null
    
    # Remove any API key settings (not needed)
    sudo sed -i '/^GEMINI_API_KEY=/d' /etc/op-dbus/environment
    sudo sed -i '/^ANTHROPIC_API_KEY=/d' /etc/op-dbus/environment
    sudo sed -i '/^OPENAI_API_KEY=/d' /etc/op-dbus/environment
    sudo sed -i '/^GOOGLE_APPLICATION_CREDENTIALS=/d' /etc/op-dbus/environment
    
    echo "   ✅ op-dbus configured"
else
    echo "   ⚠️  /etc/op-dbus/environment not found"
    echo "   Add manually:"
    echo "   LLM_PROVIDER=antigravity"
    echo "   ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:3333"
fi

# Enable and start services
echo ""
echo "🚀 Starting services..."
sudo systemctl daemon-reload
sudo systemctl enable antigravity-tunnel
sudo systemctl start antigravity-tunnel

sleep 3

# Verify
echo ""
echo "🔍 Verifying setup..."

if systemctl is-active --quiet antigravity-tunnel; then
    echo "   ✅ Tunnel service running"
else
    echo "   ❌ Tunnel service failed"
    sudo journalctl -u antigravity-tunnel -n 5 --no-pager
fi

if curl -s http://127.0.0.1:3333/health 2>/dev/null | grep -q 'ok'; then
    echo "   ✅ Bridge accessible via tunnel"
else
    echo "   ⚠️  Bridge not reachable (is IDE running on workstation?)"
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "📋 Summary:"
echo "   Tunnel: localhost:3333 → ${WORKSTATION}:3333"
echo "   Auth: Google OAuth via IDE (NO API KEYS)"
echo "   Provider: antigravity"
echo ""
echo "📌 Requirements:"
echo "   - IDE (Cursor/Windsurf/VSCode) running on ${WORKSTATION}"
echo "   - Antigravity Bridge extension started"
echo "   - Logged into Google in IDE"
echo ""
echo "🔧 Restart op-dbus to apply:"
echo "   sudo systemctl restart op-web"
