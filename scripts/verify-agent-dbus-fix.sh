#!/bin/bash
# Verify agents are registered on D-Bus after the fix

echo "🔍 Verifying Agent D-Bus Registration (Post-Fix)"
echo ""

# 1. Check if op-web is running
echo "1️⃣ Service status:"
if systemctl is-active --quiet op-web; then
    echo "   ✅ op-web is running"
else
    echo "   ❌ op-web is NOT running"
    echo "   Start with: sudo systemctl start op-web"
    exit 1
fi
echo ""

# 2. Check for agent services on D-Bus
echo "2️⃣ Agent services on D-Bus:"
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
    echo "   Possible causes:"
    echo "   - op-web didn't call register_all_agents()"
    echo "   - dbus_service::start_agent() failed"
    echo "   - D-Bus policy doesn't allow registration"
    echo ""
    echo "   Check logs: sudo journalctl -u op-web -n 100 | grep -i agent"
fi
echo ""

# 3. Test introspection
echo "3️⃣ Testing agent introspection:"
for agent in RustPro PythonPro Memory SequentialThinking; do
    SERVICE="org.dbusmcp.Agent.$agent"
    PATH="/org/dbusmcp/Agent/$agent"
    
    if busctl --system introspect "$SERVICE" "$PATH" org.dbusmcp.Agent 2>/dev/null | head -1 > /dev/null; then
        NAME=$(busctl --system call "$SERVICE" "$PATH" org.dbusmcp.Agent name 2>/dev/null | grep -o '"[^"]*"' | tr -d '"' || echo "?")
        echo "   ✅ $agent: $NAME"
    else
        echo "   ❌ $agent: NOT AVAILABLE"
    fi
done
echo ""

# 4. Test execution
echo "4️⃣ Testing agent execution (Memory.list):"
TASK='{"type":"memory","operation":"list","config":{}}'
RESULT=$(busctl --system call "org.dbusmcp.Agent.Memory" "/org/dbusmcp/Agent/Memory" org.dbusmcp.Agent Execute s "$TASK" 2>/dev/null)

if [ -n "$RESULT" ]; then
    echo "   ✅ Execution successful"
    echo "   $RESULT" | head -c 200
else
    echo "   ❌ Execution failed - service may not be running"
fi
echo ""

# 5. Check MCP tools endpoint
echo "5️⃣ Testing MCP tools endpoint:"
RESPONSE=$(curl -s -X POST http://localhost:8080/mcp/tools/message \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' 2>/dev/null)

if [ -n "$RESPONSE" ]; then
    AGENT_TOOLS=$(echo "$RESPONSE" | jq -r '.result.tools[]?.name // empty' 2>/dev/null | grep "^agent_" | head -10)
    if [ -n "$AGENT_TOOLS" ]; then
        echo "   ✅ Agent tools found:"
        echo "$AGENT_TOOLS" | sed 's/^/      - /'
    else
        echo "   ⚠️ No agent_* tools in response"
    fi
else
    echo "   ❌ MCP endpoint not responding"
fi
echo ""

echo "📋 Summary"
echo "=========="
echo "The fix registers agents as D-Bus services when tools are registered."
echo "op-web startup should show:"
echo "  INFO Registering agents as D-Bus services..."
echo "  INFO Starting agent as D-Bus service (zbus) agent=rust-pro"
echo "  INFO ✓ Agent registered on D-Bus agent=rust-pro service=org.dbusmcp.Agent.RustPro"
echo "  ..."
echo "  INFO Registered N agents (0 failed)"
