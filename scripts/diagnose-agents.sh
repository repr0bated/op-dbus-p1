#!/bin/bash
# Diagnose agent failures

echo "🔍 Diagnosing Agent System"
echo ""

# 1. Check journal for agent errors
echo "1️⃣ Recent agent errors from journal:"
sudo journalctl -u op-web -n 100 --no-pager 2>/dev/null | grep -i "agent\|dbus.*agent\|failed.*agent" | tail -20
echo ""

# 2. Check if D-Bus agent services exist
echo "2️⃣ Checking for D-Bus agent services:"
for service in RustPro PythonPro BackendArchitect NetworkEngineer Memory ContextManager SequentialThinking; do
    if busctl --system introspect "org.dbusmcp.Agent.${service}" / 2>/dev/null; then
        echo "   ✅ org.dbusmcp.Agent.${service} exists"
    else
        echo "   ❌ org.dbusmcp.Agent.${service} NOT FOUND"
    fi
done
echo ""

# 3. Check MCP agents endpoint
echo "3️⃣ Testing MCP agents endpoint:"
RESPONSE=$(curl -s -X POST http://localhost:8080/mcp/agents/message \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' 2>/dev/null)

if [ -n "$RESPONSE" ]; then
    TOOL_COUNT=$(echo "$RESPONSE" | jq '.result.tools | length' 2>/dev/null || echo "?")
    echo "   Tools available: $TOOL_COUNT"
    echo "   First few tools:"
    echo "$RESPONSE" | jq -r '.result.tools[:5] | .[].name' 2>/dev/null | sed 's/^/     /'
else
    echo "   ❌ No response from MCP agents endpoint"
fi
echo ""

# 4. Test an agent call
echo "4️⃣ Testing agent execution:"
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
    echo "   ❌ memory_list failed:"
    echo "$RESPONSE" | jq -r '.result // .error' 2>/dev/null | head -10 | sed 's/^/     /'
fi
echo ""

# 5. Check what executor is being used
echo "5️⃣ Checking executor type:"
REPO=""
for p in /home/jeremy/git/op-dbus /home/jeremy/op-dbus /opt/op-dbus; do
    if [ -d "$p/crates" ]; then
        REPO="$p"
        break
    fi
done

if [ -n "$REPO" ]; then
    echo "   Searching for executor in $REPO..."
    grep -n "AgentsServer::new\|DbusAgentExecutor\|InMemoryAgentExecutor\|TraitAgentExecutor" \
        "$REPO/crates/op-mcp/src/agents_server.rs" 2>/dev/null | head -5 | sed 's/^/   /'
    
    grep -n "AgentsServer::" "$REPO/crates/op-web/src/"*.rs 2>/dev/null | head -5 | sed 's/^/   /'
fi
echo ""

echo "📋 DIAGNOSIS SUMMARY"
echo ""
echo "The agent system has these components:"
echo "  1. AgentsServer (MCP server) - exposes agents via MCP"
echo "  2. AgentExecutor (trait) - executes agent calls"
echo "  3. DbusAgentExecutor - tries to call D-Bus services (FAILING)"
echo "  4. InMemoryAgentExecutor - mock/test executor (WORKS)"
echo "  5. CriticalAgentsState - uses AgentTrait directly (WORKS)"
echo ""
echo "The problem: DbusAgentExecutor calls D-Bus services that don't exist."
echo ""
echo "FIX: Change AgentsServer to use trait-based execution instead of D-Bus."
echo "     See the TraitAgentExecutor implementation in agents_server.rs.patch"
