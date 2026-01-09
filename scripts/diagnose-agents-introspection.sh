#!/bin/bash
# Diagnose agent availability using introspection

echo "🔍 Agent Introspection Diagnosis"
echo ""

# 1. Check D-Bus connection
echo "1️⃣ D-Bus System Bus:"
if [ -S "/var/run/dbus/system_bus_socket" ]; then
    echo "   ✅ Socket exists"
else
    echo "   ❌ Socket not found"
fi

# 2. List all D-Bus services
echo ""
echo "2️⃣ All D-Bus Services (system bus):"
busctl --system list 2>/dev/null | head -20 || echo "   Cannot list services"

# 3. Check for agent services specifically
echo ""
echo "3️⃣ Agent D-Bus Services:"
for agent in RustPro PythonPro BackendArchitect NetworkEngineer Memory ContextManager SequentialThinking; do
    SERVICE="org.dbusmcp.Agent.${agent}"
    if busctl --system introspect "$SERVICE" / 2>/dev/null | head -1 > /dev/null; then
        echo "   ✅ $SERVICE - AVAILABLE"
    else
        echo "   ❌ $SERVICE - NOT FOUND"
    fi
done

# 4. Check what the MCP endpoint returns
echo ""
echo "4️⃣ MCP Agents Endpoint:"
RESPONSE=$(curl -s -X POST http://localhost:8080/mcp/agents/message \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' 2>/dev/null)

if [ -n "$RESPONSE" ]; then
    TOOLS=$(echo "$RESPONSE" | jq -r '.result.tools[]?.name // empty' 2>/dev/null)
    if [ -n "$TOOLS" ]; then
        echo "   Available agent tools:"
        echo "$TOOLS" | sed 's/^/     - /'
    else
        echo "   No tools or error:"
        echo "$RESPONSE" | jq . 2>/dev/null | head -20 | sed 's/^/     /'
    fi
else
    echo "   ❌ No response from MCP endpoint"
fi

# 5. Test an agent call
echo ""
echo "5️⃣ Testing memory_list tool:"
RESPONSE=$(curl -s -X POST http://localhost:8080/mcp/agents/message \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "memory_list",
            "arguments": {}
        }
    }' 2>/dev/null)

if echo "$RESPONSE" | jq -e '.result.content' > /dev/null 2>&1; then
    echo "   ✅ memory_list succeeded"
    echo "$RESPONSE" | jq -r '.result.content[0].text' 2>/dev/null | head -5 | sed 's/^/     /'
else
    echo "   Result:"
    echo "$RESPONSE" | jq . 2>/dev/null | head -15 | sed 's/^/     /'
fi

echo ""
echo "📋 SUMMARY"
echo "=========="
echo ""
echo "The introspection-aware agent system should:"
echo "1. Check D-Bus for org.dbusmcp.Agent.* services"
echo "2. If service exists → Use D-Bus executor"
echo "3. If service missing → Use trait executor (if registered)"
echo "4. If neither → Mark agent unavailable"
echo ""
echo "To fix missing agents, either:"
echo "  a) Start the D-Bus service:"
echo "     systemctl start op-agent-rustpro.service"
echo "  b) Register trait implementations:"
echo "     (Already done in builtin_trait_agents.rs)"
