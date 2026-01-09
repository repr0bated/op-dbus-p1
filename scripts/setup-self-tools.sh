#!/bin/bash
# Setup self-repository tools
# Run on your server

set -e

echo "🔧 Setting up self-repository tools"

# Find the actual repo path
REPO_PATH=""
for path in "/home/jeremy/git/op-dbus" "/home/jeremy/op-dbus" "/opt/op-dbus" "$(pwd)"; do
    if [ -d "$path/crates/op-core" ]; then
        REPO_PATH="$path"
        break
    fi
done

if [ -z "$REPO_PATH" ]; then
    echo "❌ Could not find op-dbus repository"
    echo "   Looked in: /home/jeremy/git/op-dbus, /home/jeremy/op-dbus, /opt/op-dbus, $(pwd)"
    exit 1
fi

echo "✅ Found repository at: $REPO_PATH"

# Add to environment file
ENV_FILE="/etc/op-dbus/environment"
if [ -f "$ENV_FILE" ]; then
    if grep -q "OP_SELF_REPO_PATH" "$ENV_FILE"; then
        sudo sed -i "s|OP_SELF_REPO_PATH=.*|OP_SELF_REPO_PATH=$REPO_PATH|" "$ENV_FILE"
        echo "✅ Updated OP_SELF_REPO_PATH in $ENV_FILE"
    else
        echo "OP_SELF_REPO_PATH=$REPO_PATH" | sudo tee -a "$ENV_FILE" > /dev/null
        echo "✅ Added OP_SELF_REPO_PATH to $ENV_FILE"
    fi
else
    sudo mkdir -p /etc/op-dbus
    echo "OP_SELF_REPO_PATH=$REPO_PATH" | sudo tee "$ENV_FILE" > /dev/null
    echo "✅ Created $ENV_FILE with OP_SELF_REPO_PATH"
fi

# Add to systemd service override
OVERRIDE_DIR="/etc/systemd/system/op-web.service.d"
sudo mkdir -p "$OVERRIDE_DIR"

cat << EOF | sudo tee "$OVERRIDE_DIR/self-repo.conf" > /dev/null
[Service]
Environment="OP_SELF_REPO_PATH=$REPO_PATH"
EOF

echo "✅ Created systemd override at $OVERRIDE_DIR/self-repo.conf"

# Reload systemd
sudo systemctl daemon-reload
echo "✅ Reloaded systemd"

# Restart service
echo "🔄 Restarting op-web..."
sudo systemctl restart op-web

sleep 3

# Verify
if systemctl is-active --quiet op-web; then
    echo "✅ op-web is running"
else
    echo "❌ op-web failed to start"
    sudo journalctl -u op-web -n 20 --no-pager
    exit 1
fi

# Check if self tools are registered
echo ""
echo "🧪 Testing self tools..."
RESPONSE=$(curl -s -X POST http://localhost:8080/api/chat \
    -H "Content-Type: application/json" \
    -d '{"message": "List your own source code directory using self_list_directory", "user_id": "test"}' 2>/dev/null)

if echo "$RESPONSE" | grep -q "self_list_directory\|entries\|crates"; then
    echo "✅ Self tools appear to be working!"
else
    echo "⚠️  Self tools may not be registered yet"
    echo "   Response: $RESPONSE"
fi

echo ""
echo "🎉 Done! Self-repository path: $REPO_PATH"
echo ""
echo "The chatbot can now:"
echo "  - Read its own source code (self_read_file)"
echo "  - Modify its own code (self_write_file)"
echo "  - Search its codebase (self_search_code)"
echo "  - Commit changes (self_git_commit)"
echo "  - Build itself (self_build)"
echo "  - Deploy itself (self_deploy)"
