#!/bin/bash
# Verify agent modules are intact after Grok's changes

echo "🔍 Verifying Agent Module State"
echo ""

REPO=""
for p in /home/jeremy/git/op-dbus /home/jeremy/op-dbus /opt/op-dbus .; do
    if [ -d "$p/crates/op-agents" ]; then
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

# 1. Check agent directory structure
echo "1️⃣ Agent Directory Structure:"
AGENT_DIR="crates/op-agents/src/agents"
if [ -d "$AGENT_DIR" ]; then
    echo "   ✅ $AGENT_DIR exists"
    echo ""
    echo "   Subdirectories:"
    for dir in "$AGENT_DIR"/*/; do
        if [ -d "$dir" ]; then
            name=$(basename "$dir")
            if [ -f "${dir}mod.rs" ]; then
                echo "   ✅ $name/ (has mod.rs)"
            else
                echo "   ⚠️ $name/ (MISSING mod.rs)"
            fi
        fi
    done
else
    echo "   ❌ $AGENT_DIR does not exist!"
fi
echo ""

# 2. Check for conflicting standalone .rs files
echo "2️⃣ Checking for Conflicting Standalone Files:"
STANDALONE=$(ls "$AGENT_DIR"/*.rs 2>/dev/null | grep -v mod.rs | grep -v base.rs || true)
if [ -n "$STANDALONE" ]; then
    echo "   ⚠️ Found standalone .rs files that may conflict with directories:"
    echo "$STANDALONE" | sed 's/^/      /'
    echo ""
    echo "   If these match directory names, you have a Rust module conflict!"
else
    echo "   ✅ No conflicting standalone files"
fi
echo ""

# 3. Check agents/mod.rs for module declarations
echo "3️⃣ Checking agents/mod.rs:"
MOD_RS="$AGENT_DIR/mod.rs"
if [ -f "$MOD_RS" ]; then
    echo "   ✅ mod.rs exists"
    echo "   Module declarations:"
    grep "^pub mod" "$MOD_RS" | sed 's/^/      /'
else
    echo "   ❌ mod.rs missing!"
fi
echo ""

# 4. Check lib.rs for create_agent function
echo "4️⃣ Checking lib.rs for create_agent:"
LIB_RS="crates/op-agents/src/lib.rs"
if [ -f "$LIB_RS" ]; then
    if grep -q "pub fn create_agent" "$LIB_RS"; then
        echo "   ✅ create_agent function exists"
    else
        echo "   ❌ create_agent function MISSING"
        echo "   This is needed for agent tool registration!"
    fi
else
    echo "   ❌ lib.rs missing!"
fi
echo ""

# 5. Try to compile
echo "5️⃣ Attempting Compilation:"
echo "   Running: cargo check -p op-agents"
if cargo check -p op-agents 2>&1 | tail -30; then
    echo ""
    if cargo check -p op-agents 2>&1 | grep -q "^error"; then
        echo "   ❌ Compilation has errors"
    else
        echo "   ✅ op-agents compiles successfully"
    fi
else
    echo "   ❌ Compilation failed"
fi
echo ""

# 6. Check Cargo.toml dependencies
echo "6️⃣ Checking Cargo.toml Dependencies:"
CARGO_TOML="crates/op-agents/Cargo.toml"
if [ -f "$CARGO_TOML" ]; then
    echo "   Dependencies:"
    grep -E "^(tokio|serde|zbus|async-trait|anyhow|thiserror)" "$CARGO_TOML" 2>/dev/null | head -10 | sed 's/^/      /'
    
    if grep -q "workspace = true\|workspace = " "$CARGO_TOML"; then
        echo "   ✅ Uses workspace dependencies"
    else
        echo "   ⚠️ May not be using workspace dependencies"
    fi
fi
echo ""

echo "📋 Summary"
echo "=========="
echo "The agent structure should have:"
echo "  - crates/op-agents/src/agents/ with subdirectories:"
echo "    language/, architecture/, infrastructure/, orchestration/,"
echo "    seo/, analysis/, aiml/, business/, content/, database/,"
echo "    mobile/, operations/, security/, specialty/, webframeworks/"
echo "  - Each subdirectory has mod.rs with agent implementations"
echo "  - lib.rs exports create_agent() function"
echo "  - No standalone .rs files that conflict with directories"
