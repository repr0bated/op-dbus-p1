#!/usr/bin/env bash
#
# Antigravity Proxy Capture
#
# Strategy:
#   1. Start mitmproxy to intercept Antigravity's API calls
#   2. Run Antigravity IDE (with virtual Wayland if headless)
#   3. Capture the auth headers + token from API requests
#   4. Save for op-dbus to replay with same headers
#
# This captures:
#   - OAuth token (Authorization: Bearer xxx)
#   - IDE identification headers (X-Goog-IDE, User-Agent, etc.)
#   - API endpoints being used
#
# Usage:
#   ./antigravity-proxy-capture.sh
#   ./antigravity-proxy-capture.sh --headless
#

set -euo pipefail

# =============================================================================
# CONFIGURATION
# =============================================================================

PROXY_PORT="8888"
CAPTURE_DIR="${HOME}/.config/antigravity/captured"
TOKEN_FILE="${CAPTURE_DIR}/token.json"
HEADERS_FILE="${CAPTURE_DIR}/headers.json"
SESSION_FILE="${CAPTURE_DIR}/session.json"

HEADLESS="${HEADLESS:-false}"
WAYPIPE_MODE="false"
WAYLAND_DISPLAY="antigravity-capture"

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

PROXY_PID=""
WAYLAND_PID=""
ANTIGRAVITY_PID=""

cleanup() {
    log_info "Cleaning up..."
    [[ -n "$ANTIGRAVITY_PID" ]] && kill "$ANTIGRAVITY_PID" 2>/dev/null || true
    [[ -n "$WAYLAND_PID" ]] && kill "$WAYLAND_PID" 2>/dev/null || true
    [[ -n "$PROXY_PID" ]] && kill "$PROXY_PID" 2>/dev/null || true
}

trap cleanup EXIT

# =============================================================================
# MITMPROXY SETUP
# =============================================================================

check_mitmproxy() {
    if ! command -v mitmdump &>/dev/null; then
        log_error "mitmproxy not found"
        echo "Install with: pip install mitmproxy"
        echo "Or: apt install mitmproxy"
        exit 1
    fi
}

# Python script for mitmproxy to extract headers
create_capture_script() {
    mkdir -p "$CAPTURE_DIR"
    
    cat > "${CAPTURE_DIR}/capture_addon.py" << 'PYEOF'
"""
mitmproxy addon to capture Antigravity IDE API calls
"""
import json
import os
from datetime import datetime
from mitmproxy import http

CAPTURE_DIR = os.path.expanduser("~/.config/antigravity/captured")
os.makedirs(CAPTURE_DIR, exist_ok=True)

# Domains we care about
TARGET_DOMAINS = [
    "generativelanguage.googleapis.com",  # Gemini API
    "aiplatform.googleapis.com",           # Vertex AI
    "oauth2.googleapis.com",               # OAuth
    "accounts.google.com",                 # Auth
    "www.googleapis.com",                  # Google APIs
    "cloudaicompanion.googleapis.com",     # Code Assist
    "firebaseappcheck.googleapis.com",
]

# Headers we want to capture (case-insensitive matching)
IMPORTANT_HEADERS = [
    "authorization",
    "x-goog-api-key",
    "x-goog-api-client",
    "x-goog-user-project",
    "x-google-api-key",
    "x-client-data",
    "x-goog-request-reason",
    "x-goog-fieldmask",
    "x-goog-ide",
    "x-ide-",
    "x-antigravity",
    "x-client-version",
    "user-agent",
    "origin",
    "referer",
]

captured_data = {
    "tokens": [],
    "headers": {},
    "endpoints": [],
    "requests": [],
}

def save_capture():
    """Save captured data to files"""
    # Save full session
    with open(os.path.join(CAPTURE_DIR, "session.json"), "w") as f:
        json.dump(captured_data, f, indent=2, default=str)
    
    # Save latest headers
    if captured_data["headers"]:
        with open(os.path.join(CAPTURE_DIR, "headers.json"), "w") as f:
            json.dump(captured_data["headers"], f, indent=2)
    
    # Save latest token
    if captured_data["tokens"]:
        latest_token = captured_data["tokens"][-1]
        with open(os.path.join(CAPTURE_DIR, "token.json"), "w") as f:
            json.dump(latest_token, f, indent=2)

def is_target_domain(host: str) -> bool:
    """Check if this is a domain we want to capture"""
    return any(domain in host for domain in TARGET_DOMAINS)

def is_important_header(name: str) -> bool:
    """Check if this header is important to capture"""
    name_lower = name.lower()
    return any(h in name_lower for h in IMPORTANT_HEADERS)

class AntigravityCapture:
    def request(self, flow: http.HTTPFlow) -> None:
        host = flow.request.pretty_host
        
        if not is_target_domain(host):
            return
        
        # Capture important headers
        important_headers = {}
        for name, value in flow.request.headers.items():
            if is_important_header(name):
                important_headers[name] = value
                
                # Special handling for Authorization header
                if name.lower() == "authorization" and value.startswith("Bearer "):
                    token = value[7:]  # Remove "Bearer " prefix
                    token_info = {
                        "access_token": token,
                        "captured_at": datetime.now().isoformat(),
                        "endpoint": flow.request.pretty_url,
                        "headers": important_headers.copy(),
                    }
                    captured_data["tokens"].append(token_info)
                    print(f"[CAPTURED] OAuth token from {host}")
        
        # Store headers (merge with existing)
        captured_data["headers"].update(important_headers)
        
        # Log the endpoint
        endpoint_info = {
            "url": flow.request.pretty_url,
            "method": flow.request.method,
            "headers": important_headers,
            "timestamp": datetime.now().isoformat(),
        }
        captured_data["endpoints"].append(endpoint_info)
        captured_data["requests"].append(endpoint_info)
        
        print(f"[INTERCEPT] {flow.request.method} {host}{flow.request.path}")
        for h, v in important_headers.items():
            # Truncate long values for display
            display_v = v[:50] + "..." if len(v) > 50 else v
            print(f"  {h}: {display_v}")
        
        # Save after each request
        save_capture()

    def response(self, flow: http.HTTPFlow) -> None:
        host = flow.request.pretty_host
        
        if not is_target_domain(host):
            return
        
        # Check for token in response (OAuth token exchange)
        if "oauth2" in host or "token" in flow.request.path:
            try:
                content = flow.response.get_text()
                if content:
                    data = json.loads(content)
                    if "access_token" in data:
                        token_info = {
                            "access_token": data.get("access_token"),
                            "refresh_token": data.get("refresh_token"),
                            "expires_in": data.get("expires_in"),
                            "token_type": data.get("token_type"),
                            "scope": data.get("scope"),
                            "captured_at": datetime.now().isoformat(),
                            "source": "oauth_response",
                        }
                        captured_data["tokens"].append(token_info)
                        print(f"[CAPTURED] OAuth token from response")
                        save_capture()
            except (json.JSONDecodeError, Exception) as e:
                pass

addons = [AntigravityCapture()]
PYEOF
    
    log_info "Created capture addon at ${CAPTURE_DIR}/capture_addon.py"
}

start_mitmproxy() {
    log_step "Starting mitmproxy on port $PROXY_PORT..."
    
    # Start mitmdump with our capture addon
    mitmdump \
        --listen-host 127.0.0.1 \
        --listen-port "$PROXY_PORT" \
        --set block_global=false \
        --ssl-insecure \
        -s "${CAPTURE_DIR}/capture_addon.py" \
        2>&1 | tee "${CAPTURE_DIR}/proxy.log" &
    
    PROXY_PID=$!
    
    sleep 2
    
    if ! kill -0 "$PROXY_PID" 2>/dev/null; then
        log_error "mitmproxy failed to start"
        exit 1
    fi
    
    log_info "mitmproxy running (PID: $PROXY_PID)"
    
    # Export proxy settings for child processes
    export http_proxy="http://127.0.0.1:${PROXY_PORT}"
    export https_proxy="http://127.0.0.1:${PROXY_PORT}"
    export HTTP_PROXY="http://127.0.0.1:${PROXY_PORT}"
    export HTTPS_PROXY="http://127.0.0.1:${PROXY_PORT}"
    
    # For Electron apps (Antigravity uses Electron)
    export ELECTRON_GET_USE_PROXY=1
}

# =============================================================================
# WAYLAND SETUP (for headless)
# =============================================================================

start_wayland_compositor() {
    log_step "Starting virtual Wayland compositor..."
    
    local compositor=""
    if [[ "$HEADLESS" == "true" ]] && command -v weston &>/dev/null; then
        compositor="weston"
    elif command -v cage &>/dev/null; then
        compositor="cage"
    elif command -v weston &>/dev/null; then
        compositor="weston"
    else
        log_error "No Wayland compositor found. Install cage or weston."
        exit 1
    fi
    
    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
    export WAYLAND_DISPLAY="$WAYLAND_DISPLAY"
    
    case "$compositor" in
        cage)
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
        exit 1
    fi
    
    log_info "Compositor running (PID: $WAYLAND_PID)"
}

# =============================================================================
# ANTIGRAVITY LAUNCH
# =============================================================================

find_antigravity() {
    # Common locations for Antigravity IDE
    local paths=(
        "/opt/antigravity/antigravity"
        "/usr/bin/antigravity"
        "/usr/local/bin/antigravity"
        "${HOME}/.local/bin/antigravity"
        "${HOME}/Applications/Antigravity/antigravity"
        "/Applications/Antigravity.app/Contents/MacOS/Antigravity"
    )
    
    for path in "${paths[@]}"; do
        if [[ -x "$path" ]]; then
            echo "$path"
            return 0
        fi
    done
    
    # Try which
    if command -v antigravity &>/dev/null; then
        command -v antigravity
        return 0
    fi
    
    return 1
}

launch_antigravity() {
    log_step "Launching Antigravity IDE..."
    
    local antigravity_bin
    antigravity_bin=$(find_antigravity || true)
    
    if [[ -z "$antigravity_bin" ]]; then
        log_error "Antigravity IDE not found"
        echo "Please install Antigravity IDE or set the path manually"
        exit 1
    fi
    
    log_info "Found Antigravity at: $antigravity_bin"
    
    # Set up environment for proxy
    export http_proxy="http://127.0.0.1:${PROXY_PORT}"
    export https_proxy="http://127.0.0.1:${PROXY_PORT}"
    export HTTP_PROXY="http://127.0.0.1:${PROXY_PORT}"
    export HTTPS_PROXY="http://127.0.0.1:${PROXY_PORT}"
    
    # Electron-specific proxy settings
    export ELECTRON_GET_USE_PROXY=1
    
    # Disable certificate verification for mitmproxy (development only!)
    export NODE_TLS_REJECT_UNAUTHORIZED=0
    
    if [[ "$HEADLESS" == "true" ]]; then
        export WAYLAND_DISPLAY="$WAYLAND_DISPLAY"
        export DISPLAY=""  # Prefer Wayland
        export XDG_SESSION_TYPE="wayland"
    fi
    
    log_info "Starting Antigravity with proxy settings..."
    
    if [[ "$HEADLESS" == "true" ]] && command -v dbus-run-session &>/dev/null; then
        dbus-run-session -- "$antigravity_bin" &
    else
        "$antigravity_bin" &
    fi
    ANTIGRAVITY_PID=$!
    
    log_info "Antigravity running (PID: $ANTIGRAVITY_PID)"
}

# =============================================================================
# MONITORING
# =============================================================================

wait_for_capture() {
    log_step "Waiting for token capture..."
    
    echo ""
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║  ANTIGRAVITY PROXY CAPTURE RUNNING                             ║"
    echo "╠════════════════════════════════════════════════════════════════╣"
    echo "║                                                                ║"
    echo "║  1. Sign in to Antigravity IDE when it opens                   ║"
    echo "║  2. Make a request (ask it something)                          ║"
    echo "║  3. Token and headers will be captured automatically          ║"
    echo "║                                                                ║"
    echo "║  Captured data saved to:                                       ║"
    echo "║    ${CAPTURE_DIR}/                                             "
    echo "║                                                                ║"
    echo "║  Press Ctrl+C when done                                        ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo ""
    
    # Monitor for token capture
    local start_time
    start_time=$(date +%s)
    local timeout=600  # 10 minutes
    
    while true; do
        if [[ -f "$TOKEN_FILE" ]]; then
            log_info "Token captured!"
            
            # Display captured info
            echo ""
            echo "════════════════════════════════════════════════════════════════"
            echo "✅ CAPTURE SUCCESSFUL"
            echo ""
            echo "Token file: $TOKEN_FILE"
            echo "Headers file: $HEADERS_FILE"
            echo "Session file: $SESSION_FILE"
            echo ""
            
            if [[ -f "$HEADERS_FILE" ]]; then
                echo "Captured headers:"
                jq -r 'to_entries[] | "  \(.key): \(.value[:50])..."' "$HEADERS_FILE" 2>/dev/null || cat "$HEADERS_FILE"
            fi
            
            echo "════════════════════════════════════════════════════════════════"
            break
        fi
        
        # Check if Antigravity is still running
        if [[ -n "$ANTIGRAVITY_PID" ]] && ! kill -0 "$ANTIGRAVITY_PID" 2>/dev/null; then
            log_warn "Antigravity exited"
            break
        fi
        
        # Check timeout
        local elapsed
        elapsed=$(($(date +%s) - start_time))
        if [[ $elapsed -gt $timeout ]]; then
            log_warn "Timeout waiting for capture"
            break
        fi
        
        sleep 2
    done
}

# =============================================================================
# MAIN
# =============================================================================

main() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║     ANTIGRAVITY IDE PROXY CAPTURE                              ║"
    echo "║     Capture OAuth token + IDE headers for op-dbus              ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo ""
    
    # Parse args
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --headless)
                HEADLESS="true"
                shift
                ;;
            --waypipe)
                HEADLESS="true"
                WAYPIPE_MODE="true"
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [--headless] [--waypipe]"
                echo ""
                echo "Captures OAuth token and headers from Antigravity IDE."
                echo ""
                echo "Options:"
                echo "  --headless  Run with virtual Wayland display"
                echo "  --waypipe   Start headless Wayland and wait for a waypipe client"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    # Check requirements
    check_mitmproxy
    
    # Create capture directory
    mkdir -p "$CAPTURE_DIR"
    
    # Create mitmproxy addon
    create_capture_script
    
    # Start mitmproxy
    start_mitmproxy
    
    # Start Wayland if headless
    if [[ "$HEADLESS" == "true" ]]; then
        start_wayland_compositor
    fi
    
    # Launch Antigravity
    if [[ "$WAYPIPE_MODE" == "true" ]]; then
        local host
        host="$(hostname -f 2>/dev/null || hostname)"
        log_info "Waypipe mode enabled. Launch Antigravity from your local machine:"
        echo "  waypipe ssh ${USER}@${host} WAYLAND_DISPLAY=${WAYLAND_DISPLAY} antigravity"
        echo ""
        echo "Once it opens, sign in and make a request to capture tokens."
    else
        launch_antigravity
    fi
    
    # Wait for capture
    wait_for_capture
    
    echo ""
    echo "To use captured credentials with op-dbus:"
    echo "  export ANTIGRAVITY_SESSION_FILE=$SESSION_FILE"
    echo "  export LLM_PROVIDER=antigravity"
}

main "$@"
