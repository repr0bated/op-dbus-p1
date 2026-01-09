#!/bin/bash
# Quick fix for LLM provider issue
# Run as root on the server

set -e

echo "🔧 Fixing LLM Configuration"
echo ""

# Check for Antigravity bridge
if [ -n "$ANTIGRAVITY_BRIDGE_URL" ]; then
    echo "✅ ANTIGRAVITY_BRIDGE_URL found in environment"
    USE_ANTIGRAVITY=true
elif grep -q "ANTIGRAVITY_BRIDGE_URL=" /etc/op-dbus/environment 2>/dev/null; then
    BRIDGE=$(grep "ANTIGRAVITY_BRIDGE_URL=" /etc/op-dbus/environment | cut -d= -f2)
    if [ -n "$BRIDGE" ] && [ "$BRIDGE" != "" ]; then
        echo "✅ ANTIGRAVITY_BRIDGE_URL found in environment file"
        USE_ANTIGRAVITY=true
    else
        USE_ANTIGRAVITY=false
    fi
else
    echo "⚠️  No ANTIGRAVITY_BRIDGE_URL found"
    USE_ANTIGRAVITY=false
fi

echo ""

# Check for Gemini API key
if [ -n "$GEMINI_API_KEY" ]; then
    echo "✅ GEMINI_API_KEY found in environment"
    USE_GEMINI=true
elif grep -q "GEMINI_API_KEY=" /etc/op-dbus/environment 2>/dev/null; then
    KEY=$(grep "GEMINI_API_KEY=" /etc/op-dbus/environment | cut -d= -f2)
    if [ -n "$KEY" ] && [ "$KEY" != "" ]; then
        echo "✅ GEMINI_API_KEY found in environment file"
        USE_GEMINI=true
    else
        USE_GEMINI=false
    fi
else
    echo "⚠️  No GEMINI_API_KEY found"
    USE_GEMINI=false
fi

echo ""
echo "🎯 Recommended fix:"

if [ "$USE_ANTIGRAVITY" = true ]; then
    echo "  Use Antigravity bridge (enterprise auth)"
    echo ""
    echo "  Applying fix..."

    sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=antigravity/' /etc/op-dbus/environment 2>/dev/null || \
        echo "LLM_PROVIDER=antigravity" >> /etc/op-dbus/environment
    sed -i 's/^LLM_MODEL=.*/LLM_MODEL=gemini-3-pro-preview/' /etc/op-dbus/environment 2>/dev/null || \
        echo "LLM_MODEL=gemini-3-pro-preview" >> /etc/op-dbus/environment

    mkdir -p /etc/systemd/system/op-web.service.d
    cat > /etc/systemd/system/op-web.service.d/llm.conf << 'EOF'
[Service]
Environment="LLM_PROVIDER=antigravity"
Environment="LLM_MODEL=gemini-3-pro-preview"
EOF

    echo "  ✅ Set LLM_PROVIDER=antigravity, LLM_MODEL=gemini-3-pro-preview"

elif [ "$USE_GEMINI" = true ]; then
    echo "  Use Gemini API (free tier)"
    echo ""
    echo "  Applying fix..."
    
    # Update environment file
    sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=gemini/' /etc/op-dbus/environment 2>/dev/null || \
        echo "LLM_PROVIDER=gemini" >> /etc/op-dbus/environment
    sed -i 's/^LLM_MODEL=.*/LLM_MODEL=gemini-2.0-flash/' /etc/op-dbus/environment 2>/dev/null || \
        echo "LLM_MODEL=gemini-2.0-flash" >> /etc/op-dbus/environment
    
    # Update systemd
    mkdir -p /etc/systemd/system/op-web.service.d
    cat > /etc/systemd/system/op-web.service.d/llm.conf << 'EOF'
[Service]
Environment="LLM_PROVIDER=gemini"
Environment="LLM_MODEL=gemini-2.0-flash"
EOF
    
    echo "  ✅ Set LLM_PROVIDER=gemini, LLM_MODEL=gemini-2.0-flash"
    
else
    echo "  ❌ No LLM backend available!"
    echo ""
    echo "  Options:"
    echo "  1. Configure Antigravity bridge"
    echo "  2. Get Gemini API key (free): https://aistudio.google.com/"
    exit 1
fi

echo ""
echo "🔄 Restarting service..."
systemctl daemon-reload
systemctl restart op-web

sleep 3

echo ""
echo "📊 Service status:"
if systemctl is-active --quiet op-web; then
    echo "  ✅ op-web is running"
else
    echo "  ❌ op-web failed to start"
    journalctl -u op-web -n 5 --no-pager
    exit 1
fi

echo ""
echo "🧪 Testing..."
RESPONSE=$(curl -s -X POST http://localhost:8080/api/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Say hello", "user_id": "test"}' 2>/dev/null)

if echo "$RESPONSE" | jq -e '.success == true' > /dev/null 2>&1; then
    echo "  ✅ Chat API working!"
    echo "  Provider: $(echo "$RESPONSE" | jq -r '.provider')"
    echo "  Model: $(echo "$RESPONSE" | jq -r '.model')"
else
    echo "  ⚠️  Chat test returned:"
    echo "$RESPONSE" | jq . 2>/dev/null || echo "$RESPONSE"
fi

echo ""
echo "🎉 Done! Try: curl -X POST https://op-dbus.ghostbridge.tech/api/chat -H 'Content-Type: application/json' -d '{\"message\": \"Hello\"}'"
