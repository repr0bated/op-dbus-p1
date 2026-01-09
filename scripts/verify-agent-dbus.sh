#!/bin/bash
# Verify agents are registered on D-Bus

echo "🔍 Verifying Agent D-Bus Registration"
echo ""

# Check for agent services
echo "1️⃣ Agent services on D-Bus:"
AGENTS=$(busctl --system list 2>/dev/null | grep "org.dbusmcp.Agent" || true)
if [ -n "$AGENTS" ]; then
    echo "$AGENTS" | while read line; do
        SERVICE=$(echo "$line" | awk '{print $1}')
        echo "   ✅ $SERVICE"
    done
    echo ""
    echo "   Total: $(echo "$AGENTS" | wc -l) agents registered"
else
    echo "   ❌ No agent services found on D-Bus"
    echo ""
    echo "   Agents should be started automatically when tools are registered."
    echo "   Check if op-web is running and check logs:"
    echo "   sudo journalctl -u op-web -n 50 | grep -i agent"
fi
echo ""

# Test introspection
echo "2️⃣ Testing agent introspection:"
for agent in RustPro PythonPro Memory SequentialThinking; do
    SERVICE="org.dbusmcp.Agent.$agent"
    PATH="/org/dbusmcp/Agent/$agent"
    
    if busctl --system introspect "$SERVICE" "$PATH" org.dbusmcp.Agent 2>/dev/null | head -1 > /dev/null; then
        NAME=$(busctl --system call "$SERVICE" "$PATH" org.dbusmcp.Agent name 2>/dev/null | grep -o '"[^"]*"' | tr -d '"')
        echo "   ✅ $agent: $NAME"
    else
        echo "   ❌ $agent: NOT AVAILABLE"
    fi
done
echo ""

# Test execution
echo "3️⃣ Testing agent execution (Memory.list):"
TASK='{"type":"memory","operation":"list","config":{}}'
RESULT=$(busctl --system call "org.dbusmcp.Agent.Memory" "/org/dbusmcp/Agent/Memory" org.dbusmcp.Agent Execute s "$TASK" 2>/dev/null)

if [ -n "$RESULT" ]; then
    echo "   ✅ Execution successful"
    echo "   $RESULT" | head -c 200
else
    echo "   ❌ Execution failed"
fi
echo ""

echo "📋 Summary"
echo "=========="
echo "Agents are now started as D-Bus services during tool registration."
echo "When op-web starts, it:"
echo "  1. Creates ToolRegistry"
echo "  2. Calls register_all_builtin_tools()"
echo "  3. Which calls register_all_agents()"
echo "  4. Each agent is started via dbus_service::start_agent()"
echo "  5. D-Bus connection is held in AgentConnectionRegistry"
echo "  6. Agent tool is registered pointing to D-Bus service"
