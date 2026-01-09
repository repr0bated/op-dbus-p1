#!/bin/bash
# Setup admin prompt system
set -e

echo "🔧 Setting up Admin Prompt System"
echo ""

# Create directory
sudo mkdir -p /etc/op-dbus

# Create default custom prompt if not exists
if [ ! -f "/etc/op-dbus/custom-prompt.txt" ]; then
    echo "📝 Creating default custom prompt..."
    sudo tee /etc/op-dbus/custom-prompt.txt > /dev/null << 'PROMPT'
## ADDITIONAL INSTRUCTIONS

You are helpful, accurate, and security-conscious.

### Behavior Guidelines
- Be concise but thorough in explanations
- When in doubt, ask for clarification
- Prefer safe operations over risky ones
- Always confirm destructive actions

### Response Style
- Use markdown formatting for clarity
- Include relevant tool outputs
- Summarize long outputs when appropriate

### Custom Rules
- (Add your own rules here)
PROMPT
    echo "   ✅ Created /etc/op-dbus/custom-prompt.txt"
else
    echo "   ℹ️  Custom prompt already exists"
fi

# Set permissions
sudo chown root:root /etc/op-dbus/custom-prompt.txt
sudo chmod 644 /etc/op-dbus/custom-prompt.txt

echo ""
echo "🎉 Done! Admin prompt system ready."
echo ""
echo "Access the admin UI at: https://op-dbus.ghostbridge.tech/admin"
echo "Or edit directly: sudo nano /etc/op-dbus/custom-prompt.txt"
echo ""
echo "The system prompt now has:"
echo "  1. Fixed part (anti-hallucination, topology) - NOT editable"
echo "  2. Custom part (/etc/op-dbus/custom-prompt.txt) - EDITABLE"
echo "  3. Dynamic part (self-repo, tools) - Auto-generated"
