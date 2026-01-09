#!/bin/bash
#
# Extract OAuth token from running Antigravity session
#
# After you've logged into Antigravity via VNC, this script
# finds and copies the OAuth token for use by op-dbus.
#
# Usage:
#   ./scripts/antigravity-extract-token.sh
#

set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

# Output location for op-dbus
OPDBUS_TOKEN_FILE="${HOME}/.config/antigravity/token.json"
mkdir -p "$(dirname "$OPDBUS_TOKEN_FILE")"

log_info "Searching for Antigravity OAuth tokens..."

# Known locations where Antigravity might store tokens
# (VS Code-based apps typically use these patterns)
TOKEN_PATHS=(
    "$HOME/.config/antigravity"
    "$HOME/.antigravity"
    "$HOME/.config/Antigravity"
    "$HOME/.local/share/antigravity"
    "$HOME/.config/Code/User/globalStorage"
    "$HOME/.vscode-server/data/User/globalStorage"
    # gcloud ADC (might be shared auth)
    "$HOME/.config/gcloud"
)

FOUND_TOKEN=""

for base_path in "${TOKEN_PATHS[@]}"; do
    if [[ ! -d "$base_path" ]]; then
        continue
    fi
    
    log_info "Checking: $base_path"
    
    # Find JSON files that might contain tokens
    while IFS= read -r -d '' file; do
        # Check if file contains token-like data
        if grep -q -E '"(access_token|refresh_token|id_token)"' "$file" 2>/dev/null; then
            log_info "Found potential token file: $file"
            
            # Validate it's parseable JSON with token
            if python3 -c "
import json
import sys
with open('$file') as f:
    data = json.load(f)
if 'access_token' in data or 'refresh_token' in data:
    sys.exit(0)
sys.exit(1)
" 2>/dev/null; then
                FOUND_TOKEN="$file"
                break 2
            fi
        fi
    done < <(find "$base_path" -name "*.json" -type f -print0 2>/dev/null)
done

# Also check for gcloud ADC
GCLOUD_ADC="$HOME/.config/gcloud/application_default_credentials.json"
if [[ -z "$FOUND_TOKEN" ]] && [[ -f "$GCLOUD_ADC" ]]; then
    if grep -q 'refresh_token' "$GCLOUD_ADC"; then
        log_info "Found gcloud application_default_credentials.json"
        FOUND_TOKEN="$GCLOUD_ADC"
    fi
fi

if [[ -z "$FOUND_TOKEN" ]]; then
    log_error "No OAuth token found!"
    echo ""
    echo "Make sure you've logged into Antigravity via VNC first."
    echo "The token is stored after successful Google OAuth login."
    echo ""
    echo "Try:"
    echo "  1. Connect via VNC: vncviewer localhost:5900"
    echo "  2. Log in with your Google account in Antigravity"
    echo "  3. Run this script again"
    exit 1
fi

log_info "Extracting token from: $FOUND_TOKEN"

# Copy and normalize the token
python3 << PYEOF
import json
import time
import os

with open('$FOUND_TOKEN') as f:
    data = json.load(f)

# Normalize to our format
token = {
    'access_token': data.get('access_token', ''),
    'refresh_token': data.get('refresh_token', ''),
    'token_type': data.get('token_type', 'Bearer'),
    'scope': data.get('scope', ''),
    'saved_at': time.time(),
    'source': '$FOUND_TOKEN',
}

# Copy optional fields
for key in ['client_id', 'client_secret', 'expires_in', 'expires_at', 'quota_project_id']:
    if key in data:
        token[key] = data[key]

# Calculate expiry if needed
if 'expires_at' not in token and token.get('expires_in'):
    token['expires_at'] = time.time() + int(token['expires_in'])

with open('$OPDBUS_TOKEN_FILE', 'w') as f:
    json.dump(token, f, indent=2)

os.chmod('$OPDBUS_TOKEN_FILE', 0o600)
print(f"Token saved to: $OPDBUS_TOKEN_FILE")
PYEOF

echo ""
log_info "Token extracted successfully!"
echo ""
echo "To use with op-dbus:"
echo "  export GOOGLE_AUTH_TOKEN_FILE=$OPDBUS_TOKEN_FILE"
echo "  export LLM_PROVIDER=antigravity"
echo ""
echo "Or add to /etc/op-dbus/environment:"
echo "  GOOGLE_AUTH_TOKEN_FILE=$OPDBUS_TOKEN_FILE"
echo "  LLM_PROVIDER=antigravity"
echo ""
echo "Then restart op-web:"
echo "  sudo systemctl restart op-web"
