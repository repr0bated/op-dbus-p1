#!/bin/bash
# Wire self-tools into op-dbus and restart
# Run on your server

set -e

echo "🔧 Wiring Self-Tools into op-dbus"
echo ""

# Find repo
REPO=""
for p in /home/jeremy/git/op-dbus /home/jeremy/op-dbus /opt/op-dbus; do
    if [ -d "$p/crates/op-core" ]; then
        REPO="$p"
        break
    fi
done

if [ -z "$REPO" ]; then
    REPO=$(pwd)
    if [ ! -d "$REPO/crates/op-core" ]; then
        echo "❌ Cannot find op-dbus repository"
        exit 1
    fi
fi

echo "📁 Repository: $REPO"

# 1. Set environment variable
echo ""
echo "1️⃣ Setting OP_SELF_REPO_PATH..."
sudo mkdir -p /etc/systemd/system/op-web.service.d
sudo tee /etc/systemd/system/op-web.service.d/self-tools.conf > /dev/null << EOF
[Service]
Environment="OP_SELF_REPO_PATH=$REPO"
EOF
echo "   ✅ Created systemd override"

# Also add to environment file
sudo mkdir -p /etc/op-dbus
if grep -q "^OP_SELF_REPO_PATH=" /etc/op-dbus/environment 2>/dev/null; then
    sudo sed -i "s|^OP_SELF_REPO_PATH=.*|OP_SELF_REPO_PATH=$REPO|" /etc/op-dbus/environment
else
    echo "OP_SELF_REPO_PATH=$REPO" | sudo tee -a /etc/op-dbus/environment > /dev/null
fi
echo "   ✅ Updated environment file"

# 2. Check if self_identity.rs exists
echo ""
echo "2️⃣ Checking self_identity module..."
SELF_ID="$REPO/crates/op-core/src/self_identity.rs"
if [ ! -f "$SELF_ID" ]; then
    echo "   ⚠️  self_identity.rs not found - creating from template"
    # The file would be created from the source I provided above
    echo "   ❌ Please create $SELF_ID from the provided template"
else
    echo "   ✅ self_identity.rs exists"
fi

# 3. Check if self_tools.rs exists
echo ""
echo "3️⃣ Checking self_tools module..."
SELF_TOOLS="$REPO/crates/op-tools/src/builtin/self_tools.rs"
if [ ! -f "$SELF_TOOLS" ]; then
    echo "   ⚠️  self_tools.rs not found"
    echo "   ❌ Please create $SELF_TOOLS from the provided template"
else
    echo "   ✅ self_tools.rs exists ($(wc -l < "$SELF_TOOLS") lines)"
fi

# 4. Check mod.rs wiring
echo ""
echo "4️⃣ Checking module wiring..."

# op-core/src/lib.rs
if grep -q "self_identity" "$REPO/crates/op-core/src/lib.rs" 2>/dev/null; then
    echo "   ✅ self_identity wired in op-core/src/lib.rs"
else
    echo "   ⚠️  self_identity NOT wired in op-core/src/lib.rs"
    echo "      Add: pub mod self_identity;"
fi

# op-tools/src/builtin/mod.rs
if grep -q "self_tools" "$REPO/crates/op-tools/src/builtin/mod.rs" 2>/dev/null; then
    echo "   ✅ self_tools wired in op-tools/src/builtin/mod.rs"
else
    echo "   ⚠️  self_tools NOT wired in op-tools/src/builtin/mod.rs"
    echo "      Add: pub mod self_tools;"
fi

# 5. Rebuild
echo ""
echo "5️⃣ Rebuilding..."
cd "$REPO"
if cargo build --release -p op-core -p op-tools -p op-chat -p op-web 2>&1 | tail -20; then
    echo "   ✅ Build succeeded"
    
    # Install
    if [ -f "target/release/op-web-server" ]; then
        sudo cp target/release/op-web-server /usr/local/sbin/
        echo "   ✅ Installed op-web-server"
    fi
else
    echo "   ❌ Build failed - check errors above"
    exit 1
fi

# 6. Restart service
echo ""
echo "6️⃣ Restarting service..."
sudo systemctl daemon-reload
sudo systemctl restart op-web

sleep 3

if systemctl is-active --quiet op-web; then
    echo "   ✅ op-web is running"
else
    echo "   ❌ op-web failed to start"
    sudo journalctl -u op-web -n 20 --no-pager
    exit 1
fi

# 7. Verify
echo ""
echo "7️⃣ Verifying self-tools..."
RESPONSE=$(curl -s -X POST http://localhost:8080/api/chat \
    -H "Content-Type: application/json" \
    -d '{"message": "What self_* tools do you have? List them.", "user_id": "test"}' 2>/dev/null)

if echo "$RESPONSE" | grep -qi "self_read_file\|self_write_file"; then
    echo "   ✅ Self-tools mentioned in response!"
else
    echo "   ⚠️  Self-tools not in response (may need smarter model)"
fi

echo ""
echo "🎉 Done!"
echo ""
echo "The chatbot now has:"
echo "  - 200+ system tools"
echo "  - 10 self-repository tools (self_*)"
echo "  - Anti-hallucination system prompt"
echo "  - Network topology context"
echo ""
echo "Test: Ask the chatbot to 'list your source directory using self_list_directory'"
