#!/bin/bash
#
# Check Antigravity headless service status
#

set -euo pipefail

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║              ANTIGRAVITY HEADLESS STATUS                       ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

check_service() {
    local service="$1"
    if systemctl is-active --quiet "$service"; then
        echo -e "  ${GREEN}✓${NC} $service is running"
        return 0
    else
        echo -e "  ${RED}✗${NC} $service is not running"
        return 1
    fi
}

echo "Services:"
check_service "antigravity-display" || true
check_service "antigravity-vnc" || true
echo ""

# Check Wayland socket
XDG_RUNTIME_DIR="/run/user/$(id -u)"
WAYLAND_SOCKET="$XDG_RUNTIME_DIR/antigravity-0"

echo "Wayland Socket:"
if [[ -S "$WAYLAND_SOCKET" ]]; then
    echo -e "  ${GREEN}✓${NC} $WAYLAND_SOCKET exists"
else
    echo -e "  ${RED}✗${NC} $WAYLAND_SOCKET not found"
fi
echo ""

# Check VNC port
echo "VNC Server:"
if ss -tlnp | grep -q ':5900'; then
    echo -e "  ${GREEN}✓${NC} Listening on port 5900"
    echo "  $(ss -tlnp | grep ':5900' | head -1)"
else
    echo -e "  ${RED}✗${NC} Not listening on port 5900"
fi
echo ""

# Check for token
echo "OAuth Token:"
TOKEN_FILE="$HOME/.config/antigravity/token.json"
if [[ -f "$TOKEN_FILE" ]]; then
    echo -e "  ${GREEN}✓${NC} Token file exists: $TOKEN_FILE"
    
    # Check if token is valid
    if python3 -c "
import json
import time
with open('$TOKEN_FILE') as f:
    t = json.load(f)
exp = t.get('expires_at', 0)
if exp > 0:
    remaining = int(exp - time.time())
    if remaining > 0:
        print(f'    Expires in: {remaining // 60} minutes')
        exit(0)
    else:
        print('    Token EXPIRED (will auto-refresh on use)')
        exit(0)
else:
    print('    No expiry info (probably uses refresh_token)')
    exit(0)
" 2>/dev/null; then
        :
    fi
    
    if grep -q 'refresh_token' "$TOKEN_FILE"; then
        echo -e "  ${GREEN}✓${NC} Has refresh_token (can auto-refresh)"
    else
        echo -e "  ${YELLOW}!${NC} No refresh_token (may need to re-login)"
    fi
else
    echo -e "  ${YELLOW}!${NC} No token file yet"
    echo "    Log in via VNC, then run: ./scripts/antigravity-extract-token.sh"
fi
echo ""

# Connection info
echo "════════════════════════════════════════════════════════════════"
echo "Connection Info:"
PRIMARY_IP=$(hostname -I | awk '{print $1}')
echo "  VNC: vncviewer $PRIMARY_IP:5900"
echo "  SSH Tunnel: ssh -L 5900:localhost:5900 $(whoami)@$PRIMARY_IP"
echo "════════════════════════════════════════════════════════════════"
