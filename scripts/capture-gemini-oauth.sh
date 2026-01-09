#!/usr/bin/env bash
#
# Capture OAuth Token from Gemini CLI for op-dbus
#
# Strategy:
#   1. Run Gemini CLI (or any Google auth tool)
#   2. Monitor for token file creation
#   3. If needs display, spin up Wayland, complete OAuth
#   4. Extract token once authenticated
#   5. Save for op-dbus LLM provider
#
# Usage:
#   ./capture-gemini-oauth.sh
#   ./capture-gemini-oauth.sh --force-display
#

set -euo pipefail

# =============================================================================
# CONFIGURATION
# =============================================================================

# Where we'll save the token for op-dbus
OPDBUS_TOKEN_FILE="${HOME}/.config/antigravity/token.json"

# Known locations where Google tools store OAuth tokens
GEMINI_TOKEN_PATHS=(
    "${HOME}/.gemini/oauth_token.json"
    "${HOME}/.config/gemini/credentials.json"
    "${HOME}/.config/gemini-cli/credentials.json"
    "${HOME}/.config/google-gemini/oauth.json"
    "${HOME}/.cache/gemini/auth.json"
    "${HOME}/.local/share/gemini/token.json"
)

# gcloud application default credentials (used by many Google tools)
GCLOUD_ADC_PATH="${HOME}/.config/gcloud/application_default_credentials.json"

# Commands that trigger Google OAuth
GEMINI_CLI_CMD="gemini"
GCLOUD_CMD="gcloud"

# Virtual display settings
WAYLAND_DISPLAY="antigravity-wayland"
TIMEOUT_SECS=300
FORCE_DISPLAY="${FORCE_DISPLAY:-false}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_step()  { echo -e "${BLUE}[STEP]${NC} $*"; }

# =============================================================================
# CLEANUP
# =============================================================================

WAYLAND_PID=""
GEMINI_PID=""
WATCH_PID=""

cleanup() {
    log_info "Cleaning up..."
    
    [[ -n "$WATCH_PID" ]] && kill "$WATCH_PID" 2>/dev/null || true
    [[ -n "$GEMINI_PID" ]] && kill "$GEMINI_PID" 2>/dev/null || true
    [[ -n "$WAYLAND_PID" ]] && kill "$WAYLAND_PID" 2>/dev/null || true
    
    rm -f "/tmp/token_captured" 2>/dev/null || true
}

trap cleanup EXIT

# =============================================================================
# TOKEN DETECTION
# =============================================================================

# Check if a valid token already exists
check_existing_token() {
    # Check our op-dbus token
    if [[ -f "$OPDBUS_TOKEN_FILE" ]]; then
        if python3 -c "
import json, time
with open('$OPDBUS_TOKEN_FILE') as f:
    t = json.load(f)
# Check for access_token and either no expiry or not expired
if 'access_token' in t:
    exp = t.get('expires_at', 0)
    if exp == 0 or time.time() < exp - 300:
        exit(0)
exit(1)
" 2>/dev/null; then
            log_info "Valid token already exists at $OPDBUS_TOKEN_FILE"
            return 0
        fi
    fi
    
    # Check Gemini CLI paths
    for path in "${GEMINI_TOKEN_PATHS[@]}"; do
        if [[ -f "$path" ]]; then
            log_info "Found existing Gemini token at $path"
            copy_token "$path"
            return 0
        fi
    done
    
    # Check gcloud ADC
    if [[ -f "$GCLOUD_ADC_PATH" ]]; then
        if grep -q 'refresh_token' "$GCLOUD_ADC_PATH" 2>/dev/null; then
            log_info "Found gcloud application_default_credentials.json"
            copy_token "$GCLOUD_ADC_PATH"
            return 0
        fi
    fi
    
    return 1
}

# Copy token to op-dbus location
copy_token() {
    local source="$1"
    mkdir -p "$(dirname "$OPDBUS_TOKEN_FILE")"
    
    # Try to normalize the token format
    python3 << PYEOF
import json
import time
import sys

try:
    with open('$source') as f:
        data = json.load(f)
    
    # Normalize to our format
    token = {
        'access_token': data.get('access_token', ''),
        'refresh_token': data.get('refresh_token', ''),
        'token_type': data.get('token_type', 'Bearer'),
        'expires_in': data.get('expires_in', 3600),
        'scope': data.get('scope', ''),
    }
    
    # Copy additional fields if present
    for key in ['client_id', 'client_secret', 'quota_project_id']:
        if key in data:
            token[key] = data[key]
    
    # Add our metadata
    token['saved_at'] = time.time()
    token['source'] = '$source'
    
    # Calculate expiry if not present
    if 'expires_at' not in token and token.get('expires_in'):
        token['expires_at'] = time.time() + int(token['expires_in'])
    
    with open('$OPDBUS_TOKEN_FILE', 'w') as f:
        json.dump(token, f, indent=2)
    
    print(f"Token saved to $OPDBUS_TOKEN_FILE")
    sys.exit(0)
except Exception as e:
    print(f"Error copying token: {e}", file=sys.stderr)
    sys.exit(1)
PYEOF
    
    chmod 600 "$OPDBUS_TOKEN_FILE"
    return $?
}

# Watch for token file creation
watch_for_token() {
    log_info "Watching for token files..."
    
    local start_time
    start_time=$(date +%s)
    
    while true; do
        # Check all known paths
        for path in "${GEMINI_TOKEN_PATHS[@]}" "$GCLOUD_ADC_PATH"; do
            if [[ -f "$path" ]]; then
                # Check if file was modified recently (within last 60 seconds)
                local mtime
                mtime=$(stat -c %Y "$path" 2>/dev/null || stat -f %m "$path" 2>/dev/null || echo 0)
                local now
                now=$(date +%s)
                if [[ $((now - mtime)) -lt 60 ]]; then
                    log_info "Token file created/updated: $path"
                    copy_token "$path"
                    touch /tmp/token_captured
                    return 0
                fi
            fi
        done
        
        # Check timeout
        local elapsed
        elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -gt $TIMEOUT_SECS ]]; then
            log_error "Timeout waiting for token"
            return 1
        fi
        
        sleep 1
    done
}

# =============================================================================
# GEMINI CLI METHODS
# =============================================================================

# Try to run Gemini CLI and trigger auth
run_gemini_cli() {
    log_step "Running Gemini CLI to trigger OAuth..."
    
    if ! command -v "$GEMINI_CLI_CMD" &>/dev/null; then
        log_warn "Gemini CLI not found, trying alternative methods"
        return 1
    fi
    
    # Start token watcher in background
    watch_for_token &
    WATCH_PID=$!
    
    # Run Gemini CLI with a simple prompt
    # The --no-sandbox might be needed for headless
    log_info "Starting Gemini CLI (will trigger OAuth if not authenticated)"
    
    # Run with timeout and capture output
    timeout 60 "$GEMINI_CLI_CMD" --help 2>&1 || true
    
    # If still no token file, try an actual query to force auth
    if [[ ! -f /tmp/token_captured ]]; then
        log_info "Trying interactive command to force auth..."
        echo "test" | timeout 120 "$GEMINI_CLI_CMD" 2>&1 || true
    fi
    
    # Wait for watcher
    wait $WATCH_PID 2>/dev/null || true
    WATCH_PID=""
    
    [[ -f /tmp/token_captured ]]
}

# Try gcloud auth application-default login
run_gcloud_auth() {
    log_step "Trying gcloud application-default login..."
    
    if ! command -v "$GCLOUD_CMD" &>/dev/null; then
        log_warn "gcloud not found"
        return 1
    fi
    
    # Start token watcher in background
    watch_for_token &
    WATCH_PID=$!
    
    # gcloud has --no-launch-browser for headless
    # This prints URL, user visits it, pastes code
    if $GCLOUD_CMD auth application-default login --no-launch-browser 2>&1; then
        log_info "gcloud auth completed"
    fi
    
    # Wait for watcher
    wait $WATCH_PID 2>/dev/null || true
    WATCH_PID=""
    
    [[ -f /tmp/token_captured ]] || [[ -f "$GCLOUD_ADC_PATH" ]]
}

# =============================================================================
# VIRTUAL DISPLAY METHOD
# =============================================================================

start_wayland_compositor() {
    log_step "Starting virtual Wayland compositor..."
    
    # Check for compositor
    local compositor=""
    if command -v cage &>/dev/null; then
        compositor="cage"
    elif command -v weston &>/dev/null; then
        compositor="weston"
    else
        log_error "No Wayland compositor found. Install cage or weston."
        return 1
    fi
    
    log_info "Using compositor: $compositor"
    
    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
    export WAYLAND_DISPLAY="$WAYLAND_DISPLAY"
    
    case "$compositor" in
        cage)
            # cage runs a single app - we'll run a shell
            cage -s -- sleep infinity &
            WAYLAND_PID=$!
            ;;
        weston)
            weston --socket="$WAYLAND_DISPLAY" --backend=headless-backend.so &
            WAYLAND_PID=$!
            ;;
    esac
    
    sleep 2
    
    if ! kill -0 "$WAYLAND_PID" 2>/dev/null; then
        log_error "Compositor failed to start"
        return 1
    fi
    
    log_info "Compositor running (PID: $WAYLAND_PID)"
    return 0
}

run_gemini_in_display() {
    log_step "Running Gemini CLI in virtual display..."
    
    if ! command -v "$GEMINI_CLI_CMD" &>/dev/null; then
        log_error "Gemini CLI not found"
        return 1
    fi
    
    # Start token watcher
    watch_for_token &
    WATCH_PID=$!
    
    # Run Gemini CLI in the virtual display
    export WAYLAND_DISPLAY="$WAYLAND_DISPLAY"
    export DISPLAY=""  # Prefer Wayland
    
    log_info "Starting Gemini CLI in virtual display..."
    
    # This will open browser in the virtual display for OAuth
    # User won't see it, but the auth might complete via device code
    echo "Hello, how are you?" | timeout 120 "$GEMINI_CLI_CMD" 2>&1 &
    GEMINI_PID=$!
    
    # Wait for either:
    # 1. Token file to appear
    # 2. Gemini CLI to exit
    # 3. Timeout
    local start_time
    start_time=$(date +%s)
    
    while kill -0 "$GEMINI_PID" 2>/dev/null; do
        if [[ -f /tmp/token_captured ]]; then
            log_info "Token captured!"
            kill "$GEMINI_PID" 2>/dev/null || true
            break
        fi
        
        local elapsed
        elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -gt $TIMEOUT_SECS ]]; then
            log_error "Timeout"
            kill "$GEMINI_PID" 2>/dev/null || true
            break
        fi
        
        sleep 1
    done
    
    # Wait for processes
    wait $GEMINI_PID 2>/dev/null || true
    wait $WATCH_PID 2>/dev/null || true
    GEMINI_PID=""
    WATCH_PID=""
    
    [[ -f /tmp/token_captured ]]
}

try_virtual_display() {
    log_step "Attempting OAuth with virtual display..."
    
    if ! start_wayland_compositor; then
        return 1
    fi
    
    if run_gemini_in_display; then
        return 0
    fi
    
    # If Gemini didn't work, try browser-based OAuth
    log_info "Trying browser OAuth in virtual display..."
    run_browser_oauth_in_display
}

run_browser_oauth_in_display() {
    # Similar to our earlier implementation
    # Use localhost callback server + browser in virtual display
    
    if [[ -z "${GOOGLE_CLIENT_ID:-}" ]]; then
        log_warn "GOOGLE_CLIENT_ID not set, cannot do browser OAuth"
        return 1
    fi
    
    export WAYLAND_DISPLAY="$WAYLAND_DISPLAY"
    
    # Start callback server
    python3 << 'PYEOF' &
import http.server
import urllib.parse
import json
import os
import sys
import time

class CallbackHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == '/callback':
            query = urllib.parse.parse_qs(parsed.query)
            if 'code' in query:
                code = query['code'][0]
                with open('/tmp/oauth_code.txt', 'w') as f:
                    f.write(code)
                self.send_response(200)
                self.send_header('Content-type', 'text/html')
                self.end_headers()
                self.wfile.write(b'<h1>Success!</h1>')
                sys.exit(0)
        self.send_response(404)
        self.end_headers()
    def log_message(self, format, *args):
        pass

server = http.server.HTTPServer(('localhost', 8085), CallbackHandler)
server.handle_request()
PYEOF
    local server_pid=$!
    
    # Build OAuth URL
    local redirect_uri="http://localhost:8085/callback"
    local scope="openid%20email%20profile"
    local auth_url="https://accounts.google.com/o/oauth2/v2/auth?client_id=${GOOGLE_CLIENT_ID}&redirect_uri=${redirect_uri}&response_type=code&scope=${scope}&access_type=offline&prompt=consent"
    
    # Find browser
    local browser=""
    for b in chromium chromium-browser google-chrome firefox; do
        if command -v "$b" &>/dev/null; then
            browser="$b"
            break
        fi
    done
    
    if [[ -z "$browser" ]]; then
        kill $server_pid 2>/dev/null || true
        return 1
    fi
    
    # Run browser in virtual display
    "$browser" --no-sandbox --disable-gpu "$auth_url" &
    local browser_pid=$!
    
    # Wait for code
    local start_time
    start_time=$(date +%s)
    while [[ ! -f /tmp/oauth_code.txt ]]; do
        sleep 1
        local elapsed
        elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -gt $TIMEOUT_SECS ]]; then
            kill $browser_pid 2>/dev/null || true
            kill $server_pid 2>/dev/null || true
            return 1
        fi
    done
    
    kill $browser_pid 2>/dev/null || true
    
    # Exchange code for token
    local auth_code
    auth_code=$(cat /tmp/oauth_code.txt)
    rm -f /tmp/oauth_code.txt
    
    local token_response
    token_response=$(curl -s -X POST 'https://oauth2.googleapis.com/token' \
        -d "client_id=${GOOGLE_CLIENT_ID}" \
        -d "client_secret=${GOOGLE_CLIENT_SECRET:-}" \
        -d "code=${auth_code}" \
        -d "redirect_uri=${redirect_uri}" \
        -d 'grant_type=authorization_code')
    
    if echo "$token_response" | jq -e '.access_token' >/dev/null 2>&1; then
        mkdir -p "$(dirname "$OPDBUS_TOKEN_FILE")"
        echo "$token_response" | jq ". + {saved_at: $(date +%s), expires_at: ($(date +%s) + (.expires_in // 3600))}" > "$OPDBUS_TOKEN_FILE"
        chmod 600 "$OPDBUS_TOKEN_FILE"
        touch /tmp/token_captured
        return 0
    fi
    
    return 1
}

# =============================================================================
# MAIN
# =============================================================================

main() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║     CAPTURE GEMINI CLI OAUTH TOKEN FOR OP-DBUS                 ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo ""
    
    # Parse args
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --force-display)
                FORCE_DISPLAY="true"
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [--force-display]"
                echo ""
                echo "Captures OAuth token from Gemini CLI for use with op-dbus."
                echo ""
                echo "Options:"
                echo "  --force-display  Skip CLI methods, use virtual display directly"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    # Check for existing valid token first
    if check_existing_token; then
        log_info "Using existing token"
        show_success
        exit 0
    fi
    
    # Method 1: Try running Gemini CLI directly (might work headless)
    if [[ "$FORCE_DISPLAY" != "true" ]]; then
        if run_gemini_cli; then
            show_success
            exit 0
        fi
        
        # Method 2: Try gcloud auth (has good headless support)
        if run_gcloud_auth; then
            # Copy gcloud ADC to our format
            if [[ -f "$GCLOUD_ADC_PATH" ]]; then
                copy_token "$GCLOUD_ADC_PATH"
            fi
            show_success
            exit 0
        fi
    fi
    
    # Method 3: Virtual display
    log_warn "CLI methods failed or skipped, trying virtual display..."
    
    if try_virtual_display; then
        show_success
        exit 0
    fi
    
    log_error "All methods failed to capture OAuth token"
    exit 1
}

show_success() {
    echo ""
    echo "════════════════════════════════════════════════════════════════"
    echo "✅ OAuth token captured!"
    echo ""
    echo "Token saved to: $OPDBUS_TOKEN_FILE"
    echo ""
    echo "To use with op-dbus:"
    echo "  export GOOGLE_AUTH_TOKEN_FILE=$OPDBUS_TOKEN_FILE"
    echo "  export LLM_PROVIDER=antigravity"
    echo "════════════════════════════════════════════════════════════════"
}

main "$@"
