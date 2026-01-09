#!/bin/bash
# Verify agent D-Bus registration after the fix

echo "🔍 Verifying Agent D-Bus Registration"
echo ""

# 1. Check op-web status
echo "1️⃣ Service Status:"
if systemctl is-active --quiet op-web 2>/dev/null; then
    echo "   ✅ op-web is running"
else
    echo "   ❌ op-web is NOT running"
    echo "   Start with: sudo systemctl start op-web"
fi
echo ""

# 2. Check for agent services on D-Bus
echo "2️⃣ Agent D-Bus Services:"
AGENTS=$(busctl --system list 2>/dev/null | grep "org.dbusmcp.Agent" || true)
if [ -n "$AGENTS" ]; then
    COUNT=$(echo "$AGENTS" | wc -l)
    echo "   ✅ Found $COUNT agent services:"
    echo "$AGENTS" | while read line; do
        SERVICE=$(echo "$line" | awk '{print $1}')
        echo "      - $SERVICE"
    done
else
    echo "   ❌ No agent services on D-Bus"
    echo ""
    echo "   Check logs: sudo journalctl -u op-web -n 100 | grep -i agent"
fi
echo ""

# 3. Test agent execution
echo "3️⃣ Testing Agent Execution:"
for agent in Memory SequentialThinking; do
    SERVICE="org.dbusmcp.Agent.$agent"
    PATH="/org/dbusmcp/Agent/$agent"
    TASK='{"type":"test","operation":"list","args":null}'
    
    RESULT=$(busctl --system call "$SERVICE" "$PATH" org.dbusmcp.Agent Execute s "$TASK" 2>/dev/null)
    if [ -n "$RESULT" ]; then
        echo "   ✅ $agent: responds"
    else
        echo "   ❌ $agent: no response"
    fi
done
echo ""

# 4. Test via HTTP
echo "4️⃣ Testing via HTTP API:"
RESPONSE=$(curl -s http://localhost:8080/api/tools 2>/dev/null | head -c 500)
if echo "$RESPONSE" | grep -q "agent_"; then
    AGENT_COUNT=$(echo "$RESPONSE" | grep -o 'agent_[a-z_]*' | sort -u | wc -l)
    echo "   ✅ Found $AGENT_COUNT agent tools via API"
else
    echo "   ⚠️ No agent_* tools in API response"
fi
echo ""

echo "📋 Summary"
echo "=========="
echo "The fix removes dependency on op_agents::create_agent()"
echo "Agent definitions are now static in agent_tool.rs"
echo "D-Bus services created directly using zbus"
