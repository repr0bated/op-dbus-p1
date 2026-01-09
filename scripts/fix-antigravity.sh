#!/bin/bash
# Fix Antigravity provider and restart service
set -e

echo "🔧 Fixing Antigravity Provider"

# Check if GEMINI_API_KEY is set
if [ -z "$GEMINI_API_KEY" ]; then
    echo "⚠️  GEMINI_API_KEY not found in environment"
    echo "Get one free at: https://aistudio.google.com/"
    echo ""
    read -p "Enter your Gemini API key: " GEMINI_API_KEY
    
    if [ -z "$GEMINI_API_KEY" ]; then
        echo "❌ No API key provided, exiting"
        exit 1
    fi
fi

# Add to environment file
echo "📝 Updating /etc/op-dbus/environment..."
sudo grep -q "^GEMINI_API_KEY=" /etc/op-dbus/environment 2>/dev/null && \
    sudo sed -i "s|^GEMINI_API_KEY=.*|GEMINI_API_KEY=$GEMINI_API_KEY|" /etc/op-dbus/environment || \
    echo "GEMINI_API_KEY=$GEMINI_API_KEY" | sudo tee -a /etc/op-dbus/environment > /dev/null

sudo grep -q "^LLM_PROVIDER=" /etc/op-dbus/environment 2>/dev/null && \
    sudo sed -i "s|^LLM_PROVIDER=.*|LLM_PROVIDER=antigravity|" /etc/op-dbus/environment || \
    echo "LLM_PROVIDER=antigravity" | sudo tee -a /etc/op-dbus/environment > /dev/null

sudo grep -q "^LLM_MODEL=" /etc/op-dbus/environment 2>/dev/null && \
    sudo sed -i "s|^LLM_MODEL=.*|LLM_MODEL=gemini-2.0-flash|" /etc/op-dbus/environment || \
    echo "LLM_MODEL=gemini-2.0-flash" | sudo tee -a /etc/op-dbus/environment > /dev/null

# Update systemd override
echo "📝 Updating systemd service..."
sudo mkdir -p /etc/systemd/system/op-web.service.d
sudo tee /etc/systemd/system/op-web.service.d/antigravity.conf > /dev/null << EOF
[Service]
Environment="LLM_PROVIDER=antigravity"
Environment="GEMINI_API_KEY=$GEMINI_API_KEY"
Environment="LLM_MODEL=gemini-2.0-flash"
Environment="ANTIGRAVITY_AUTO_ROUTING=true"
Environment="ANTIGRAVITY_AGENTIC=true"
EOF

# Rebuild if source exists
if [ -d "crates/op-llm" ]; then
    echo "🔨 Rebuilding op-llm..."
    cargo build --release -p op-llm -p op-web 2>/dev/null || echo "⚠️  Build skipped (run manually)"
    
    # Copy binary if build succeeded
    if [ -f "target/release/op-web-server" ]; then
        echo "📦 Installing new binary..."
        sudo cp target/release/op-web-server /usr/local/sbin/op-web-server
    fi
fi

# Restart service
echo "🔄 Restarting op-web service..."
sudo systemctl daemon-reload
sudo systemctl restart op-web

# Wait and check
sleep 3
if systemctl is-active --quiet op-web; then
    echo "✅ op-web is running"
else
    echo "❌ op-web failed to start"
    sudo journalctl -u op-web -n 10 --no-pager
    exit 1
fi

# Test the API
echo ""
echo "🧪 Testing Antigravity..."
RESPONSE=$(curl -s -X POST http://localhost:8080/api/chat \
    -H "Content-Type: application/json" \
    -d '{"message": "Say hello", "user_id": "test"}' 2>/dev/null)

PROVIDER=$(echo "$RESPONSE" | jq -r '.provider // "unknown"' 2>/dev/null)
SUCCESS=$(echo "$RESPONSE" | jq -r '.success // false' 2>/dev/null)

if [ "$PROVIDER" = "antigravity" ] && [ "$SUCCESS" = "true" ]; then
    echo "✅ Antigravity provider is working!"
    echo "   Provider: $PROVIDER"
    echo "   Model: $(echo "$RESPONSE" | jq -r '.model')"
else
    echo "⚠️  Provider returned: $PROVIDER"
    echo "   Response: $RESPONSE"
fi

echo ""
echo "🎉 Done! Test with:"
echo "   curl -X POST https://op-dbus.ghostbridge.tech/api/chat -H 'Content-Type: application/json' -d '{\"message\": \"Hello\"}'"
