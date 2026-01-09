#!/bin/bash
# Verify D-Bus agents are running and discoverable

echo "🔍 Verifying D-Bus Agents"
echo ""

# 1. Check for agent services
echo "1️⃣ Agent services on D-Bus:"
AGENTS=$(busctl --system list 2>/dev/null | grep "org.dbusmcp.Agent" || true)
if [ -n "$AGENTS" ]; then
    echo "$AGENTS" | while read line; do
        SERVICE=$(echo "$line" | awk '{print $1}')
        echo "   ✅ $SERVICE"
    done
else
    echo "   ❌ No agent services found"
    echo ""
    echo "   To start agents:"
    echo "   ./scripts/start-dbus-agents.sh"
    exit 1
fi
echo ""

# 2. Test introspection
echo "2️⃣ Testing introspection:"
for agent in RustPro PythonPro Memory SequentialThinking; do
    SERVICE="org.dbusmcp.Agent.$agent"
    PATH="/org/dbusmcp/Agent/$agent"
    
    if busctl --system introspect "$SERVICE" "$PATH" "org.dbusmcp.Agent" 2>/dev/null | head -1 > /dev/null; then
        # Get operations
        OPS=$(busctl --system call "$SERVICE" "$PATH" "org.dbusmcp.Agent" "operations" 2>/dev/null | grep -o '"[^"]*"' | tr -d '"' | head -5 | tr '\n' ',' | sed 's/,$//')
        echo "   ✅ $agent: $OPS"
    else
        echo "   ❌ $agent: not responding"
    fi
done
echo ""

# 3. Test an agent call
echo "3️⃣ Testing agent execution:"
TEST_TASK='{"task_type":"memory","operation":"list","config":{}}'
RESULT=$(busctl --system call "org.dbusmcp.Agent.Memory" "/org/dbusmcp/Agent/Memory" "org.dbusmcp.Agent" "Execute" "s" "$TEST_TASK" 2>/dev/null)

if [ -n "$RESULT" ]; then
    echo "   ✅ Memory.Execute succeeded"
    echo "   $RESULT" | head -c 200
else
    echo "   ❌ Memory.Execute failed"
fi
echo ""

# 4. Check MCP endpoint
echo "4️⃣ Testing MCP agents endpoint:"
RESPONSE=$(curl -s -X POST http://localhost:8080/mcp/agents/message \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' 2>/dev/null)

if [ -n "$RESPONSE" ]; then
    TOOL_COUNT=$(echo "$RESPONSE" | jq '.result.tools | length' 2>/dev/null || echo "?")
    echo "   ✅ MCP endpoint responding: $TOOL_COUNT tools"
else
    echo "   ❌ MCP endpoint not responding"
fi
echo ""

echo "📋 Summary"
echo "=========="
echo "D-Bus agents should be discovered via introspection."
echo "If agents aren't running, start them with:"
echo "  ./scripts/start-dbus-agents.sh --service"
