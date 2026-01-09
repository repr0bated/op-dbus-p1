#!/bin/bash
# Fix agent execution by switching to trait-based executor

set -e

echo "🔧 Fixing Agent System"
echo ""

# Find repo
REPO=""
for p in /home/jeremy/git/op-dbus /home/jeremy/op-dbus /opt/op-dbus; do
    if [ -d "$p/crates/op-mcp" ]; then
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

# 1. Check if TraitAgentExecutor exists
if [ -f "crates/op-mcp/src/trait_agent_executor.rs" ]; then
    echo "✅ TraitAgentExecutor already exists"
else
    echo "📝 Creating TraitAgentExecutor..."
    # The file should be created from the source I provided above
    echo "   Please create crates/op-mcp/src/trait_agent_executor.rs"
fi

# 2. Check agents_server.rs for executor usage
echo ""
echo "📋 Current executor in agents_server.rs:"
grep -n "AgentsServer::new\|executor:" crates/op-mcp/src/agents_server.rs | head -10 | sed 's/^/   /'

# 3. Update AgentsServer::new to use TraitAgentExecutor
echo ""
echo "📝 To fix, update AgentsServer::new() in agents_server.rs:"
echo ""
echo '   // Change this:'
echo '   executor: Arc::new(DbusAgentExecutor::new()),'
echo ''
echo '   // To this:'
echo '   executor: Arc::new(TraitAgentExecutor::new()),'
echo ""

# 4. Rebuild
read -p "🔨 Rebuild op-mcp? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "📦 Building..."
    cargo build --release -p op-mcp -p op-web 2>&1 | tail -20
    
    if [ -f "target/release/op-web-server" ]; then
        echo "📦 Installing..."
        sudo cp target/release/op-web-server /usr/local/sbin/
        sudo systemctl restart op-web
        echo "✅ Installed and restarted"
    fi
fi

# 5. Test
echo ""
echo "🧪 Testing agents..."
sleep 3

RESPONSE=$(curl -s -X POST http://localhost:8080/mcp/agents/message \
    -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_list",
            "arguments": {}
        }
    }' 2>/dev/null)

if echo "$RESPONSE" | jq -e '.result.content' > /dev/null 2>&1; then
    echo "✅ Agent execution works!"
    echo "$RESPONSE" | jq -r '.result.content[0].text' | head -5
else
    echo "❌ Agent execution still failing:"
    echo "$RESPONSE" | jq .
fi
