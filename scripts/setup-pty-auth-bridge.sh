#!/bin/bash
# Setup PTY Auth Bridge for headless authentication
#
# This creates wrapper scripts for CLI tools that need interactive auth

set -e

echo "🔐 Setting up PTY Auth Bridge"
echo ""

# Check if we're on a headless server
if [ -n "$DISPLAY" ] || [ -n "$WAYLAND_DISPLAY" ]; then
    echo "⚠️ GUI detected - you might not need this on a desktop system"
    echo "   Press Enter to continue or Ctrl+C to cancel"
    read
fi

# Create directories
mkdir -p ~/.config/pty-auth-bridge/sessions
mkdir -p ~/.local/bin

# Create wrapper scripts for common tools
cat > ~/.local/bin/gemini-headless << 'EOF'
#!/bin/bash
# Wrapper for Gemini CLI with headless auth support
#
# If auth is required, prints the URL to stderr and waits

exec 3>&1 4>&2

# Run gemini and capture output
OUTPUT=$(gemini "$@" 2>&1 | tee /dev/fd/4)

# Check for auth URLs in output
if echo "$OUTPUT" | grep -qi "https://accounts.google.com"; then
    AUTH_URL=$(echo "$OUTPUT" | grep -oE 'https://accounts.google.com[^ ]*')
    
    echo "" >&2
    echo "╔══════════════════════════════════════════════════════════════╗" >&2
    echo "║  🔐 AUTHENTICATION REQUIRED                                  ║" >&2
    echo "╠══════════════════════════════════════════════════════════════╣" >&2
    echo "║  Visit this URL in a browser:                                ║" >&2
    echo "║  $AUTH_URL" >&2
    echo "╚══════════════════════════════════════════════════════════════╝" >&2
    echo "" >&2
    
    # Send webhook if configured
    if [ -n "$PTY_AUTH_WEBHOOK" ]; then
        curl -s -X POST "$PTY_AUTH_WEBHOOK" \
            -H "Content-Type: application/json" \
            -d "{\"event\": \"auth_required\", \"tool\": \"gemini\", \"url\": \"$AUTH_URL\"}" \
            > /dev/null 2>&1 || true
    fi
fi
EOF
chmod +x ~/.local/bin/gemini-headless

cat > ~/.local/bin/gh-headless << 'EOF'
#!/bin/bash
# Wrapper for GitHub CLI with headless auth support

# Check if already authenticated
if gh auth status > /dev/null 2>&1; then
    exec gh "$@"
fi

# Run with device code flow (works headless)
if [ "$1" = "auth" ] && [ "$2" = "login" ]; then
    exec gh auth login --web --git-protocol https
else
    exec gh "$@"
fi
EOF
chmod +x ~/.local/bin/gh-headless

cat > ~/.local/bin/gcloud-headless << 'EOF'
#!/bin/bash
# Wrapper for gcloud with headless auth support

# Check if already authenticated
if gcloud auth list 2>/dev/null | grep -q ACTIVE; then
    exec gcloud "$@"
fi

# Use device code flow for login
if [ "$1" = "auth" ] && [ "$2" = "login" ]; then
    exec gcloud auth login --no-launch-browser
else
    exec gcloud "$@"
fi
EOF
chmod +x ~/.local/bin/gcloud-headless

# Create systemd user service for auth notification server
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/pty-auth-notify.service << 'EOF'
[Unit]
Description=PTY Auth Bridge Notification Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/python3 -m http.server 3334 --directory ~/.config/pty-auth-bridge
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

# Add to PATH if not already
if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
    echo ''
    echo '⚠️ Add this to your ~/.bashrc or ~/.zshrc:'
    echo '   export PATH="$HOME/.local/bin:$PATH"'
fi

echo ""
echo "✅ PTY Auth Bridge setup complete!"
echo ""
echo "📋 Wrapper scripts created:"
echo "   ~/.local/bin/gemini-headless"
echo "   ~/.local/bin/gh-headless"
echo "   ~/.local/bin/gcloud-headless"
echo ""
echo "🔔 To enable webhook notifications:"
echo "   export PTY_AUTH_WEBHOOK='https://your-server.com/webhook'"
echo ""
echo "🌐 To start the notification web server:"
echo "   systemctl --user enable pty-auth-notify"
echo "   systemctl --user start pty-auth-notify"
echo "   Then visit http://localhost:3334 to see pending auths"
echo ""
echo "📚 Usage:"
echo "   gemini-headless chat 'Hello world'"
echo "   gh-headless auth login"
echo "   gcloud-headless auth login"
