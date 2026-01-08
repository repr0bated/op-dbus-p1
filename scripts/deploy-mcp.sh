#!/bin/bash
#
# Deploy MCP Server
# Builds and installs the unified MCP server with both Compact and Agents modes
#
# Usage: ./scripts/deploy-mcp.sh
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
INSTALL_DIR="/usr/local/sbin"
SYSTEMD_DIR="/etc/systemd/system"
MCP_CONFIG_DIR="/etc/mcp"

COMPACT_PORT="3001"
AGENTS_PORT="3002"

cd "$PROJECT_ROOT"

# Step 1: Build as current user (not root) to avoid permission issues
log_info "Building MCP server..."
if cargo build --release -p op-mcp; then
    log_success "Build complete"
else
    log_error "Build failed"
    exit 1
fi

# Check if binary exists
if [[ ! -f "target/release/op-mcp-server" ]]; then
    log_error "Binary not found: target/release/op-mcp-server"
    exit 1
fi

# Step 2: Require sudo for installation steps
if [[ $EUID -ne 0 ]]; then
    log_info "Sudo required for installation..."
    sudo -v || { log_error "Sudo authentication failed"; exit 1; }
fi

# Stop existing services
log_info "Stopping existing services..."
sudo systemctl stop op-mcp-compact 2>/dev/null || true
sudo systemctl stop op-mcp-agents 2>/dev/null || true

# Install binary
log_info "Installing binary..."
sudo cp target/release/op-mcp-server "$INSTALL_DIR/"
sudo chmod 755 "$INSTALL_DIR/op-mcp-server"
log_success "Installed: $INSTALL_DIR/op-mcp-server"

# Create compact service
log_info "Creating compact service (port $COMPACT_PORT)..."
sudo tee "$SYSTEMD_DIR/op-mcp-compact.service" > /dev/null <<EOF
[Unit]
Description=OP MCP Compact Server (4 meta-tools)
After=network.target dbus.service
Wants=dbus.service

[Service]
Type=simple
User=root
EnvironmentFile=-/etc/op-dbus/environment
ExecStart=$INSTALL_DIR/op-mcp-server --mode compact --http 0.0.0.0:$COMPACT_PORT --log-level info
Restart=always
RestartSec=5

NoNewPrivileges=true
ProtectSystem=strict
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

# Create agents service
log_info "Creating agents service (port $AGENTS_PORT)..."
sudo tee "$SYSTEMD_DIR/op-mcp-agents.service" > /dev/null <<EOF
[Unit]
Description=OP MCP Agents Server (run-on-connection)
After=network.target dbus.service
Wants=dbus.service

[Service]
Type=simple
User=root
EnvironmentFile=-/etc/op-dbus/environment
ExecStart=$INSTALL_DIR/op-mcp-server --mode agents --http 0.0.0.0:$AGENTS_PORT --log-level info
Restart=always
RestartSec=5

NoNewPrivileges=true
ProtectSystem=strict
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

# Deploy MCP config examples
log_info "Deploying MCP configuration examples..."
if [[ -d "$PROJECT_ROOT/deploy/config/examples" ]]; then
    sudo mkdir -p "$MCP_CONFIG_DIR"
    sudo cp "$PROJECT_ROOT/deploy/config/examples/"*.json "$MCP_CONFIG_DIR/" 2>/dev/null || true
    sudo cp "$PROJECT_ROOT/deploy/config/examples/README.md" "$MCP_CONFIG_DIR/" 2>/dev/null || true
    sudo chmod 644 "$MCP_CONFIG_DIR/"* 2>/dev/null || true
    log_success "MCP configs deployed to $MCP_CONFIG_DIR"
fi

# Reload and start
log_info "Starting services..."
sudo systemctl daemon-reload
sudo systemctl enable op-mcp-compact op-mcp-agents 2>/dev/null || true
sudo systemctl start op-mcp-compact op-mcp-agents

sleep 2

# Verify
echo ""
log_info "Service Status:"
for svc in op-mcp-compact op-mcp-agents; do
    if systemctl is-active --quiet "$svc"; then
        echo -e "  $svc: ${GREEN}running${NC}"
    else
        echo -e "  $svc: ${RED}failed${NC}"
        echo "    Last logs:"
        journalctl -u "$svc" -n 3 --no-pager 2>/dev/null | sed 's/^/    /'
    fi
done

echo ""
log_info "Endpoints:"
echo -e "  Compact:  ${YELLOW}http://localhost:$COMPACT_PORT${NC} (4 meta-tools)"
echo -e "  Agents:   ${YELLOW}http://localhost:$AGENTS_PORT${NC} (run-on-connection)"

echo ""
log_info "Available Agents:"
echo "  - rust_pro (cargo check/build/test/clippy)"
echo "  - backend_architect (design/review/suggest)"
echo "  - sequential_thinking (think/plan/analyze)"
echo "  - memory (remember/recall/forget)"
echo "  - context_manager (save/load/export)"

echo ""
log_info "MCP Configs installed to: $MCP_CONFIG_DIR"
ls -1 "$MCP_CONFIG_DIR/"*.json 2>/dev/null | sed 's/^/  /'

echo ""
log_info "Test commands:"
echo "  curl http://localhost:$COMPACT_PORT/health"
echo "  curl http://localhost:$AGENTS_PORT/health"
echo ""
log_success "MCP deployment complete!"
