#!/bin/bash
# Check if agent_tool.rs compiles

set -e

echo "🔍 Checking agent tool compilation"
echo ""

cd "$(dirname "$0")/.."

# Try to compile just op-tools
echo "1️⃣ Compiling op-tools..."
if cargo check -p op-tools 2>&1 | head -50; then
    echo "   ✅ op-tools compiles"
else
    echo "   ❌ op-tools has errors"
    echo ""
    echo "   Full error output:"
    cargo check -p op-tools 2>&1
fi
echo ""

# Check what's exported from op-agents
echo "2️⃣ Checking op-agents exports:"
echo "   Looking for dbus_service module..."
if [ -f "crates/op-agents/src/dbus_service.rs" ]; then
    echo "   ✅ dbus_service.rs exists"
    grep -n "pub fn\|pub async fn\|pub struct" crates/op-agents/src/dbus_service.rs | head -10
else
    echo "   ❌ dbus_service.rs not found"
fi
echo ""

echo "   Looking for create_agent function..."
grep -rn "pub fn create_agent" crates/op-agents/src/ || echo "   ❌ create_agent not found"
echo ""

echo "   Looking for AgentTrait..."
grep -rn "pub trait.*Agent" crates/op-agents/src/ | head -5 || echo "   ❌ AgentTrait not found"
