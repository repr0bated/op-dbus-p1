#!/bin/bash
# Diagnose system prompt issues

echo "🔍 Diagnosing System Prompt Configuration"
echo ""

# 1. Check OP_SELF_REPO_PATH
echo "1️⃣ Checking OP_SELF_REPO_PATH..."
if [ -n "$OP_SELF_REPO_PATH" ]; then
    echo "   ✅ Set to: $OP_SELF_REPO_PATH"
    if [ -d "$OP_SELF_REPO_PATH" ]; then
        echo "   ✅ Directory exists"
    else
        echo "   ❌ Directory does NOT exist"
    fi
else
    echo "   ❌ NOT SET - self-tools won't work"
fi
echo ""

# 2. Check environment file
echo "2️⃣ Checking /etc/op-dbus/environment..."
if [ -f "/etc/op-dbus/environment" ]; then
    echo "   Contents:"
    grep -E "(OP_SELF_REPO|SYSTEM_PROMPT|LLM)" /etc/op-dbus/environment | sed 's/^/   /'
else
    echo "   ❌ File not found"
fi
echo ""

# 3. Check systemd environment
echo "3️⃣ Checking systemd service environment..."
sudo systemctl show op-web -p Environment 2>/dev/null | sed 's/^/   /'
echo ""

# 4. Check for system prompt files
echo "4️⃣ Looking for system prompt files..."
for path in \
    "/home/jeremy/git/gemini-op-dbus/LLM-SYSTEM-PROMPT-COMPLETE.txt" \
    "/home/jeremy/op-dbus-v2/LLM-SYSTEM-PROMPT-COMPLETE.txt" \
    "./LLM-SYSTEM-PROMPT-COMPLETE.txt" \
    "../LLM-SYSTEM-PROMPT-COMPLETE.txt" \
    "./SYSTEM-PROMPT.md" \
    "../SYSTEM-PROMPT.md"; do
    if [ -f "$path" ]; then
        echo "   ✅ Found: $path ($(wc -l < "$path") lines)"
    fi
done
echo ""

# 5. Check if self_tools module is wired
echo "5️⃣ Checking if self_tools module is registered..."
REPO_PATH="${OP_SELF_REPO_PATH:-/home/jeremy/git/op-dbus}"
if [ -d "$REPO_PATH" ]; then
    if grep -q "pub mod self_tools" "$REPO_PATH/crates/op-tools/src/builtin/mod.rs" 2>/dev/null; then
        echo "   ✅ self_tools module declared in mod.rs"
    else
        echo "   ❌ self_tools NOT declared in builtin/mod.rs"
    fi
    
    if grep -q "create_self_tools" "$REPO_PATH/crates/op-tools/src/builtin/mod.rs" 2>/dev/null; then
        echo "   ✅ create_self_tools called in registration"
    else
        echo "   ❌ create_self_tools NOT called - tools won't be registered"
    fi
else
    echo "   ⚠️  Cannot check - repo path not found"
fi
echo ""

# 6. Test chat endpoint
echo "6️⃣ Testing chat endpoint..."
RESPONSE=$(curl -s -X POST http://localhost:8080/api/chat \
    -H "Content-Type: application/json" \
    -d '{"message": "What is your system prompt? What tools do you have for self-modification?", "user_id": "test"}' 2>/dev/null)

if [ -n "$RESPONSE" ]; then
    echo "   Response received ($(echo "$RESPONSE" | wc -c) bytes)"
    PROVIDER=$(echo "$RESPONSE" | jq -r '.provider // "unknown"' 2>/dev/null)
    MODEL=$(echo "$RESPONSE" | jq -r '.model // "unknown"' 2>/dev/null)
    SUCCESS=$(echo "$RESPONSE" | jq -r '.success // false' 2>/dev/null)
    echo "   Provider: $PROVIDER"
    echo "   Model: $MODEL"
    echo "   Success: $SUCCESS"
    
    # Check if response mentions self tools
    if echo "$RESPONSE" | grep -qi "self_read_file\|self_write_file\|self-modification"; then
        echo "   ✅ Response mentions self-tools"
    else
        echo "   ❌ Response does NOT mention self-tools"
    fi
else
    echo "   ❌ No response from chat endpoint"
fi
echo ""

# 7. Check logs for system prompt
echo "7️⃣ Checking logs for system prompt loading..."
sudo journalctl -u op-web -n 100 --no-pager 2>/dev/null | grep -i "system.prompt\|self.repo\|self_tools" | tail -10 | sed 's/^/   /'
echo ""

echo "📋 SUMMARY"
echo "=========="
echo ""
echo "To fix system prompt issues:"
echo ""
echo "1. Set OP_SELF_REPO_PATH:"
echo "   sudo tee /etc/systemd/system/op-web.service.d/self-repo.conf << 'EOF'"
echo "   [Service]"
echo "   Environment=\"OP_SELF_REPO_PATH=/home/jeremy/git/op-dbus\""
echo "   EOF"
echo ""
echo "2. Ensure self_tools is wired in mod.rs (see fix below)"
echo ""
echo "3. Rebuild and restart:"
echo "   cargo build --release -p op-tools -p op-web"
echo "   sudo cp target/release/op-web-server /usr/local/sbin/"
echo "   sudo systemctl daemon-reload"
echo "   sudo systemctl restart op-web"
