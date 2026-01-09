#!/bin/bash
#
# Setup Antigravity IDE with Headless Wayland Display
#
# This creates:
# 1. Virtual Wayland compositor (cage) running Antigravity
# 2. VNC server (wayvnc) for remote access
# 3. Systemd services for automatic startup
#
# After setup:
# - Connect via VNC to log in once
# - Auth persists, Antigravity stays running
# - op-dbus can use the authenticated session
#
# Usage:
#   ./setup-antigravity-headless.sh [--user USER] [--vnc-port PORT]
#

set -euo pipefail

# =============================================================================
# CONFIGURATION
# =============================================================================

USER="${USER:-jeremy}"
VNC_PORT="5900"
WAYLAND_DISPLAY="antigravity-0"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

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
# PARSE ARGS
# =============================================================================

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user)
            USER="$2"
            shift 2
            ;;
        --vnc-port)
            VNC_PORT="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [--user USER] [--vnc-port PORT]"
            echo ""
            echo "Sets up Antigravity IDE with a headless Wayland display."
            echo ""
            echo "Options:"
            echo "  --user USER      User to run as (default: $USER)"
            echo "  --vnc-port PORT  VNC port (default: 5900)"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# =============================================================================
# CHECKS
# =============================================================================

if [[ $EUID -ne 0 ]]; then
    log_error "This script must be run as root"
    exit 1
fi

log_info "Setting up Antigravity headless display for user: $USER"

# =============================================================================
# INSTALL DEPENDENCIES
# =============================================================================

log_step "Installing dependencies..."

# Detect distro
if [[ -f /etc/debian_version ]]; then
    apt-get update
    apt-get install -y \
        cage \
        wayvnc \
        wlr-randr \
        libwayland-server0 \
        libwlroots11 \
        pixman \
        | tail -5
    log_info "Installed: cage, wayvnc, wlr-randr"
elif [[ -f /etc/fedora-release ]]; then
    dnf install -y cage wayvnc wlr-randr
elif [[ -f /etc/arch-release ]]; then
    pacman -S --noconfirm cage wayvnc wlr-randr
else
    log_warn "Unknown distro. Install manually: cage, wayvnc, wlr-randr"
fi

# Check for Antigravity
if ! command -v antigravity &>/dev/null; then
    log_warn "Antigravity not found in PATH"
    log_info "Please install Antigravity IDE from https://antigravity.google/"
    log_info "Or link: ln -s /path/to/antigravity /usr/bin/antigravity"
fi

# =============================================================================
# CREATE SYSTEMD SERVICES
# =============================================================================

log_step "Installing systemd services..."

# Get user's UID
USER_UID=$(id -u "$USER")
USER_GID=$(id -g "$USER")
USER_HOME=$(eval echo "~$USER")

# Create main service (cage + antigravity)
cat > /etc/systemd/system/antigravity-display.service << EOF
[Unit]
Description=Antigravity IDE with Virtual Wayland Display
After=network-online.target dbus.service
Wants=network-online.target
Documentation=https://antigravity.google/docs/home

[Service]
Type=simple
User=$USER
Group=$USER

# Environment for headless Wayland
Environment=XDG_RUNTIME_DIR=/run/user/$USER_UID
Environment=WAYLAND_DISPLAY=$WAYLAND_DISPLAY
Environment=WLR_BACKENDS=headless
Environment=WLR_LIBINPUT_NO_DEVICES=1
Environment=WLR_RENDERER=pixman
Environment=WLR_RENDERER_ALLOW_SOFTWARE=1
Environment=LIBGL_ALWAYS_SOFTWARE=1
Environment=XDG_SESSION_TYPE=wayland
Environment=HOME=$USER_HOME

# Electron/Antigravity settings
Environment=ELECTRON_DISABLE_GPU=1
Environment=ELECTRON_ENABLE_LOGGING=1
Environment=DISPLAY=

# cage runs single app in minimal Wayland compositor
# -s = allow VNC connections via socket
ExecStart=/usr/bin/cage -s -- /usr/bin/antigravity --disable-gpu --no-sandbox $USER_HOME/git/op-dbus-v2

Restart=always
RestartSec=10

# Ensure XDG_RUNTIME_DIR exists
ExecStartPre=/bin/mkdir -p /run/user/$USER_UID
ExecStartPre=/bin/chown $USER:$USER /run/user/$USER_UID
ExecStartPre=/bin/chmod 700 /run/user/$USER_UID

StandardOutput=journal
StandardError=journal
SyslogIdentifier=antigravity-display

[Install]
WantedBy=multi-user.target
EOF

# Create VNC service
cat > /etc/systemd/system/antigravity-vnc.service << EOF
[Unit]
Description=WayVNC for Antigravity Remote Access
After=antigravity-display.service
BindsTo=antigravity-display.service
PartOf=antigravity-display.service

[Service]
Type=simple
User=$USER
Group=$USER

Environment=XDG_RUNTIME_DIR=/run/user/$USER_UID
Environment=WAYLAND_DISPLAY=$WAYLAND_DISPLAY
Environment=HOME=$USER_HOME

# Wait for Wayland socket to appear
ExecStartPre=/bin/bash -c 'for i in \$(seq 1 30); do [ -S /run/user/$USER_UID/$WAYLAND_DISPLAY ] && exit 0; sleep 1; done; echo "Timeout waiting for Wayland socket"; exit 1'

# wayvnc - listen on all interfaces
ExecStart=/usr/bin/wayvnc 0.0.0.0 $VNC_PORT

Restart=always
RestartSec=5

StandardOutput=journal
StandardError=journal
SyslogIdentifier=antigravity-vnc

[Install]
WantedBy=multi-user.target
EOF

log_info "Created /etc/systemd/system/antigravity-display.service"
log_info "Created /etc/systemd/system/antigravity-vnc.service"

# =============================================================================
# CREATE MANAGEMENT SCRIPT
# =============================================================================

log_step "Creating management script..."

cat > /usr/local/bin/antigravity-ctl << 'SCRIPT'
#!/bin/bash
# Antigravity Headless Control Script

case "$1" in
    start)
        echo "Starting Antigravity display and VNC..."
        sudo systemctl start antigravity-display
        sudo systemctl start antigravity-vnc
        ;;
    stop)
        echo "Stopping Antigravity..."
        sudo systemctl stop antigravity-vnc
        sudo systemctl stop antigravity-display
        ;;
    restart)
        echo "Restarting Antigravity..."
        sudo systemctl restart antigravity-display
        sleep 3
        sudo systemctl restart antigravity-vnc
        ;;
    status)
        echo "=== Antigravity Display ==="
        systemctl status antigravity-display --no-pager -l | head -20
        echo ""
        echo "=== Antigravity VNC ==="
        systemctl status antigravity-vnc --no-pager -l | head -20
        ;;
    logs)
        journalctl -u antigravity-display -u antigravity-vnc -f
        ;;
    connect)
        echo "Connect with VNC client to: $(hostname -I | awk '{print $1}'):5900"
        echo "Or use: vncviewer $(hostname):5900"
        ;;
    *)
        echo "Usage: $0 {start|stop|restart|status|logs|connect}"
        exit 1
        ;;
esac
SCRIPT

chmod +x /usr/local/bin/antigravity-ctl
log_info "Created /usr/local/bin/antigravity-ctl"

# =============================================================================
# CONFIGURE FIREWALL
# =============================================================================

log_step "Configuring firewall..."

if command -v ufw &>/dev/null; then
    ufw allow $VNC_PORT/tcp comment 'Antigravity VNC' 2>/dev/null || true
    log_info "Opened port $VNC_PORT in UFW"
elif command -v firewall-cmd &>/dev/null; then
    firewall-cmd --permanent --add-port=$VNC_PORT/tcp 2>/dev/null || true
    firewall-cmd --reload 2>/dev/null || true
    log_info "Opened port $VNC_PORT in firewalld"
else
    log_warn "No firewall detected. Ensure port $VNC_PORT is accessible."
fi

# =============================================================================
# ENABLE AND START
# =============================================================================

log_step "Enabling services..."

systemctl daemon-reload
systemctl enable antigravity-display
systemctl enable antigravity-vnc

log_step "Starting services..."

systemctl start antigravity-display
sleep 5
systemctl start antigravity-vnc

# =============================================================================
# VERIFY
# =============================================================================

sleep 3

echo ""
echo "════════════════════════════════════════════════════════════════════"
log_info "Setup complete!"
echo ""

if systemctl is-active --quiet antigravity-display; then
    echo -e "  ${GREEN}✓${NC} antigravity-display is running"
else
    echo -e "  ${RED}✗${NC} antigravity-display failed"
    journalctl -u antigravity-display -n 5 --no-pager
fi

if systemctl is-active --quiet antigravity-vnc; then
    echo -e "  ${GREEN}✓${NC} antigravity-vnc is running on port $VNC_PORT"
else
    echo -e "  ${RED}✗${NC} antigravity-vnc failed"
    journalctl -u antigravity-vnc -n 5 --no-pager
fi

echo ""
echo "To connect:"
echo "  1. Use any VNC client"
echo "  2. Connect to: $(hostname -I | awk '{print $1}'):$VNC_PORT"
echo "  3. Log in to Antigravity with your Google account"
echo "  4. Auth persists - you only need to log in once!"
echo ""
echo "Commands:"
echo "  antigravity-ctl status   - Check status"
echo "  antigravity-ctl logs     - View logs"
echo "  antigravity-ctl restart  - Restart services"
echo "  antigravity-ctl connect  - Show connection info"
echo ""
echo "Once logged in, Antigravity's auth token is stored locally."
echo "op-dbus can then use the authenticated session."
echo "════════════════════════════════════════════════════════════════════"
