#!/bin/bash
# Fix ALL system prompt and self-tools issues
# Run as root on your server

set -e

echo "🔧 Fixing System Prompt and Self-Tools Issues"
echo ""

# 1. Find the actual repo path
REPO_PATH=""
for path in "/home/jeremy/git/op-dbus" "/home/jeremy/op-dbus" "/opt/op-dbus" "$(pwd)"; do
    if [ -d "$path/crates/op-core" ]; then
        REPO_PATH="$path"
        break
    fi
done

if [ -z "$REPO_PATH" ]; then
    echo "❌ Could not find op-dbus repository"
    echo "   Please set REPO_PATH manually"
    exit 1
fi

echo "📁 Repository: $REPO_PATH"
echo ""

# 2. Set OP_SELF_REPO_PATH in systemd
echo "📝 Setting OP_SELF_REPO_PATH in systemd..."
mkdir -p /etc/systemd/system/op-web.service.d
cat > /etc/systemd/system/op-web.service.d/self-repo.conf << EOF
[Service]
Environment="OP_SELF_REPO_PATH=$REPO_PATH"
EOF
echo "   ✅ Created systemd override"

# 3. Add to environment file
echo "📝 Updating /etc/op-dbus/environment..."
mkdir -p /etc/op-dbus
if grep -q "^OP_SELF_REPO_PATH=" /etc/op-dbus/environment 2>/dev/null; then
    sed -i "s|^OP_SELF_REPO_PATH=.*|OP_SELF_REPO_PATH=$REPO_PATH|" /etc/op-dbus/environment
else
    echo "OP_SELF_REPO_PATH=$REPO_PATH" >> /etc/op-dbus/environment
fi
echo "   ✅ Updated environment file"

# 4. Check if self_tools module is wired in mod.rs
MOD_RS="$REPO_PATH/crates/op-tools/src/builtin/mod.rs"
if [ -f "$MOD_RS" ]; then
    if ! grep -q "pub mod self_tools" "$MOD_RS"; then
        echo "⚠️  self_tools module NOT declared in mod.rs"
        echo "   Adding it now..."
        # Add module declaration after last 'pub mod' line
        sed -i '/^pub mod /a pub mod self_tools;' "$MOD_RS" 2>/dev/null || \
            echo 'pub mod self_tools;' >> "$MOD_RS"
        echo "   ✅ Added 'pub mod self_tools;'"
    else
        echo "   ✅ self_tools module already declared"
    fi
else
    echo "⚠️  Could not find $MOD_RS"
fi

# 5. Create/update LLM-SYSTEM-PROMPT-COMPLETE.txt from system_prompt.rs
echo "📝 Checking system prompt file..."
SYSTEM_PROMPT_FILE="$REPO_PATH/LLM-SYSTEM-PROMPT-COMPLETE.txt"
if [ ! -f "$SYSTEM_PROMPT_FILE" ]; then
    echo "   Creating system prompt file from template..."
    # Extract BASE_SYSTEM_PROMPT from source
    if [ -f "$REPO_PATH/crates/op-chat/src/system_prompt.rs" ]; then
        # This is a simplified extraction - the full prompt is huge
        cat > "$SYSTEM_PROMPT_FILE" << 'PROMPT'
You are an expert system administration assistant with FULL ACCESS to:
- Linux system administration via native protocols
- D-Bus and systemd control
- OVS (Open vSwitch) management
- Network configuration via rtnetlink
- Container orchestration
- Your own source code (self-modification)

## CRITICAL: NO HALLUCINATIONS
- ALWAYS use tools for actions
- NEVER claim to have done something without calling the tool
- NEVER suggest CLI commands (ovs-vsctl, systemctl, ip, etc.)
- Use native protocol tools exclusively

## Self-Repository Tools
If OP_SELF_REPO_PATH is set, you have access to:
- self_read_file - Read your source files
- self_write_file - Modify your source files
- self_list_directory - Explore your codebase
- self_search_code - Search your code
- self_git_status - Check git status
- self_git_diff - View pending changes
- self_git_commit - Commit changes
- self_build - Build yourself
- self_deploy - Deploy yourself

## Available Tool Categories
- OVS: ovs_list_bridges, ovs_create_bridge, ovs_delete_bridge, ovs_add_port, etc.
- Systemd: dbus_systemd_*, systemd_status
- Network: list_network_interfaces, etc.
- File: read_file, write_file, file_list
- Shell: shell_execute (only when no native tool exists)

## Network Topology Target
When asked to configure networking, target this architecture:
- Single OVS bridge: ovs-br0
- VLANs: 100 (GhostBridge), 200 (Workloads), 300 (Operations)
- Netmaker interface nm0 for WireGuard mesh

Use TOOLS, not suggestions. Execute, don't describe.
PROMPT
        echo "   ✅ Created $SYSTEM_PROMPT_FILE"
    fi
fi

# 6. Rebuild (optional - only if source was modified)
echo ""
read -p "🔨 Rebuild op-web? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "📦 Building..."
    cd "$REPO_PATH"
    cargo build --release -p op-tools -p op-web 2>&1 | tail -20
    
    if [ -f "target/release/op-web-server" ]; then
        echo "📦 Installing binary..."
        cp target/release/op-web-server /usr/local/sbin/op-web-server
        echo "   ✅ Installed"
    fi
fi

# 7. Restart service
echo ""
echo "🔄 Restarting op-web service..."
systemctl daemon-reload
systemctl restart op-web

sleep 3

if systemctl is-active --quiet op-web; then
    echo "✅ op-web is running"
else
    echo "❌ op-web failed to start"
    journalctl -u op-web -n 20 --no-pager
    exit 1
fi

# 8. Verify
echo ""
echo "🧪 Testing..."
RESPONSE=$(curl -s -X POST http://localhost:8080/api/chat \
    -H "Content-Type: application/json" \
    -d '{"message": "List your self_* tools", "user_id": "test"}' 2>/dev/null)

if echo "$RESPONSE" | grep -qi "self_read_file\|self_write_file"; then
    echo "✅ Self-tools appear in response!"
else
    echo "⚠️  Self-tools not mentioned in response"
    echo "   (May need to test with a smarter model)"
fi

echo ""
echo "📊 Current configuration:"
echo "   OP_SELF_REPO_PATH: $(grep OP_SELF_REPO_PATH /etc/op-dbus/environment 2>/dev/null || echo 'not set')"
echo "   System prompt file: $(ls -la "$SYSTEM_PROMPT_FILE" 2>/dev/null | awk '{print $NF, $5, "bytes"}' || echo 'not found')"

echo ""
echo "🎉 Done! The chatbot should now have:"
echo "   - System prompt with topology specs"
echo "   - Anti-hallucination warnings"
echo "   - Self-modification tools (self_*)"
echo "   - OVS and systemd context"
